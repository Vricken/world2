use super::super::*;
use crate::mesh_topology::{
    canonical_chunk_topology, SAMPLED_VERTICES_PER_EDGE, STITCH_VARIANT_COUNT,
    VISIBLE_VERTICES_PER_EDGE,
};
use crate::runtime::{
    CURRENT_IMPLEMENTED_PHASE, CURRENT_IMPLEMENTED_PHASE_LABEL,
    DEFAULT_MIN_AVERAGE_CHUNK_SURFACE_SPAN_METERS, NEXT_PHASE_LABEL,
};
use crate::topology::{DEFAULT_MAX_LOD, DIRECTED_EDGE_TRANSFORM_COUNT, MAX_SUPPORTED_MAX_LOD};
use godot::classes::Engine;

impl PlanetRoot {
    pub(crate) fn init_state(base: Base<Node3D>) -> Self {
        let runtime = PlanetRuntime::default();
        Self {
            base,
            cached_scenario_rid: None,
            cached_physics_space_rid: None,
            planet_radius: runtime.config.planet_radius,
            terrain_height_amplitude: runtime.config.height_amplitude,
            sea_level_meters: runtime.config.sea_level_meters,
            water_enabled: runtime.config.water_enabled,
            hill_strength: runtime.config.hill_strength,
            mountain_strength: runtime.config.mountain_strength,
            mountain_frequency: runtime.config.mountain_frequency,
            atmosphere_height: 0.2,
            frustum_culling_enabled: runtime.config.enable_frustum_culling,
            keep_coarse_lod_chunks_rendered: runtime.config.keep_coarse_lod_chunks_rendered,
            debug_force_server_pool_render_backend: false,
            debug_show_runtime_hud: true,
            debug_origin_rebase_count: 0,
            runtime_debug_canvas: None,
            runtime_debug_label: None,
            runtime,
            runtime_tick_count: 0,
            editor_preview_radius_applied: -1.0,
            editor_preview: None,
        }
    }

    pub(crate) fn ready_impl(&mut self) {
        if self.is_editor_context() {
            self.base_mut().set_process(true);
            self.base_mut().set_physics_process(false);
            self.sync_atmosphere_settings();
            self.sync_editor_preview();
            return;
        }

        self.remove_runtime_preview_node();
        self.sync_atmosphere_settings();
        self.base_mut().set_process(true);
        self.base_mut().set_physics_process(true);
        self.ensure_runtime_debug_hud();
        self.sync_runtime_scale_bootstrap();
        self.cache_world_rids();
        self.rebuild_runtime();
        self.apply_runtime_origin_shift();

        let seam = self.runtime.seam_debug_snapshot();
        let assets = self.runtime.asset_debug_snapshot();
        let build_order = self.runtime.build_order_summary();
        let strategies = self.runtime.strategy_summary();

        godot_print!(
            "PlanetRoot ready. {} active. chunks_in_scene_tree=false cached_world_rids={} origin_mode={} large_world_coordinates={} origin_recenter_distance={} planet_radius={} runtime_max_lod={} runtime_max_lod_cap={} metadata_precompute_max_lod={} dense_metadata_prebuild_max_lod={} payload_precompute_max_lod={} selection_reference_height_px={} target_render_chunks={} hard_render_chunk_cap={} sparse_meta={} min_avg_chunk_span_m={} worker_threads={} prebuilt_meta={} edge_xforms={} topology_default_max_lod={} topology_supported_max_lod={} visible_edge_verts={} sampled_edge_verts={} stitch_variants={} base_index_count={} planet_seed={} asset_cells_per_axis={} asset_group_span={} active_asset_groups={} active_asset_instances={} active_stitch_masks={} pooled_stitch_masks={} build_order_summary={} strategy_summary={} next_phase={}",
            CURRENT_IMPLEMENTED_PHASE_LABEL,
            self.cached_scenario_rid.is_some() && self.cached_physics_space_rid.is_some(),
            self.runtime.origin_mode_label(),
            self.runtime.config.use_large_world_coordinates,
            self.runtime.config.origin_recenter_distance,
            self.runtime.config.planet_radius,
            self.runtime.config.max_lod,
            self.runtime.config.max_lod_cap,
            self.runtime.metadata_precompute_max_lod(),
            self.runtime.dense_metadata_prebuild_max_lod(),
            self.runtime.payload_precompute_max_lod(),
            self.runtime.config.render_lod_reference_height_px,
            self.runtime.config.target_render_chunks,
            self.runtime.config.hard_render_chunk_cap,
            self.runtime.sparse_meta_count(),
            DEFAULT_MIN_AVERAGE_CHUNK_SURFACE_SPAN_METERS,
            self.runtime.worker_thread_count(),
            self.runtime.meta_count(),
            DIRECTED_EDGE_TRANSFORM_COUNT,
            DEFAULT_MAX_LOD,
            MAX_SUPPORTED_MAX_LOD,
            VISIBLE_VERTICES_PER_EDGE,
            SAMPLED_VERTICES_PER_EDGE,
            STITCH_VARIANT_COUNT,
            canonical_chunk_topology().base_indices().len(),
            self.runtime.config.planet_seed,
            self.runtime.config.asset_placement_cells_per_axis,
            self.runtime.config.asset_group_chunk_span,
            assets.active_groups,
            assets.active_instances,
            seam.active_stitch_mask_summary(),
            seam.pooled_stitch_mask_summary(),
            build_order,
            strategies,
            NEXT_PHASE_LABEL
        );
    }

