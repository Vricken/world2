use super::{
    dvec3_to_vector3, CameraState, ChunkKey, ChunkMeta, GdPackedStaging, PackedMeshRegions,
    PlanetRuntime, RenderWarmPath, RuntimeConfig, SelectionFrameState, SurfaceClassKey,
};
use crate::geometry::CubeProjection;
use glam::DVec3;

pub trait ProjectionStrategy {
    fn label(&self) -> &'static str;
    fn project(&self, cube_point: DVec3) -> DVec3;
}

impl ProjectionStrategy for CubeProjection {
    fn label(&self) -> &'static str {
        match self {
            Self::Normalized => "normalized_cube",
            Self::Spherified => "spherified_cube",
        }
    }

    fn project(&self, cube_point: DVec3) -> DVec3 {
        self.project_cube_point(cube_point)
    }
}

pub trait ChunkVisibilityStrategy {
    fn label(&self) -> &'static str;
    fn horizon_visible(
        &self,
        config: &RuntimeConfig,
        camera: &CameraState,
        meta: &ChunkMeta,
    ) -> bool;
    fn frustum_visible(
        &self,
        config: &RuntimeConfig,
        camera: &CameraState,
        meta: &ChunkMeta,
    ) -> bool;
    fn screen_error_px(
        &self,
        config: &RuntimeConfig,
        camera: &CameraState,
        meta: &ChunkMeta,
    ) -> f32;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VisibilityStrategyKind {
    #[default]
    HorizonFrustumLod,
}

impl ChunkVisibilityStrategy for VisibilityStrategyKind {
    fn label(&self) -> &'static str {
        match self {
            Self::HorizonFrustumLod => "horizon_frustum_lod",
        }
    }

    fn horizon_visible(
        &self,
        config: &RuntimeConfig,
        camera: &CameraState,
        meta: &ChunkMeta,
    ) -> bool {
        match self {
            Self::HorizonFrustumLod => default_horizon_visible(config, camera, meta),
        }
    }

    fn frustum_visible(
        &self,
        config: &RuntimeConfig,
        camera: &CameraState,
        meta: &ChunkMeta,
    ) -> bool {
        match self {
            Self::HorizonFrustumLod => default_frustum_visible(config, camera, meta),
        }
    }

    fn screen_error_px(
        &self,
        config: &RuntimeConfig,
        camera: &CameraState,
        meta: &ChunkMeta,
    ) -> f32 {
        match self {
            Self::HorizonFrustumLod => default_projected_error_px(config, camera, meta),
        }
    }
}

pub trait ChunkRenderBackend {
    fn label(&self) -> &'static str;
    fn commit_render_payload(
        &self,
        runtime: &mut PlanetRuntime,
        key: ChunkKey,
        frame_state: &mut SelectionFrameState,
    ) -> bool;
    fn deactivate_render(&self, runtime: &mut PlanetRuntime, key: ChunkKey);
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RenderBackendKind {
    #[default]
    ServerPool,
}

impl ChunkRenderBackend for RenderBackendKind {
    fn label(&self) -> &'static str {
        match self {
            Self::ServerPool => "server_pool_render_backend",
        }
    }

    fn commit_render_payload(
        &self,
        runtime: &mut PlanetRuntime,
        key: ChunkKey,
        frame_state: &mut SelectionFrameState,
    ) -> bool {
        match self {
            Self::ServerPool => runtime.commit_render_payload_with_server_backend(key, frame_state),
        }
    }

    fn deactivate_render(&self, runtime: &mut PlanetRuntime, key: ChunkKey) {
        match self {
            Self::ServerPool => runtime.deactivate_render_commit_with_server_backend(key),
        }
    }
}

pub trait PackedStagingPolicy {
    fn label(&self) -> &'static str;
    fn acquire_staging(
        &self,
        runtime: &mut PlanetRuntime,
        key: ChunkKey,
        surface_class: &SurfaceClassKey,
        warm_path: &RenderWarmPath,
    ) -> GdPackedStaging;
    fn fill_staging_from_payload(
        &self,
        staging: Option<GdPackedStaging>,
        packed_regions: Option<&PackedMeshRegions>,
        surface_class: &SurfaceClassKey,
    ) -> Option<GdPackedStaging>;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PackedStagingPolicyKind {
    #[default]
    GodotOwnedReuse,
}

impl PackedStagingPolicy for PackedStagingPolicyKind {
    fn label(&self) -> &'static str {
        match self {
            Self::GodotOwnedReuse => "godot_owned_packed_byte_array",
        }
    }

