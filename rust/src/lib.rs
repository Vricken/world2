pub mod runtime;

use godot::classes::{INode3D, Node3D};
use godot::prelude::*;
use runtime::{PlanetRuntime, PAYLOAD_PRECOMPUTE_MAX_LOD};

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
            "PlanetRoot ready. Shell-only scene active. chunks_in_scene_tree=false cached_world_rids={}",
            self.has_cached_world_rids()
        );
    }

    fn process(&mut self, _delta: f64) {
        self.runtime_tick_count = self.runtime_tick_count.saturating_add(1);

        if self.runtime_tick_count == 1 || self.runtime_tick_count % 300 == 0 {
            self.cache_world_rids();
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
    fn runtime_resident_payload_count(&self) -> i64 {
        self.runtime.resident_payload_count() as i64
    }

    #[func]
    fn runtime_rid_state_count(&self) -> i64 {
        self.runtime.rid_state_count() as i64
    }

    #[func]
    fn payload_precompute_max_lod(&self) -> i64 {
        PAYLOAD_PRECOMPUTE_MAX_LOD as i64
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
