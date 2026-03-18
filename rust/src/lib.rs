use godot::classes::{INode3D, Node3D};
use godot::prelude::*;

#[derive(GodotClass)]
#[class(base = Node3D)]
pub struct PlanetRoot {
    base: Base<Node3D>,
    cached_scenario_rid: Option<Rid>,
    cached_physics_space_rid: Option<Rid>,
    runtime_tick_count: u64,
}

#[godot_api]
impl INode3D for PlanetRoot {
    fn init(base: Base<Node3D>) -> Self {
        Self {
            base,
            cached_scenario_rid: None,
            cached_physics_space_rid: None,
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
    fn refresh_world_rids(&mut self) {
        self.cache_world_rids();
    }

    #[func]
    fn runtime_tick_count(&self) -> i64 {
        self.runtime_tick_count as i64
    }

    fn cache_world_rids(&mut self) {
        let Some(world) = self.base().get_world_3d() else {
            self.cached_scenario_rid = None;
            self.cached_physics_space_rid = None;
            godot_warn!("PlanetRoot could not cache World3D RIDs because world was unavailable.");
            return;
        };

        self.cached_scenario_rid = Some(world.get_scenario());
        self.cached_physics_space_rid = Some(world.get_space());
    }
}

struct World2Extension;

#[gdextension]
unsafe impl ExtensionLibrary for World2Extension {}