    pub(crate) fn exit_tree_impl(&mut self) {
        if self.is_editor_context() {
            self.teardown_editor_preview();
            self.base_mut().set_process(false);
            self.base_mut().set_physics_process(false);
            return;
        }
        self.teardown_runtime_debug_hud();
        self.runtime.release_server_resources();
        self.base_mut().set_process(false);
        self.base_mut().set_physics_process(false);
    }

    pub(crate) fn process_impl(&mut self) {
        if self.is_editor_context() {
            self.sync_atmosphere_settings();
            self.sync_editor_preview();
            return;
        }

        self.sync_atmosphere_settings();
        self.runtime_tick_count = self.runtime_tick_count.saturating_add(1);

        if self.runtime_tick_count == 1 || self.runtime_tick_count % 300 == 0 {
            self.cache_world_rids();
        }

        self.sync_runtime_camera_clip();
        self.update_runtime_debug_hud();

        let Some(camera_state) = self.acquire_camera_state() else {
            if self.runtime_tick_count == 1 {
                godot_warn!(
                    "PlanetRoot could not find an active Camera3D; skipping Phase {} runtime tick.",
                    CURRENT_IMPLEMENTED_PHASE
                );
            }
            return;
        };

        if let Err(err) = self.runtime.step_visibility_selection(&camera_state) {
            godot_error!(
                "PlanetRoot Phase {} runtime tick failed: {err:?}",
                CURRENT_IMPLEMENTED_PHASE
            );
            return;
        }

        if self.runtime_tick_count == 1 || self.runtime_tick_count % 120 == 0 {
            self.log_runtime_frame_summary();
        }
    }

    pub(crate) fn physics_process_impl(&mut self) {
        if self.is_editor_context() {
            return;
        }

        self.sync_runtime_origin_shift();
    }
    pub(super) fn is_editor_context(&self) -> bool {
        Engine::singleton().is_editor_hint()
    }

