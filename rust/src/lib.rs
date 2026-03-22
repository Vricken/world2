pub mod geometry;
pub mod mesh_topology;
pub mod runtime;
pub mod topology;

use godot::classes::{INode3D, Node3D};
use godot::prelude::*;
use mesh_topology::{
    canonical_chunk_topology, SAMPLED_VERTICES_PER_EDGE, STITCH_VARIANT_COUNT,
    VISIBLE_VERTICES_PER_EDGE,
};
use runtime::{CameraState, PlanetRuntime};
use topology::{DEFAULT_MAX_LOD, DIRECTED_EDGE_TRANSFORM_COUNT};

#[derive(GodotClass)]
#[class(base = Node3D)]
pub struct PlanetRoot {
    base: Base<Node3D>,
    cached_scenario_rid: Option<Rid>,
    cached_physics_space_rid: Option<Rid>,
    runtime: PlanetRuntime,
    runtime_tick_count: u64,
}

#[godot_api]
impl INode3D for PlanetRoot {
    fn init(base: Base<Node3D>) -> Self {
        Self {
            base,
            cached_scenario_rid: None,
            cached_physics_space_rid: None,
            runtime: PlanetRuntime::default(),
            runtime_tick_count: 0,
        }
    }

    fn ready(&mut self) {
        self.base_mut().set_process(true);
        self.cache_world_rids();
        self.apply_runtime_origin_shift();
        let seam = self.runtime.seam_debug_snapshot();
        let assets = self.runtime.asset_debug_snapshot();

        godot_print!(
            "PlanetRoot ready. Phase 13 runtime active. chunks_in_scene_tree=false cached_world_rids={} origin_mode={} large_world_coordinates={} origin_recenter_distance={} meta_precompute_max_lod={} payload_precompute_max_lod={} worker_threads={} prebuilt_meta={} edge_xforms={} default_max_lod={} visible_edge_verts={} sampled_edge_verts={} stitch_variants={} base_index_count={} planet_seed={} asset_cells_per_axis={} asset_group_span={} active_asset_groups={} active_asset_instances={} active_stitch_masks={} pooled_stitch_masks={}",
            self.has_cached_world_rids(),
            self.runtime.origin_mode_label(),
            self.runtime.config.use_large_world_coordinates,
            self.runtime.config.origin_recenter_distance,
            self.runtime.metadata_precompute_max_lod(),
            self.runtime.payload_precompute_max_lod(),
            self.runtime.worker_thread_count(),
            self.runtime.meta_count(),
            DIRECTED_EDGE_TRANSFORM_COUNT,
            DEFAULT_MAX_LOD,
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
            seam.pooled_stitch_mask_summary()
        );
    }

    fn exit_tree(&mut self) {
        self.runtime.release_server_resources();
        self.base_mut().set_process(false);
    }

