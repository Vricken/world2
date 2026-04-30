use super::super::*;
use crate::runtime::{
    RuntimeConfig, DEFAULT_DENSE_METADATA_PREBUILD_MAX_LOD,
    DEFAULT_MIN_AVERAGE_CHUNK_SURFACE_SPAN_METERS,
};
use godot::classes::{CharacterBody3D, ProjectSettings};

impl PlanetRoot {
    pub(crate) fn rebuild_runtime(&mut self) {
        self.runtime.release_server_resources();
        self.runtime = PlanetRuntime::new(
            self.effective_runtime_config(),
            self.cached_scenario_rid.unwrap_or(Rid::Invalid),
            self.cached_physics_space_rid.unwrap_or(Rid::Invalid),
        );
        self.runtime_tick_count = 0;
    }

    pub(crate) fn effective_runtime_config(&self) -> RuntimeConfig {
        let default_config = RuntimeConfig::default();
        RuntimeConfig {
            planet_radius: self.planet_radius.max(1.0),
            height_amplitude: self.terrain_height_amplitude.max(0.0),
            sea_level_meters: self.sea_level_meters,
            water_enabled: self.water_enabled,
            hill_strength: self.hill_strength.max(0.0),
            mountain_strength: self.mountain_strength.max(0.0),
            mountain_frequency: self.mountain_frequency.max(f64::EPSILON),
            max_lod_cap: Self::project_max_lod_cap(),
            metadata_precompute_max_lod: DEFAULT_DENSE_METADATA_PREBUILD_MAX_LOD,
            dense_metadata_prebuild_max_lod: DEFAULT_DENSE_METADATA_PREBUILD_MAX_LOD,
            enable_frustum_culling: self.frustum_culling_enabled,
            keep_coarse_lod_chunks_rendered: self.keep_coarse_lod_chunks_rendered,
            render_backend: if self.debug_force_server_pool_render_backend {
                runtime::RenderBackendKind::ServerPool
            } else {
                default_config.render_backend
            },
            ..default_config
        }
    }

    pub(crate) fn effective_atmosphere_height(&self) -> f64 {
        self.planet_radius.max(1.0) * self.atmosphere_height.max(0.0)
    }

    fn project_max_lod_cap() -> u8 {
        let mut settings = ProjectSettings::singleton();
        let has_setting = settings
            .call("has_setting", &[PROJECT_SETTING_MAX_LOD_CAP.to_variant()])
            .to::<bool>();
        if !has_setting {
            return topology::DEFAULT_MAX_LOD;
        }
        let raw = settings
            .get(PROJECT_SETTING_MAX_LOD_CAP)
            .to::<i64>()
            .clamp(0, i64::from(topology::MAX_SUPPORTED_MAX_LOD));
        raw as u8
    }

    pub(crate) fn sync_runtime_scale_bootstrap(&mut self) {
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

    pub(crate) fn sync_runtime_camera_clip(&mut self) {
        let Some(mut camera) = self.active_camera_3d() else {
            return;
        };

        let camera_distance = self
            .runtime
            .camera_planet_position_from_render(camera.get_camera_transform().origin)
            .length();
        let target_far = self.runtime_camera_far_clip_for_distance(camera_distance);
        let target_near = runtime_camera_near_clip_for_far_clip(target_far);
        let current_far = f64::from(camera.get_far());
        let current_near = f64::from(camera.get_near());

        if current_far + 1.0 < target_far
            || current_far > target_far * DEFAULT_DEBUG_CAMERA_FAR_CLIP_SHRINK_RATIO
        {
            camera.set_far(target_far as f32);
        }
        if (current_near - target_near).abs() > 0.01 {
            camera.set_near(target_near as f32);
        }
    }

    pub(crate) fn runtime_player_spawn_distance(&self) -> f64 {
        let radius = self.planet_radius.max(1.0);
        let atmosphere_height = self.effective_atmosphere_height();
        let safety_margin = (radius * DEFAULT_DEBUG_PLAYER_SPAWN_MARGIN_RADIUS_SCALE)
            .max(self.terrain_height_amplitude.max(0.0) * 4.0)
            .max(DEFAULT_DEBUG_PLAYER_SPAWN_MARGIN);
        radius + atmosphere_height + safety_margin
    }

    pub(crate) fn runtime_camera_far_clip_for_distance(&self, camera_distance: f64) -> f64 {
        let radius = self.planet_radius.max(1.0);
        let atmosphere_height = self.effective_atmosphere_height();
        dynamic_runtime_camera_far_clip(radius, atmosphere_height, camera_distance)
    }

    pub(crate) fn runtime_min_average_chunk_surface_span_meters_value(&self) -> f64 {
        DEFAULT_MIN_AVERAGE_CHUNK_SURFACE_SPAN_METERS
    }
}

pub(crate) fn dynamic_runtime_camera_far_clip(
    planet_radius: f64,
    atmosphere_height: f64,
    camera_distance: f64,
) -> f64 {
    let outer_radius = planet_radius.max(1.0) + atmosphere_height.max(0.0);
    let proxy_bounding_radius =
        outer_radius * (DEFAULT_ATMOSPHERE_PROXY_BOX_SIZE_SCALE * 0.5) * 3.0_f64.sqrt();
    ((camera_distance.max(0.0) + proxy_bounding_radius)
        * DEFAULT_DEBUG_CAMERA_FAR_CLIP_MARGIN_SCALE)
        .max(DEFAULT_DEBUG_CAMERA_FAR_CLIP_MIN)
}

pub(crate) fn runtime_camera_near_clip_for_far_clip(far_clip: f64) -> f64 {
    (far_clip / DEFAULT_DEBUG_CAMERA_MAX_FAR_NEAR_RATIO).clamp(
        DEFAULT_DEBUG_CAMERA_NEAR_CLIP_MIN,
        DEFAULT_DEBUG_CAMERA_NEAR_CLIP_MAX,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dynamic_far_clip_covers_atmosphere_proxy_cube_from_orbit() {
        let planet_radius = 100_000.0;
        let atmosphere_height = 20_000.0;
        let camera_distance = 500_000.0;
        let target_far =
            dynamic_runtime_camera_far_clip(planet_radius, atmosphere_height, camera_distance);
        let outer_radius = planet_radius + atmosphere_height;
        let proxy_bounding_radius =
            outer_radius * (DEFAULT_ATMOSPHERE_PROXY_BOX_SIZE_SCALE * 0.5) * 3.0_f64.sqrt();

        assert!(target_far > camera_distance + proxy_bounding_radius);
    }

    #[test]
    fn dynamic_far_clip_keeps_small_world_minimum() {
        assert_eq!(
            dynamic_runtime_camera_far_clip(1_000.0, 200.0, 1_225.0),
            DEFAULT_DEBUG_CAMERA_FAR_CLIP_MIN
        );
    }

    #[test]
    fn dynamic_near_clip_bounds_far_near_ratio() {
        let far_clip = 422_176.188;
        let near_clip = runtime_camera_near_clip_for_far_clip(far_clip);

        assert!(near_clip > 2.0);
        assert!(far_clip / near_clip <= DEFAULT_DEBUG_CAMERA_MAX_FAR_NEAR_RATIO + 1.0e-6);
    }
}
