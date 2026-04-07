use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaxLodPolicyKind {
    RadiusDerived,
    Fixed,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeConfig {
    pub max_lod_policy: MaxLodPolicyKind,
    pub max_lod: u8,
    pub max_lod_cap: u8,
    pub metadata_precompute_max_lod: u8,
    pub dense_metadata_prebuild_max_lod: u8,
    pub payload_precompute_max_lod: u8,
    pub worker_thread_count: usize,
    pub planet_seed: u64,
    pub cube_projection: CubeProjection,
    pub visibility_strategy: VisibilityStrategyKind,
    pub enable_frustum_culling: bool,
    pub keep_coarse_lod_chunks_rendered: bool,
    pub render_backend: RenderBackendKind,
    pub staging_policy: PackedStagingPolicyKind,
    pub enable_godot_staging: bool,
    pub use_large_world_coordinates: bool,
    pub origin_recenter_distance: f64,
    pub planet_radius: f64,
    pub height_amplitude: f64,
    pub render_lod_reference_height_px: f32,
    pub target_render_chunks: usize,
    pub hard_render_chunk_cap: usize,
    pub split_threshold_px: f32,
    pub merge_threshold_px: f32,
    pub horizon_safety_margin: f64,
    pub physics_activation_radius: f64,
    pub physics_max_active_chunks: usize,
    pub commit_budget_per_frame: usize,
    pub upload_budget_bytes_per_frame: usize,
    pub render_activation_budget_per_frame: usize,
    pub render_update_budget_per_frame: usize,
    pub render_deactivation_budget_per_frame: usize,
    pub physics_activation_budget_per_frame: usize,
    pub physics_deactivation_budget_per_frame: usize,
    pub render_material_class: u8,
    pub render_format_mask: u64,
    pub render_vertex_stride: usize,
    pub render_attribute_stride: usize,
    pub render_index_stride: usize,
    pub render_pool_watermark_per_class: usize,
    pub physics_pool_watermark: usize,
    pub asset_placement_cells_per_axis: u32,
    pub asset_group_chunk_span: u32,
}

impl RuntimeConfig {
    pub fn normalized(mut self) -> Self {
        self.max_lod_cap = self.max_lod_cap.min(topology::MAX_SUPPORTED_MAX_LOD);
        self.max_lod = match self.max_lod_policy {
            MaxLodPolicyKind::RadiusDerived => radius_derived_max_lod_for_planet_radius(
                self.planet_radius,
                DEFAULT_MIN_AVERAGE_CHUNK_SURFACE_SPAN_METERS,
                self.max_lod_cap,
            ),
            MaxLodPolicyKind::Fixed => self.max_lod.min(self.max_lod_cap),
        };
        self.metadata_precompute_max_lod = self.metadata_precompute_max_lod.min(self.max_lod);
        self.dense_metadata_prebuild_max_lod =
            self.dense_metadata_prebuild_max_lod.min(self.max_lod);
        self.payload_precompute_max_lod = self.payload_precompute_max_lod.min(self.max_lod);
        self.render_lod_reference_height_px = self.render_lod_reference_height_px.max(1.0);
        self.hard_render_chunk_cap = self.hard_render_chunk_cap.max(Face::ALL.len());
        self.target_render_chunks = self
            .target_render_chunks
            .max(Face::ALL.len())
            .min(self.hard_render_chunk_cap);
        self
    }
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        let terrain = TerrainFieldSettings::default();
        let worker_thread_count = thread::available_parallelism()
            .map(|count| count.get().clamp(1, DEFAULT_MAX_WORKER_THREADS))
            .unwrap_or(1);
        let max_lod = radius_derived_max_lod_for_planet_radius(
            terrain.planet_radius,
            DEFAULT_MIN_AVERAGE_CHUNK_SURFACE_SPAN_METERS,
            topology::DEFAULT_MAX_LOD,
        );

        Self {
            max_lod_policy: MaxLodPolicyKind::RadiusDerived,
            max_lod,
            max_lod_cap: topology::DEFAULT_MAX_LOD,
            metadata_precompute_max_lod: DEFAULT_DENSE_METADATA_PREBUILD_MAX_LOD.min(max_lod),
            dense_metadata_prebuild_max_lod: DEFAULT_DENSE_METADATA_PREBUILD_MAX_LOD.min(max_lod),
            payload_precompute_max_lod: PAYLOAD_PRECOMPUTE_MAX_LOD.min(max_lod),
            worker_thread_count,
            planet_seed: DEFAULT_PLANET_SEED,
            cube_projection: CubeProjection::Spherified,
            visibility_strategy: VisibilityStrategyKind::HorizonFrustumLod,
            enable_frustum_culling: true,
            keep_coarse_lod_chunks_rendered: false,
            render_backend: RenderBackendKind::GpuDisplacedCanonical,
            staging_policy: PackedStagingPolicyKind::GodotOwnedReuse,
            enable_godot_staging: true,
            use_large_world_coordinates: false,
            origin_recenter_distance: DEFAULT_ORIGIN_RECENTER_DISTANCE,
            planet_radius: terrain.planet_radius,
            height_amplitude: terrain.height_amplitude,
            render_lod_reference_height_px: DEFAULT_RENDER_LOD_REFERENCE_HEIGHT_PX,
            target_render_chunks: DEFAULT_TARGET_RENDER_CHUNKS,
            hard_render_chunk_cap: DEFAULT_HARD_RENDER_CHUNK_CAP,
            split_threshold_px: DEFAULT_SPLIT_THRESHOLD_PX,
            merge_threshold_px: DEFAULT_MERGE_THRESHOLD_PX,
            horizon_safety_margin: DEFAULT_HORIZON_SAFETY_MARGIN,
            physics_activation_radius: DEFAULT_PHYSICS_ACTIVATION_RADIUS,
            physics_max_active_chunks: DEFAULT_PHYSICS_MAX_ACTIVE_CHUNKS,
            commit_budget_per_frame: DEFAULT_COMMIT_BUDGET_PER_FRAME,
            upload_budget_bytes_per_frame: DEFAULT_UPLOAD_BUDGET_BYTES_PER_FRAME,
            render_activation_budget_per_frame: DEFAULT_RENDER_ACTIVATION_BUDGET_PER_FRAME,
            render_update_budget_per_frame: DEFAULT_RENDER_UPDATE_BUDGET_PER_FRAME,
            render_deactivation_budget_per_frame: DEFAULT_RENDER_DEACTIVATION_BUDGET_PER_FRAME,
            physics_activation_budget_per_frame: DEFAULT_PHYSICS_ACTIVATION_BUDGET_PER_FRAME,
            physics_deactivation_budget_per_frame: DEFAULT_PHYSICS_DEACTIVATION_BUDGET_PER_FRAME,
            render_material_class: DEFAULT_RENDER_MATERIAL_CLASS,
            render_format_mask: DEFAULT_RENDER_FORMAT_MASK,
            render_vertex_stride: DEFAULT_RENDER_VERTEX_STRIDE,
            render_attribute_stride: DEFAULT_RENDER_ATTRIBUTE_STRIDE,
            render_index_stride: DEFAULT_RENDER_INDEX_STRIDE,
            render_pool_watermark_per_class: DEFAULT_RENDER_POOL_WATERMARK_PER_CLASS,
            physics_pool_watermark: DEFAULT_PHYSICS_POOL_WATERMARK,
            asset_placement_cells_per_axis: DEFAULT_ASSET_PLACEMENT_CELLS_PER_AXIS,
            asset_group_chunk_span: DEFAULT_ASSET_GROUP_CHUNK_SPAN,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CameraState {
    pub position_planet: DVec3,
    pub forward_planet: DVec3,
    pub frustum_planes: [Plane; 6],
    pub projection_scale: f64,
    pub viewport_height_px: f32,
    pub origin: OriginSnapshot,
}

impl CameraState {
    pub fn from_godot(
        transform: Transform3D,
        frustum_planes: [Plane; 6],
        fov_y_degrees: f32,
        viewport_height_px: f32,
        origin: OriginSnapshot,
    ) -> Self {
        let position_planet = vector3_to_dvec3(transform.origin) + origin.render_origin_planet;
        let forward_planet = -vector3_to_dvec3(transform.basis.col_c()).normalize_or_zero();
        let half_fov_radians = f64::from(fov_y_degrees).to_radians() * 0.5;
        let viewport_height_px = viewport_height_px.max(1.0);
        let projection_scale =
            f64::from(viewport_height_px) / (2.0 * half_fov_radians.tan().max(1.0e-6));

        Self {
            position_planet,
            forward_planet,
            frustum_planes,
            projection_scale,
            viewport_height_px,
            origin,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CommitOpKind {
    ActivateRender,
    UpdateRender,
    DeactivateRender,
    ActivatePhysics,
    DeactivatePhysics,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DeferredOpKey {
    pub kind: CommitOpKind,
    pub key: ChunkKey,
}

impl DeferredOpKey {
    pub fn new(kind: CommitOpKind, key: ChunkKey) -> Self {
        Self { kind, key }
    }
}