    fn process(&mut self, _delta: f64) {
        self.runtime_tick_count = self.runtime_tick_count.saturating_add(1);

        if self.runtime_tick_count == 1 || self.runtime_tick_count % 300 == 0 {
            self.cache_world_rids();
        }

        let Some(camera_state) = self.acquire_camera_state() else {
            if self.runtime_tick_count == 1 {
                godot_warn!(
                    "PlanetRoot could not find an active Camera3D; skipping Phase 13 runtime tick."
                );
            }
            return;
        };

        if let Err(err) = self.runtime.step_visibility_selection(&camera_state) {
            godot_error!("PlanetRoot Phase 13 runtime tick failed: {err:?}");
            return;
        }

        if self.runtime_tick_count == 1 || self.runtime_tick_count % 120 == 0 {
            let frame = self.runtime.frame_state();
            let seam = self.runtime.seam_debug_snapshot();
            let assets = self.runtime.asset_debug_snapshot();
            godot_print!(
                "PlanetRoot phase13 tick={} meta={} payloads={} desired_render={} active_render={} desired_physics={} active_physics={} horizon={} frustum={} neighbor_splits={} sampled={} meshed={} packed={} staged={} commit_payloads={} warm_current={} warm_pool={} cold={} render_warm_current_commits={} render_warm_pool_commits={} render_cold_commits={} physics_commits={} fallback_missing_current={} fallback_incompatible_current={} fallback_no_pool={} worker_threads={} worker_jobs={} worker_queue_peak={} worker_waits={} sample_scratch_reuse={} mesh_scratch_reuse={} pack_scratch_reuse={} scratch_growth={} origin_rebases={} render_rebinds={} physics_rebinds={} origin_mode={} render_pool_entries={} physics_pool_entries={} asset_payload_chunks={} asset_candidates={} asset_rejected={} asset_accepted={} active_asset_groups={} active_asset_instances={} asset_family_meshes={} active_stitched_chunks={} active_stitch_masks={} stitched_edges={} pooled_stitch_masks={} pending_seam_mismatches={} missing_active_surface_classes={} queued_ops={} deferred_ops={} deferred_upload_bytes={} starvation_frames={}",
                frame.tick,
                self.runtime.meta_count(),
                self.runtime.resident_payload_count(),
                frame.desired_render_count,
                self.runtime.active_render_count(),
                frame.desired_physics_count,
                self.runtime.active_physics_count(),
                frame.horizon_survivor_count,
                frame.frustum_survivor_count,
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
                frame.phase8_fallback_missing_current_surface_class,
                frame.phase8_fallback_incompatible_current_surface_class,
                frame.phase8_fallback_no_compatible_pooled_surface,
                frame.phase9_worker_threads,
                frame.phase9_generation_jobs,
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
            );
        }
    }
}

#[godot_api]
impl PlanetRoot {
    #[func]
    fn has_cached_world_rids(&self) -> bool {
        self.cached_scenario_rid.is_some() && self.cached_physics_space_rid.is_some()
    }

    #[func]
    fn runtime_has_valid_world_rids(&self) -> bool {
        self.runtime.has_valid_world_rids()
    }

    #[func]
    fn refresh_world_rids(&mut self) {
        self.cache_world_rids();
    }

    #[func]
    fn runtime_tick_count(&self) -> i64 {
        self.runtime_tick_count as i64
    }

    #[func]
    fn runtime_meta_count(&self) -> i64 {
        self.runtime.meta_count() as i64
    }

    #[func]
    fn runtime_active_render_count(&self) -> i64 {
        self.runtime.active_render_count() as i64
    }

    #[func]
    fn runtime_active_physics_count(&self) -> i64 {
        self.runtime.active_physics_count() as i64
    }

    #[func]
    fn runtime_active_asset_group_count(&self) -> i64 {
        self.runtime.active_asset_group_count() as i64
    }

    #[func]
    fn runtime_active_asset_instance_count(&self) -> i64 {
        self.runtime.active_asset_instance_count() as i64
    }

    #[func]
    fn runtime_desired_render_count(&self) -> i64 {
        self.runtime.desired_render_count() as i64
    }

    #[func]
    fn runtime_desired_physics_count(&self) -> i64 {
        self.runtime.desired_physics_count() as i64
    }

    #[func]
    fn runtime_deferred_commit_count(&self) -> i64 {
        self.runtime.deferred_commit_count() as i64
    }

    #[func]
    fn runtime_deferred_upload_bytes(&self) -> i64 {
        self.runtime.deferred_upload_bytes() as i64
    }

    #[func]
    fn runtime_resident_payload_count(&self) -> i64 {
        self.runtime.resident_payload_count() as i64
    }

    #[func]
    fn runtime_rid_state_count(&self) -> i64 {
        self.runtime.rid_state_count() as i64
    }

    #[func]
    fn metadata_precompute_max_lod(&self) -> i64 {
        self.runtime.metadata_precompute_max_lod() as i64
    }

    #[func]
    fn payload_precompute_max_lod(&self) -> i64 {
        self.runtime.payload_precompute_max_lod() as i64
    }

    #[func]
    fn topology_default_max_lod(&self) -> i64 {
        DEFAULT_MAX_LOD as i64
    }

    #[func]
    fn topology_edge_transform_count(&self) -> i64 {
        DIRECTED_EDGE_TRANSFORM_COUNT as i64
    }

