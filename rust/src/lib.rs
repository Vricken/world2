pub mod geometry;
pub mod mesh_topology;
pub mod runtime;
pub mod topology;

use godot::builtin::{VarDictionary, VariantType};
use godot::classes::{Camera3D, CharacterBody3D, DirectionalLight3D, INode3D, Node, Node3D};
use godot::classes::{Engine, MeshInstance3D, ProjectSettings, SphereMesh};
use godot::init::InitStage;
use godot::prelude::*;
use godot::register::info::PropertyHint;
use mesh_topology::{
    canonical_chunk_topology, SAMPLED_VERTICES_PER_EDGE, STITCH_VARIANT_COUNT,
    VISIBLE_VERTICES_PER_EDGE,
};
use runtime::{
    CameraState, PlanetRuntime, RuntimeConfig, CURRENT_IMPLEMENTED_PHASE,
    CURRENT_IMPLEMENTED_PHASE_LABEL, DEFAULT_DENSE_METADATA_PREBUILD_MAX_LOD,
    DEFAULT_MIN_AVERAGE_CHUNK_SURFACE_SPAN_METERS, NEXT_PHASE_LABEL,
};
use topology::{DEFAULT_MAX_LOD, DIRECTED_EDGE_TRANSFORM_COUNT, MAX_SUPPORTED_MAX_LOD};

const PROJECT_SETTING_MAX_LOD_CAP: &str = "world2/runtime/max_lod_cap";
const PROJECT_SETTING_DEBUG_LOD_VIEWPORT_HEIGHT_OVERRIDE: &str =
    "world2/debug/lod_viewport_height_override";
const EDITOR_PREVIEW_NODE_NAME: &str = "__World2EditorPreview";
const ATMOSPHERE_NODE_NAME: &str = "PlanetAtmosphere";
const ATMOSPHERE_PLANET_RADIUS_PROPERTY: &str = "planet_radius";
const ATMOSPHERE_HEIGHT_PROPERTY: &str = "atmosphere_height";
const DEFAULT_DEBUG_PLAYER_SPAWN_MARGIN: f64 = 500.0;
const DEFAULT_DEBUG_PLAYER_SPAWN_MARGIN_RADIUS_SCALE: f64 = 0.02;
const DEFAULT_DEBUG_CAMERA_FAR_CLIP_MIN: f64 = 100_000.0;

#[derive(GodotClass)]
#[class(tool, base = Node3D)]
pub struct PlanetRoot {
    base: Base<Node3D>,
    cached_scenario_rid: Option<Rid>,
    cached_physics_space_rid: Option<Rid>,
    runtime: PlanetRuntime,
    runtime_tick_count: u64,
    runtime_camera_clip_bootstrapped: bool,
    #[export]
    planet_radius: f64,
    #[export]
    terrain_height_amplitude: f64,
    #[export]
    atmosphere_height: f64,
    #[export]
    frustum_culling_enabled: bool,
    #[export]
    keep_coarse_lod_chunks_rendered: bool,
    editor_preview_radius_applied: f64,
    editor_preview: Option<Gd<MeshInstance3D>>,
}

#[godot_api]
impl INode3D for PlanetRoot {
    fn init(base: Base<Node3D>) -> Self {
        let runtime = PlanetRuntime::default();
        Self {
            base,
            cached_scenario_rid: None,
            cached_physics_space_rid: None,
            planet_radius: runtime.config.planet_radius,
            terrain_height_amplitude: runtime.config.height_amplitude,
            atmosphere_height: 0.2,
            frustum_culling_enabled: runtime.config.enable_frustum_culling,
            keep_coarse_lod_chunks_rendered: runtime.config.keep_coarse_lod_chunks_rendered,
            runtime,
            runtime_tick_count: 0,
            runtime_camera_clip_bootstrapped: false,
            editor_preview_radius_applied: -1.0,
            editor_preview: None,
        }
    }

