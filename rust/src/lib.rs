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

        godot_print!(
            "PlanetRoot ready. Phase 09 runtime active. chunks_in_scene_tree=false cached_world_rids={} meta_precompute_max_lod={} payload_precompute_max_lod={} worker_threads={} prebuilt_meta={} edge_xforms={} default_max_lod={} visible_edge_verts={} sampled_edge_verts={} stitch_variants={} base_index_count={}",
            self.has_cached_world_rids(),
            self.runtime.metadata_precompute_max_lod(),
            self.runtime.payload_precompute_max_lod(),
            self.runtime.worker_thread_count(),
            self.runtime.meta_count(),
            DIRECTED_EDGE_TRANSFORM_COUNT,
            DEFAULT_MAX_LOD,
            VISIBLE_VERTICES_PER_EDGE,
            SAMPLED_VERTICES_PER_EDGE,
            STITCH_VARIANT_COUNT,
            canonical_chunk_topology().base_indices().len()
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
                    "PlanetRoot could not find an active Camera3D; skipping Phase 09 runtime tick."
                );
            }
            return;
        };

        if let Err(err) = self.runtime.step_visibility_selection(&camera_state) {
            godot_error!("PlanetRoot Phase 09 runtime tick failed: {err:?}");
            return;
        }

        if self.runtime_tick_count == 1 || self.runtime_tick_count % 120 == 0 {
            let frame = self.runtime.frame_state();
            godot_print!(
                "PlanetRoot phase09 tick={} meta={} payloads={} desired_render={} active_render={} desired_physics={} active_physics={} horizon={} frustum={} neighbor_splits={} sampled={} meshed={} packed={} staged={} commit_payloads={} warm_current={} warm_pool={} cold={} render_warm_current_commits={} render_warm_pool_commits={} render_cold_commits={} physics_commits={} fallback_missing_current={} fallback_incompatible_current={} fallback_no_pool={} worker_threads={} worker_jobs={} worker_queue_peak={} worker_waits={} sample_scratch_reuse={} mesh_scratch_reuse={} pack_scratch_reuse={} scratch_growth={} render_pool_entries={} physics_pool_entries={} queued_ops={} deferred_ops={} deferred_upload_bytes={} starvation_frames={}",
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
                frame.render_pool_entries,
                frame.physics_pool_entries,
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

    fn acquire_camera_state(&self) -> Option<CameraState> {
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

        Some(CameraState::from_godot(
            camera.get_camera_transform(),
            frustum_planes,
            camera.get_fov(),
            visible_rect.size.y,
        ))
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
}

struct World2Extension;

#[gdextension]
unsafe impl ExtensionLibrary for World2Extension {}
