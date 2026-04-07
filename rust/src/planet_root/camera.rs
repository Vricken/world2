use super::super::*;
use crate::runtime::CameraState;
use godot::builtin::Plane;
use godot::classes::Camera3D;

pub(crate) struct RawCameraState {
    pub(crate) transform: Transform3D,
    pub(crate) frustum_planes: [Plane; 6],
    pub(crate) fov_y_degrees: f32,
    pub(crate) viewport_height_px: f32,
}

impl PlanetRoot {
    pub(crate) fn active_camera_3d(&self) -> Option<Gd<Camera3D>> {
        let viewport = self.base().get_viewport()?;
        viewport.get_camera_3d()
    }

    pub(crate) fn acquire_camera_state(&mut self) -> Option<CameraState> {
        let raw = self.acquire_raw_camera_state()?;
        Some(CameraState::from_godot(
            raw.transform,
            raw.frustum_planes,
            raw.fov_y_degrees,
            raw.viewport_height_px,
            self.runtime.origin_snapshot(),
        ))
    }

    pub(crate) fn acquire_raw_camera_state(&self) -> Option<RawCameraState> {
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
        })
    }

    pub(crate) fn cache_world_rids(&mut self) {
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