    fn ready(&mut self) {
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
            self.has_cached_world_rids(),
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

    fn exit_tree(&mut self) {
        if self.is_editor_context() {
            self.teardown_editor_preview();
            self.base_mut().set_process(false);
            self.base_mut().set_physics_process(false);
            return;
        }
        self.runtime.release_server_resources();
        self.base_mut().set_process(false);
        self.base_mut().set_physics_process(false);
    }

    fn process(&mut self, _delta: f64) {
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

        let Some(camera_state) = self.acquire_camera_state() else {
            if self.runtime_tick_count == 1 {
                godot_warn!(
                    "PlanetRoot could not find an active Camera3D; skipping Phase {} runtime tick.",
                    CURRENT_IMPLEMENTED_PHASE
                );
            }
            return;
        };
        self.sync_runtime_camera_clip_bootstrap();

        if let Err(err) = self.runtime.step_visibility_selection(&camera_state) {
            godot_error!(
                "PlanetRoot Phase {} runtime tick failed: {err:?}",
                CURRENT_IMPLEMENTED_PHASE
            );
            return;
        }

        if self.runtime_tick_count == 1 || self.runtime_tick_count % 120 == 0 {
            let frame = self.runtime.frame_state();
            let seam = self.runtime.seam_debug_snapshot();
            let assets = self.runtime.asset_debug_snapshot();
            let strategies = self.runtime.strategy_summary();
            godot_print!(
                "PlanetRoot phase{} tick={} meta={} sparse_meta={} payloads={} desired_render={} active_render={} desired_physics={} active_physics={} horizon={} frustum={} selected_candidates={} refinement_iterations={} selection_cap_hits={} fullscreen_lod_bias=none selection_reference_height_px={} target_render_chunks={} hard_render_chunk_cap={} neighbor_splits={} sampled={} meshed={} packed={} staged={} commit_payloads={} warm_current={} warm_pool={} cold={} render_warm_current_commits={} render_warm_pool_commits={} render_cold_commits={} physics_commits={} meta_submitted={} meta_installed={} fallback_missing_current={} fallback_incompatible_current={} fallback_no_pool={} worker_threads={} worker_submitted={} worker_jobs={} worker_ready={} worker_stale={} worker_superseded={} worker_inflight={} worker_queue_peak={} worker_waits={} sample_scratch_reuse={} mesh_scratch_reuse={} pack_scratch_reuse={} scratch_growth={} origin_rebases={} render_rebinds={} physics_rebinds={} origin_mode={} render_pool_entries={} physics_pool_entries={} asset_payload_chunks={} asset_candidates={} asset_rejected={} asset_accepted={} active_asset_groups={} active_asset_instances={} asset_family_meshes={} active_stitched_chunks={} active_stitch_masks={} stitched_edges={} pooled_stitch_masks={} pending_seam_mismatches={} missing_active_surface_classes={} queued_ops={} deferred_ops={} deferred_upload_bytes={} starvation_frames={} build_order_steps={} strategy_summary={} next_phase={}",
                CURRENT_IMPLEMENTED_PHASE,
                frame.tick,
                self.runtime.meta_count(),
                frame.sparse_meta_entries,
                self.runtime.resident_payload_count(),
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

    fn physics_process(&mut self, _delta: f64) {
        if self.is_editor_context() {
            return;
        }

        self.sync_runtime_origin_shift();
    }
}

#[godot_api]
impl PlanetRoot {
    fn is_editor_context(&self) -> bool {
        Engine::singleton().is_editor_hint()
    }

    fn rebuild_runtime(&mut self) {
        self.runtime.release_server_resources();
        self.runtime = PlanetRuntime::new(
            self.effective_runtime_config(),
            self.cached_scenario_rid.unwrap_or(Rid::Invalid),
            self.cached_physics_space_rid.unwrap_or(Rid::Invalid),
        );
        self.runtime_tick_count = 0;
        self.runtime_camera_clip_bootstrapped = false;
    }

    fn effective_runtime_config(&self) -> RuntimeConfig {
        let mut config = RuntimeConfig::default();
        config.planet_radius = self.planet_radius.max(1.0);
        config.height_amplitude = self.terrain_height_amplitude.max(0.0);
        config.max_lod_cap = Self::project_max_lod_cap();
        config.metadata_precompute_max_lod = DEFAULT_DENSE_METADATA_PREBUILD_MAX_LOD;
        config.dense_metadata_prebuild_max_lod = DEFAULT_DENSE_METADATA_PREBUILD_MAX_LOD;
        config.enable_frustum_culling = self.frustum_culling_enabled;
        config.keep_coarse_lod_chunks_rendered = self.keep_coarse_lod_chunks_rendered;
        config
    }

    fn effective_atmosphere_height(&self) -> f64 {
        self.planet_radius.max(1.0) * self.atmosphere_height.max(0.0)
    }

    fn project_max_lod_cap() -> u8 {
        let mut settings = ProjectSettings::singleton();
        let has_setting = settings
            .call("has_setting", &[PROJECT_SETTING_MAX_LOD_CAP.to_variant()])
            .to::<bool>();
        if !has_setting {
            return DEFAULT_MAX_LOD;
        }
        let raw = settings
            .get(PROJECT_SETTING_MAX_LOD_CAP)
            .to::<i64>()
            .clamp(0, i64::from(MAX_SUPPORTED_MAX_LOD));
        raw as u8
    }

    fn project_lod_viewport_height_override() -> Option<f32> {
        let mut settings = ProjectSettings::singleton();
        let has_setting = settings
            .call(
                "has_setting",
                &[PROJECT_SETTING_DEBUG_LOD_VIEWPORT_HEIGHT_OVERRIDE.to_variant()],
            )
            .to::<bool>();
        if !has_setting {
            return None;
        }

        let override_px = settings
            .get(PROJECT_SETTING_DEBUG_LOD_VIEWPORT_HEIGHT_OVERRIDE)
            .to::<f64>()
            .max(0.0) as f32;
        (override_px > 0.0).then_some(override_px)
    }

    fn sync_editor_preview(&mut self) {
        self.editor_preview = self.prune_editor_preview_nodes();
        let radius = self.planet_radius.max(1.0);
        if (self.editor_preview_radius_applied - radius).abs() <= f64::EPSILON
            && self.editor_preview.is_some()
        {
            return;
        }

        let mut preview = self.ensure_editor_preview();
        self.assign_editor_preview_owner(&mut preview);
        let mut mesh = SphereMesh::new_gd();
        mesh.set_radius(radius as f32);
        mesh.set_height((radius * 2.0) as f32);
        preview.set_name(EDITOR_PREVIEW_NODE_NAME);
        preview.set("mesh", &mesh.to_variant());
        preview.set_visible(true);
        self.editor_preview_radius_applied = radius;
    }

    fn sync_atmosphere_settings(&mut self) {
        let radius = self.planet_radius.max(1.0);
        let height = self.effective_atmosphere_height();
        let light_transform = self.first_directional_light_transform();
        let child_count = self.base().get_child_count();

        for child_index in 0..child_count {
            let Some(mut child) = self.base().get_child(child_index) else {
                continue;
            };
            if child.get_name().to_string() != ATMOSPHERE_NODE_NAME {
                continue;
            }

            child.set(ATMOSPHERE_PLANET_RADIUS_PROPERTY, &radius.to_variant());
            child.set(ATMOSPHERE_HEIGHT_PROPERTY, &height.to_variant());

            let Ok(mut atmosphere) = child.try_cast::<Node3D>() else {
                continue;
            };
            if let Some(light_transform) = light_transform {
                let mut atmosphere_transform = atmosphere.get_transform();
                atmosphere_transform.basis = light_transform.basis;
                atmosphere_transform.origin = Vector3::ZERO;
                atmosphere.set_transform(atmosphere_transform);
            }
        }
    }

    fn sync_runtime_scale_bootstrap(&mut self) {
        let spawn_distance = self.runtime_player_spawn_distance();

        let child_count = self.base().get_child_count();
        for child_index in 0..child_count {
            let Some(child) = self.base().get_child(child_index) else {
                continue;
            };
            let Ok(mut player) = child.try_cast::<CharacterBody3D>() else {
                continue;
            };

            let mut player_transform = player.get_transform();
            player_transform.origin = Vector3::new(0.0, 0.0, spawn_distance as f32);
            player.set_transform(player_transform);
            player.reset_physics_interpolation();
            break;
        }
    }

    fn sync_runtime_camera_clip_bootstrap(&mut self) {
        if self.runtime_camera_clip_bootstrapped {
            return;
        }

        let Some(mut camera) = self.active_camera_3d() else {
            return;
        };

        // Keep the far plane close to the actual startup view volume so large
        // worlds do not pay unnecessary precision loss on the first frame.
        let target_far = self.runtime_camera_far_clip() as f32;
        if camera.get_far() < target_far {
            camera.set_far(target_far);
        }
        self.runtime_camera_clip_bootstrapped = true;
    }

    fn runtime_player_spawn_distance(&self) -> f64 {
        let radius = self.planet_radius.max(1.0);
        let atmosphere_height = self.effective_atmosphere_height();
        let safety_margin = (radius * DEFAULT_DEBUG_PLAYER_SPAWN_MARGIN_RADIUS_SCALE)
            .max(self.terrain_height_amplitude.max(0.0) * 4.0)
            .max(DEFAULT_DEBUG_PLAYER_SPAWN_MARGIN);
        radius + atmosphere_height + safety_margin
    }

    fn runtime_camera_far_clip(&self) -> f64 {
        let radius = self.planet_radius.max(1.0);
        let atmosphere_height = self.effective_atmosphere_height();
        (self.runtime_player_spawn_distance() + radius + atmosphere_height)
            .max(DEFAULT_DEBUG_CAMERA_FAR_CLIP_MIN)
    }

    fn active_camera_3d(&self) -> Option<Gd<Camera3D>> {
        let viewport = self.base().get_viewport()?;
        viewport.get_camera_3d()
    }

    fn first_directional_light_transform(&self) -> Option<Transform3D> {
        let child_count = self.base().get_child_count();
        for child_index in 0..child_count {
            let Some(child) = self.base().get_child(child_index) else {
                continue;
            };
            let Ok(light) = child.try_cast::<DirectionalLight3D>() else {
                continue;
            };
            return Some(light.get_transform());
        }
        None
    }

    fn ensure_editor_preview(&mut self) -> Gd<MeshInstance3D> {
        if let Some(preview) = self.editor_preview.as_ref() {
            return preview.clone();
        }

        if let Some(preview) = self.find_editor_preview_node() {
            self.editor_preview = Some(preview.clone());
            return preview;
        }

        let mut preview = MeshInstance3D::new_alloc();
        preview.set_name(EDITOR_PREVIEW_NODE_NAME);
        self.base_mut().add_child(&preview);
        self.assign_editor_preview_owner(&mut preview);
        self.editor_preview = Some(preview.clone());
        preview
    }

    fn assign_editor_preview_owner(&self, preview: &mut Gd<MeshInstance3D>) {
        let owner = self
            .base()
            .get_tree()
            .get_edited_scene_root()
            .or_else(|| Some(self.base().clone().upcast::<Node>()));
        if let Some(owner) = owner {
            preview.set_owner(&owner);
        }
    }

    fn find_editor_preview_node(&self) -> Option<Gd<MeshInstance3D>> {
        let child_count = self.base().get_child_count();
        for child_index in 0..child_count {
            let Some(child) = self.base().get_child(child_index) else {
                continue;
            };
            if child.get_name().to_string() != EDITOR_PREVIEW_NODE_NAME {
                continue;
            }
            if let Ok(preview) = child.try_cast::<MeshInstance3D>() {
                return Some(preview);
            }
        }
        None
    }

    fn prune_editor_preview_nodes(&mut self) -> Option<Gd<MeshInstance3D>> {
        let child_count = self.base().get_child_count();
        let mut kept_preview = None;
        let mut duplicate_children = Vec::new();

        for child_index in 0..child_count {
            let Some(child) = self.base().get_child(child_index) else {
                continue;
            };
            if child.get_name().to_string() != EDITOR_PREVIEW_NODE_NAME {
                continue;
            }

            match child.clone().try_cast::<MeshInstance3D>() {
                Ok(preview) if kept_preview.is_none() => {
                    kept_preview = Some(preview);
                }
                Ok(_) | Err(_) => duplicate_children.push(child),
            }
        }

        for mut child in duplicate_children {
            self.base_mut().remove_child(&child);
            child.queue_free();
        }

        kept_preview
    }

    fn remove_runtime_preview_node(&mut self) {
        self.editor_preview = None;
        let child_count = self.base().get_child_count();
        let mut to_remove = Vec::new();

        for child_index in 0..child_count {
            let Some(child) = self.base().get_child(child_index) else {
                continue;
            };
            if child.get_name().to_string() == EDITOR_PREVIEW_NODE_NAME {
                to_remove.push(child);
            }
        }

        for mut child in to_remove {
            self.base_mut().remove_child(&child);
            child.queue_free();
        }
        self.editor_preview_radius_applied = -1.0;
    }

    fn teardown_editor_preview(&mut self) {
        if let Some(mut preview) = self.editor_preview.take() {
            preview.queue_free();
        }
        self.editor_preview_radius_applied = -1.0;
    }

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
    fn runtime_selected_candidates(&self) -> i64 {
        self.runtime.frame_state().selected_candidates as i64
    }

    #[func]
    fn runtime_refinement_iterations(&self) -> i64 {
        self.runtime.frame_state().refinement_iterations as i64
    }

    #[func]
    fn runtime_selection_cap_hits(&self) -> i64 {
        self.runtime.frame_state().selection_cap_hits as i64
    }

    #[func]
    fn runtime_sparse_meta_count(&self) -> i64 {
        self.runtime.sparse_meta_count() as i64
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
    fn dense_metadata_prebuild_max_lod(&self) -> i64 {
        self.runtime.dense_metadata_prebuild_max_lod() as i64
    }

    #[func]
    fn payload_precompute_max_lod(&self) -> i64 {
        self.runtime.payload_precompute_max_lod() as i64
    }

    #[func]
    fn runtime_max_lod(&self) -> i64 {
        self.runtime.config.max_lod as i64
    }

    #[func]
    fn runtime_max_lod_cap(&self) -> i64 {
        self.runtime.config.max_lod_cap as i64
    }

    #[func]
    fn runtime_render_lod_reference_height_px(&self) -> f64 {
        f64::from(self.runtime.config.render_lod_reference_height_px)
    }

    #[func]
    fn runtime_target_render_chunks(&self) -> i64 {
        self.runtime.config.target_render_chunks as i64
    }

    #[func]
    fn runtime_hard_render_chunk_cap(&self) -> i64 {
        self.runtime.config.hard_render_chunk_cap as i64
    }

    #[func]
    fn runtime_fullscreen_lod_bias(&self) -> GString {
        GString::from("none")
    }

    #[func]
    fn runtime_min_average_chunk_surface_span_meters(&self) -> f64 {
        DEFAULT_MIN_AVERAGE_CHUNK_SURFACE_SPAN_METERS
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

    #[func]
    fn runtime_build_order_step_count(&self) -> i64 {
        PlanetRuntime::build_order_stage_count() as i64
    }

    #[func]
    fn runtime_build_order_summary(&self) -> GString {
        let summary = self.runtime.build_order_summary();
        GString::from(summary.as_str())
    }

    #[func]
    fn runtime_strategy_summary(&self) -> GString {
        let summary = self.runtime.strategy_summary();
        GString::from(summary.as_str())
    }

    #[func]
    fn runtime_next_phase_label(&self) -> GString {
        GString::from(NEXT_PHASE_LABEL)
    }

    fn acquire_camera_state(&mut self) -> Option<CameraState> {
        let raw = self.acquire_raw_camera_state()?;
        Some(CameraState::from_godot(
            raw.transform,
            raw.frustum_planes,
            raw.fov_y_degrees,
            raw.viewport_height_px,
            raw.lod_reference_height_px_override,
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
        let viewport_height_px = visible_rect.size.y.max(1.0);

        Some(RawCameraState {
            transform: camera.get_camera_transform(),
            frustum_planes,
            fov_y_degrees: camera.get_fov(),
            viewport_height_px,
            lod_reference_height_px_override: Self::project_lod_viewport_height_override(),
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

    fn sync_runtime_origin_shift(&mut self) {
        let Some(raw) = self.acquire_raw_camera_state() else {
            return;
        };
        if self.should_defer_origin_shift_for_collision() {
            return;
        }

        let camera_position_planet = self
            .runtime
            .camera_planet_position_from_render(raw.transform.origin);
        if self
            .runtime
            .update_origin_from_camera(camera_position_planet)
        {
            self.apply_runtime_origin_shift();
            self.runtime.flush_pending_origin_rebinds();
        }
    }

    fn should_defer_origin_shift_for_collision(&self) -> bool {
        let Some(viewport) = self.base().get_viewport() else {
            return false;
        };
        let Some(camera) = viewport.get_camera_3d() else {
            return false;
        };

        let mut current = camera.get_parent();
        while let Some(node) = current {
            if let Ok(body) = node.clone().try_cast::<CharacterBody3D>() {
                return body.get_slide_collision_count() > 0
                    || body.is_on_floor()
                    || body.is_on_wall()
                    || body.is_on_ceiling();
            }
            current = node.get_parent();
        }

        false
    }

    fn apply_runtime_origin_shift(&mut self) {
        let root_position = self.runtime.root_scene_position();
        let current_position = self.base().get_position();
        if (current_position - root_position).length_squared() <= 1.0e-10 {
            return;
        }
        self.base_mut().set_position(root_position);
        self.base_mut().reset_physics_interpolation();
    }
}

struct RawCameraState {
    transform: Transform3D,
    frustum_planes: [Plane; 6],
    fov_y_degrees: f32,
    viewport_height_px: f32,
    lod_reference_height_px_override: Option<f32>,
}

struct World2Extension;

fn register_world2_project_settings() {
    let mut settings = ProjectSettings::singleton();
    let default_cap = i64::from(DEFAULT_MAX_LOD);
    let default_lod_viewport_height_override = 0.0_f64;
    let has_setting = settings
        .call("has_setting", &[PROJECT_SETTING_MAX_LOD_CAP.to_variant()])
        .to::<bool>();

    if !has_setting {
        settings.set(PROJECT_SETTING_MAX_LOD_CAP, &default_cap.to_variant());
    }

    let mut info = VarDictionary::new();
    info.set("name", PROJECT_SETTING_MAX_LOD_CAP);
    info.set("type", VariantType::INT);
    info.set("hint", PropertyHint::RANGE);
    info.set("hint_string", format!("0,{},1", MAX_SUPPORTED_MAX_LOD));
    settings.call("add_property_info", &[info.to_variant()]);
    settings.call(
        "set_initial_value",
        &[
            PROJECT_SETTING_MAX_LOD_CAP.to_variant(),
            default_cap.to_variant(),
        ],
    );
    settings.call(
        "set_as_basic",
        &[PROJECT_SETTING_MAX_LOD_CAP.to_variant(), true.to_variant()],
    );

    let has_debug_override_setting = settings
        .call(
            "has_setting",
            &[PROJECT_SETTING_DEBUG_LOD_VIEWPORT_HEIGHT_OVERRIDE.to_variant()],
        )
        .to::<bool>();
    if !has_debug_override_setting {
        settings.set(
            PROJECT_SETTING_DEBUG_LOD_VIEWPORT_HEIGHT_OVERRIDE,
            &default_lod_viewport_height_override.to_variant(),
        );
    }

    let mut debug_info = VarDictionary::new();
    debug_info.set("name", PROJECT_SETTING_DEBUG_LOD_VIEWPORT_HEIGHT_OVERRIDE);
    debug_info.set("type", VariantType::FLOAT);
    debug_info.set("hint", PropertyHint::RANGE);
    debug_info.set("hint_string", "0,16384,1,or_greater");
    settings.call("add_property_info", &[debug_info.to_variant()]);
    settings.call(
        "set_initial_value",
        &[
            PROJECT_SETTING_DEBUG_LOD_VIEWPORT_HEIGHT_OVERRIDE.to_variant(),
            default_lod_viewport_height_override.to_variant(),
        ],
    );
}

#[gdextension]
unsafe impl ExtensionLibrary for World2Extension {
    fn on_stage_init(stage: InitStage) {
        if stage == InitStage::Scene {
            register_world2_project_settings();
        }
    }
}
