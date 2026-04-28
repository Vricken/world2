use super::super::*;
use godot::classes::CharacterBody3D;

impl PlanetRoot {
    pub(crate) fn sync_runtime_origin_shift(&mut self) {
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
            self.debug_origin_rebase_count = self.debug_origin_rebase_count.saturating_add(1);
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

    pub(crate) fn apply_runtime_origin_shift(&mut self) {
        let root_position = self.runtime.root_scene_position();
        let current_position = self.base().get_position();
        if (current_position - root_position).length_squared() <= 1.0e-10 {
            return;
        }
        self.base_mut().set_position(root_position);
        self.base_mut().reset_physics_interpolation();
    }
}