    fn acquire_staging(
        &self,
        runtime: &mut PlanetRuntime,
        key: ChunkKey,
        surface_class: &SurfaceClassKey,
        warm_path: &RenderWarmPath,
    ) -> GdPackedStaging {
        match self {
            Self::GodotOwnedReuse => match warm_path {
                RenderWarmPath::ReuseCurrentSurface => runtime
                    .ensure_rid_state(key)
                    .gd_staging
                    .take()
                    .unwrap_or_else(|| GdPackedStaging::new_for_surface_class(surface_class)),
                RenderWarmPath::ReusePooledSurface(entry) => entry
                    .gd_staging
                    .clone()
                    .unwrap_or_else(|| GdPackedStaging::new_for_surface_class(surface_class)),
                RenderWarmPath::ColdPath(_) => {
                    GdPackedStaging::new_for_surface_class(surface_class)
                }
            },
        }
    }

    fn fill_staging_from_payload(
        &self,
        staging: Option<GdPackedStaging>,
        packed_regions: Option<&PackedMeshRegions>,
        surface_class: &SurfaceClassKey,
    ) -> Option<GdPackedStaging> {
        match self {
            Self::GodotOwnedReuse => {
                let mut staging = staging
                    .unwrap_or_else(|| GdPackedStaging::new_for_surface_class(surface_class));
                if let Some(packed_regions) = packed_regions {
                    staging
                        .copy_from_regions(packed_regions, surface_class)
                        .ok()?;
                }
                Some(staging)
            }
        }
    }
}

fn default_horizon_visible(config: &RuntimeConfig, camera: &CameraState, meta: &ChunkMeta) -> bool {
    let camera_distance = camera.position_planet.length().max(f64::EPSILON);
    let occluder_radius = (config.planet_radius - config.height_amplitude).max(1.0);
    let beta_camera = if camera_distance <= occluder_radius {
        std::f64::consts::PI
    } else {
        (occluder_radius / camera_distance).clamp(-1.0, 1.0).acos()
    };
    let chunk_max_radius = meta.bounds.max_radius.max(occluder_radius);
    let beta_chunk = if chunk_max_radius <= occluder_radius {
        0.0
    } else {
        (occluder_radius / chunk_max_radius).clamp(-1.0, 1.0).acos()
    };
    let angular_slack = (config.horizon_safety_margin / camera_distance.max(chunk_max_radius))
        .clamp(0.0, std::f64::consts::FRAC_PI_2);
    let theta = camera
        .position_planet
        .normalize_or_zero()
        .angle_between(meta.bounds.center_planet.normalize_or_zero());

    theta <= beta_camera + beta_chunk + f64::from(meta.metrics.angular_radius) + angular_slack
}

fn default_frustum_visible(config: &RuntimeConfig, camera: &CameraState, meta: &ChunkMeta) -> bool {
    if !config.enable_frustum_culling {
        return true;
    }

    let center = dvec3_to_vector3(meta.bounds.center_planet - camera.origin.render_origin_planet);
    let radius = meta.bounds.radius as f32;

    camera
        .frustum_planes
        .iter()
        .all(|plane| plane.distance_to(center) <= radius)
}

fn default_projected_error_px(
    config: &RuntimeConfig,
    camera: &CameraState,
    meta: &ChunkMeta,
) -> f32 {
    let distance = (meta.bounds.center_planet - camera.position_planet)
        .length()
        .max(f64::from(f32::EPSILON));
    let reference_height_px =
        camera.effective_lod_reference_height_px(config.render_lod_reference_height_px);
    let projection_scale = camera.projection_scale
        * (f64::from(reference_height_px) / f64::from(camera.viewport_height_px.max(1.0)));

    (f64::from(meta.metrics.geometric_error) * projection_scale / distance) as f32
}