    fn log_runtime_frame_summary(&self) {
        let frame = self.runtime.frame_state();
        let seam = self.runtime.seam_debug_snapshot();
        let assets = self.runtime.asset_debug_snapshot();
        let strategies = self.runtime.strategy_summary();
        godot_print!(
            "PlanetRoot phase{} tick={} meta={} sparse_meta={} payloads={} gpu_tile_upload_bytes={} gpu_material_binds={} active_gpu_render_chunks={} canonical_render_meshes={} render_residency={} render_residency_evictions={} render_tile_bytes={} render_tile_slots={} render_tile_active_slots={} render_tile_free_slots={} render_tile_eviction_ready_slots={} collision_residency={} collision_residency_bytes={} selected_render_starved={} selected_render_starvation_failures={} selected_render_starvation_frames={} desired_render={} active_render={} desired_physics={} active_physics={} horizon={} frustum={} selected_candidates={} refinement_iterations={} selection_cap_hits={} fullscreen_lod_bias=none selection_reference_height_px={} target_render_chunks={} hard_render_chunk_cap={} neighbor_splits={} sampled={} meshed={} packed={} staged={} commit_payloads={} warm_current={} warm_pool={} cold={} render_warm_current_commits={} render_warm_pool_commits={} render_cold_commits={} physics_commits={} meta_submitted={} meta_installed={} fallback_missing_current={} fallback_incompatible_current={} fallback_no_pool={} worker_threads={} worker_submitted={} worker_jobs={} worker_ready={} worker_stale={} worker_superseded={} worker_inflight={} worker_queue_peak={} worker_waits={} sample_scratch_reuse={} mesh_scratch_reuse={} pack_scratch_reuse={} scratch_growth={} origin_rebases={} render_rebinds={} physics_rebinds={} origin_mode={} render_pool_entries={} physics_pool_entries={} asset_payload_chunks={} asset_candidates={} asset_rejected={} asset_accepted={} active_asset_groups={} active_asset_instances={} asset_family_meshes={} active_stitched_chunks={} active_stitch_masks={} stitched_edges={} pooled_stitch_masks={} pending_seam_mismatches={} missing_active_surface_classes={} queued_ops={} deferred_ops={} deferred_upload_bytes={} starvation_frames={} build_order_steps={} strategy_summary={} next_phase={}",
            CURRENT_IMPLEMENTED_PHASE,
            frame.tick,
            self.runtime.meta_count(),
            frame.sparse_meta_entries,
            self.runtime.resident_payload_count(),
            frame.gpu_tile_upload_bytes,
            frame.gpu_material_binds,
            frame.active_gpu_render_chunks,
            frame.canonical_render_meshes,
            frame.render_residency_entries,
            frame.render_residency_evictions,
            frame.render_tile_bytes,
            frame.render_tile_pool_slots,
            frame.render_tile_pool_active_slots,
            frame.render_tile_pool_free_slots,
            frame.render_tile_eviction_ready_slots,
            frame.collision_residency_entries,
            frame.collision_residency_bytes,
            frame.selected_render_starved_chunks,
            frame.selected_render_starvation_failures,
            frame.max_selected_render_starvation_frames,
            frame.desired_render_count,
            self.runtime.active_render_count(),
            frame.desired_physics_count,
            self.runtime.active_physics_count(),
            frame.horizon_survivor_count,
            frame.frustum_survivor_count,
            frame.selected_candidates,
            frame.refinement_iterations,
            frame.selection_cap_hits,
            self.runtime.config.render_lod_reference_height_px,
            self.runtime.config.target_render_chunks,
            self.runtime.config.hard_render_chunk_cap,
            frame.neighbor_split_count,
            frame.phase7_sampled_chunks,
            frame.phase7_meshed_chunks,
            frame.phase7_packed_chunks,
            frame.phase7_staged_chunks,
            frame.phase7_commit_payloads,
            frame.phase7_warm_current_reuse_hits,
            frame.phase7_warm_pool_reuse_hits,
            frame.phase7_cold_fallbacks,
            frame.phase8_render_warm_current_commits,
            frame.phase8_render_warm_pool_commits,
            frame.phase8_render_cold_commits,
            frame.phase8_physics_commits,
            frame.phase9_meta_requests_submitted,
            frame.phase9_meta_results_installed,
            frame.phase8_fallback_missing_current_surface_class,
            frame.phase8_fallback_incompatible_current_surface_class,
            frame.phase8_fallback_no_compatible_pooled_surface,
            frame.phase9_worker_threads,
            frame.phase9_submitted_jobs,
            frame.phase9_generation_jobs,
            frame.phase9_ready_results,
            frame.phase9_stale_results_dropped,
            frame.phase9_superseded_jobs,
            frame.phase9_inflight_jobs,
            frame.phase9_queue_peak,
            frame.phase9_result_wait_count,
            frame.phase9_sample_scratch_reuse_hits,
            frame.phase9_mesh_scratch_reuse_hits,
            frame.phase9_pack_scratch_reuse_hits,
            frame.phase9_scratch_growth_events,
            frame.phase10_origin_rebases,
            frame.phase10_render_transform_rebinds,
            frame.phase10_physics_transform_rebinds,
            self.runtime.origin_mode_label(),
            frame.render_pool_entries,
            frame.physics_pool_entries,
            frame.phase12_chunks_with_asset_payloads,
            frame.phase12_asset_candidate_count,
            frame.phase12_asset_rejected_count,
            frame.phase12_asset_accepted_count,
            frame.phase12_active_groups,
            frame.phase12_active_instances,
            assets.family_meshes,
            seam.active_stitched_chunks,
            seam.active_stitch_mask_summary(),
            seam.active_stitched_edge_summary(),
            seam.pooled_stitch_mask_summary(),
            seam.pending_surface_class_mismatch_chunks,
            seam.active_chunks_missing_surface_class,
            frame.queued_commit_ops,
            frame.deferred_commit_ops,
            frame.upload_bytes_deferred,
            frame.max_deferred_starvation_frames,
            PlanetRuntime::build_order_stage_count(),
            strategies,
            NEXT_PHASE_LABEL,
        );
    }
}