    #[func]
    fn topology_visible_vertices_per_edge(&self) -> i64 {
        VISIBLE_VERTICES_PER_EDGE as i64
    }

    #[func]
    fn topology_sampled_vertices_per_edge(&self) -> i64 {
        SAMPLED_VERTICES_PER_EDGE as i64
    }

    #[func]
    fn topology_stitch_variant_count(&self) -> i64 {
        STITCH_VARIANT_COUNT as i64
    }

    #[func]
    fn topology_base_index_count(&self) -> i64 {
        canonical_chunk_topology().base_indices().len() as i64
    }

    #[func]
    fn runtime_active_stitch_mask_summary(&self) -> GString {
        let summary = self
            .runtime
            .seam_debug_snapshot()
            .active_stitch_mask_summary();
        GString::from(&summary)
    }

    #[func]
    fn runtime_pooled_stitch_mask_summary(&self) -> GString {
        let summary = self
            .runtime
            .seam_debug_snapshot()
            .pooled_stitch_mask_summary();
        GString::from(&summary)
    }

    #[func]
    fn runtime_active_stitched_edge_summary(&self) -> GString {
        let summary = self
            .runtime
            .seam_debug_snapshot()
            .active_stitched_edge_summary();
        GString::from(&summary)
    }

    #[func]
    fn runtime_pending_seam_mismatch_count(&self) -> i64 {
        self.runtime
            .seam_debug_snapshot()
            .pending_surface_class_mismatch_chunks as i64
    }

    fn acquire_camera_state(&mut self) -> Option<CameraState> {
        let raw = self.acquire_raw_camera_state()?;
        let camera_position_planet = self
            .runtime
            .camera_planet_position_from_render(raw.transform.origin);
        self.runtime
            .update_origin_from_camera(camera_position_planet);
        self.apply_runtime_origin_shift();

        let raw = self.acquire_raw_camera_state()?;
        Some(CameraState::from_godot(
            raw.transform,
            raw.frustum_planes,
            raw.fov_y_degrees,
            raw.viewport_height_px,
            self.runtime.origin_snapshot(),
        ))
    }

    fn acquire_raw_camera_state(&self) -> Option<RawCameraState> {
        let viewport = self.base().get_viewport()?;
        let camera = viewport.get_camera_3d()?;
        let frustum = camera.get_frustum();
        if frustum.len() != 6 {
            godot_warn!(
                "PlanetRoot expected 6 frustum planes from Camera3D but received {}.",
                frustum.len()
            );
            return None;
        }

        let frustum_planes = [
            frustum.at(0),
            frustum.at(1),
            frustum.at(2),
            frustum.at(3),
            frustum.at(4),
            frustum.at(5),
        ];
        let visible_rect = viewport.get_visible_rect();

        Some(RawCameraState {
            transform: camera.get_camera_transform(),
            frustum_planes,
            fov_y_degrees: camera.get_fov(),
            viewport_height_px: visible_rect.size.y,
        })
    }

    fn cache_world_rids(&mut self) {
        let Some(world) = self.base().get_world_3d() else {
            self.cached_scenario_rid = None;
            self.cached_physics_space_rid = None;
            self.runtime.set_world_rids(Rid::Invalid, Rid::Invalid);
            godot_warn!("PlanetRoot could not cache World3D RIDs because world was unavailable.");
            return;
        };

        let scenario_rid = world.get_scenario();
        let physics_space_rid = world.get_space();

        self.cached_scenario_rid = Some(scenario_rid);
        self.cached_physics_space_rid = Some(physics_space_rid);
        self.runtime.set_world_rids(scenario_rid, physics_space_rid);
    }

    fn apply_runtime_origin_shift(&mut self) {
        let root_position = self.runtime.root_scene_position();
        self.base_mut().set_position(root_position);
    }
}

struct RawCameraState {
    transform: Transform3D,
    frustum_planes: [Plane; 6],
    fov_y_degrees: f32,
    viewport_height_px: f32,
}

struct World2Extension;

#[gdextension]
unsafe impl ExtensionLibrary for World2Extension {}
