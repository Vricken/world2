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
        self.runtime_camera_clip_bootstrapped = false;
    }

    pub(crate) fn effective_runtime_config(&self) -> RuntimeConfig {
        let default_config = RuntimeConfig::default();
        RuntimeConfig {
            planet_radius: self.planet_radius.max(1.0),
            height_amplitude: self.terrain_height_amplitude.max(0.0),
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

    pub(crate) fn sync_runtime_camera_clip_bootstrap(&mut self) {
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

    pub(crate) fn runtime_player_spawn_distance(&self) -> f64 {
        let radius = self.planet_radius.max(1.0);
        let atmosphere_height = self.effective_atmosphere_height();
        let safety_margin = (radius * DEFAULT_DEBUG_PLAYER_SPAWN_MARGIN_RADIUS_SCALE)
            .max(self.terrain_height_amplitude.max(0.0) * 4.0)
            .max(DEFAULT_DEBUG_PLAYER_SPAWN_MARGIN);
        radius + atmosphere_height + safety_margin
    }

    pub(crate) fn runtime_camera_far_clip(&self) -> f64 {
        let radius = self.planet_radius.max(1.0);
        let atmosphere_height = self.effective_atmosphere_height();
        (self.runtime_player_spawn_distance() + radius + atmosphere_height)
            .max(DEFAULT_DEBUG_CAMERA_FAR_CLIP_MIN)
    }

    pub(crate) fn runtime_min_average_chunk_surface_span_meters_value(&self) -> f64 {
        DEFAULT_MIN_AVERAGE_CHUNK_SURFACE_SPAN_METERS
    }
}
