pub mod geometry;
pub mod mesh_topology;
mod planet_root;
pub mod runtime;
pub mod topology;

use godot::builtin::{VarDictionary, VariantType};
use godot::classes::{INode3D, MeshInstance3D, Node3D};
use godot::init::InitStage;
use godot::prelude::*;
use godot::register::info::PropertyHint;
use mesh_topology::{
    canonical_chunk_topology, SAMPLED_VERTICES_PER_EDGE, STITCH_VARIANT_COUNT,
    VISIBLE_VERTICES_PER_EDGE,
};
use runtime::{PlanetRuntime, NEXT_PHASE_LABEL};
use topology::{DEFAULT_MAX_LOD, DIRECTED_EDGE_TRANSFORM_COUNT};

const PROJECT_SETTING_MAX_LOD_CAP: &str = "world2/runtime/max_lod_cap";
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
    #[export]
    debug_force_server_pool_render_backend: bool,
    editor_preview_radius_applied: f64,
    editor_preview: Option<Gd<MeshInstance3D>>,
}

#[godot_api]
impl INode3D for PlanetRoot {
    fn init(base: Base<Node3D>) -> Self {
        Self::init_state(base)
    }

    fn ready(&mut self) {
        self.ready_impl();
    }

    fn exit_tree(&mut self) {
        self.exit_tree_impl();
    }

    fn process(&mut self, _delta: f64) {
        self.process_impl();
    }

    fn physics_process(&mut self, _delta: f64) {
        self.physics_process_impl();
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
    fn runtime_render_residency_count(&self) -> i64 {
        self.runtime.render_residency_count() as i64
    }

    #[func]
    fn runtime_gpu_tile_upload_bytes(&self) -> i64 {
        self.runtime.frame_state().gpu_tile_upload_bytes as i64
    }

    #[func]
    fn runtime_gpu_material_binds(&self) -> i64 {
        self.runtime.frame_state().gpu_material_binds as i64
    }

    #[func]
    fn runtime_active_gpu_render_chunks(&self) -> i64 {
        self.runtime.frame_state().active_gpu_render_chunks as i64
    }

    #[func]
    fn runtime_canonical_render_meshes(&self) -> i64 {
        self.runtime.frame_state().canonical_render_meshes as i64
    }

    #[func]
    fn runtime_render_residency_evictions(&self) -> i64 {
        self.runtime.frame_state().render_residency_evictions as i64
    }

    #[func]
    fn runtime_render_tile_bytes(&self) -> i64 {
        self.runtime.frame_state().render_tile_bytes as i64
    }

    #[func]
    fn runtime_render_tile_pool_slots(&self) -> i64 {
        self.runtime.frame_state().render_tile_pool_slots as i64
    }

    #[func]
    fn runtime_render_tile_pool_active_slots(&self) -> i64 {
        self.runtime.frame_state().render_tile_pool_active_slots as i64
    }

    #[func]
    fn runtime_render_tile_pool_free_slots(&self) -> i64 {
        self.runtime.frame_state().render_tile_pool_free_slots as i64
    }

    #[func]
    fn runtime_render_tile_eviction_ready_slots(&self) -> i64 {
        self.runtime.frame_state().render_tile_eviction_ready_slots as i64
    }

    #[func]
    fn runtime_collision_residency_count(&self) -> i64 {
        self.runtime.frame_state().collision_residency_entries as i64
    }

    #[func]
    fn runtime_collision_residency_bytes(&self) -> i64 {
        self.runtime.frame_state().collision_residency_bytes as i64
    }

    #[func]
    fn runtime_selected_render_starved_chunks(&self) -> i64 {
        self.runtime.frame_state().selected_render_starved_chunks as i64
    }

    #[func]
    fn runtime_selected_render_starvation_failures(&self) -> i64 {
        self.runtime
            .frame_state()
            .selected_render_starvation_failures as i64
    }

    #[func]
    fn runtime_max_selected_render_starvation_frames(&self) -> i64 {
        self.runtime
            .frame_state()
            .max_selected_render_starvation_frames as i64
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
        self.runtime_min_average_chunk_surface_span_meters_value()
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
}

struct World2Extension;

fn register_world2_project_settings() {
    let mut settings = godot::classes::ProjectSettings::singleton();
    let default_cap = i64::from(topology::DEFAULT_MAX_LOD);
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
    info.set(
        "hint_string",
        format!("0,{},1", topology::MAX_SUPPORTED_MAX_LOD),
    );
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
}

#[gdextension]
unsafe impl ExtensionLibrary for World2Extension {
    fn on_stage_init(stage: InitStage) {
        if stage == InitStage::Scene {
            register_world2_project_settings();
        }
    }
}
