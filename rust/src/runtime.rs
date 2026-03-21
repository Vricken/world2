use std::collections::{HashMap, HashSet, VecDeque};

use glam::{DVec2, DVec3};
use godot::builtin::{
    Array, Color, Dictionary, PackedByteArray, PackedColorArray, PackedInt32Array,
    PackedVector2Array, PackedVector3Array, Plane, Rid, StringName, Transform3D, Variant,
    Vector2, Vector3,
};
use godot::classes::physics_server_3d::{BodyMode, BodyState};
use godot::classes::rendering_server::PrimitiveType;
use godot::classes::{PhysicsServer3D, RenderingServer};
use godot::meta::ToGodot;
use godot::obj::Singleton;

use crate::geometry::{
    chunk_uv_to_face_uv, cube_point_for_face, face_uv_to_signed_coords, CubeProjection,
    TerrainFieldSettings,
};
use crate::mesh_topology::{self, StitchError, CANONICAL_TOPOLOGY_CLASS};
use crate::topology::{self, TopologyError};

/// Treat Rust->Godot packed-array transfer as copy-possible unless the docs
/// explicitly guarantee a zero-copy ownership handoff for the exact API used.
pub const DEFAULT_METADATA_PRECOMPUTE_MAX_LOD: u8 = 5;
pub const PAYLOAD_PRECOMPUTE_MAX_LOD: u8 = 5;
pub const DEFAULT_SPLIT_THRESHOLD_PX: f32 = 8.0;
pub const DEFAULT_MERGE_THRESHOLD_PX: f32 = 4.0;
pub const DEFAULT_PHYSICS_ACTIVATION_RADIUS: f64 = 3_000.0;
pub const DEFAULT_COMMIT_BUDGET_PER_FRAME: usize = 24;
pub const DEFAULT_UPLOAD_BUDGET_BYTES_PER_FRAME: usize = 8 * 1024 * 1024;
pub const DEFAULT_RENDER_MATERIAL_CLASS: u8 = 0;
pub const DEFAULT_RENDER_FORMAT_MASK: u64 = 0x1B;
pub const DEFAULT_RENDER_VERTEX_STRIDE: usize = 12;
pub const DEFAULT_RENDER_ATTRIBUTE_STRIDE: usize = 24;
pub const DEFAULT_RENDER_INDEX_STRIDE: usize = 4;
pub const DEFAULT_RENDER_POOL_WATERMARK_PER_CLASS: usize = 8;
pub const DEFAULT_PHYSICS_POOL_WATERMARK: usize = 32;
const PACKED_NORMAL_BYTES: usize = 12;
const PACKED_UV_BYTES: usize = 8;
const PACKED_COLOR_BYTES: usize = 4;
const PACKED_COLOR_OFFSET: usize = PACKED_NORMAL_BYTES + PACKED_UV_BYTES;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Face {
    Px,
    Nx,
    Py,
    Ny,
    Pz,
    Nz,
}

impl Face {
    pub const ALL: [Self; 6] = [Self::Px, Self::Nx, Self::Py, Self::Ny, Self::Pz, Self::Nz];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Edge {
    NegU,
    PosU,
    NegV,
    PosV,
}

impl Edge {
    pub const ALL: [Self; 4] = [Self::NegU, Self::PosU, Self::NegV, Self::PosV];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChunkKey {
    pub face: Face,
    pub lod: u8,
    pub x: u32,
    pub y: u32,
}

impl ChunkKey {
    pub fn new(face: Face, lod: u8, x: u32, y: u32) -> Self {
        Self { face, lod, x, y }
    }

    pub fn resolution_for_lod(lod: u8) -> u32 {
        1_u32.checked_shl(u32::from(lod)).unwrap_or(0)
    }

    pub fn is_valid_for_lod(&self) -> bool {
        let resolution = Self::resolution_for_lod(self.lod);
        resolution > 0 && self.x < resolution && self.y < resolution
    }

    pub fn parent(&self) -> Option<Self> {
        (self.lod > 0).then(|| Self::new(self.face, self.lod - 1, self.x / 2, self.y / 2))
    }

    pub fn children(&self) -> Option<[Self; 4]> {
        let child_lod = self.lod.checked_add(1)?;
        let base_x = self.x.checked_mul(2)?;
        let base_y = self.y.checked_mul(2)?;

        Some([
            Self::new(self.face, child_lod, base_x, base_y),
            Self::new(self.face, child_lod, base_x + 1, base_y),
            Self::new(self.face, child_lod, base_x, base_y + 1),
            Self::new(self.face, child_lod, base_x + 1, base_y + 1),
        ])
    }

    pub fn is_descendant_of(&self, ancestor: &Self) -> bool {
        if self.face != ancestor.face || self.lod < ancestor.lod {
            return false;
        }

        let shift = u32::from(self.lod - ancestor.lod);
        (self.x >> shift) == ancestor.x && (self.y >> shift) == ancestor.y
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChunkBounds {
    pub center_planet: DVec3,
    pub radius: f64,
    pub min_height: f32,
    pub max_height: f32,
    pub min_radius: f64,
    pub max_radius: f64,
}

impl ChunkBounds {
    pub fn new(
        center_planet: DVec3,
        radius: f64,
        min_height: f32,
        max_height: f32,
        min_radius: f64,
        max_radius: f64,
    ) -> Self {
        Self {
            center_planet,
            radius,
            min_height,
            max_height,
            min_radius,
            max_radius,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChunkMetrics {
    pub geometric_error: f32,
    pub max_slope_deg: f32,
    pub angular_radius: f32,
}

impl ChunkMetrics {
    pub fn new(geometric_error: f32, max_slope_deg: f32, angular_radius: f32) -> Self {
        Self {
            geometric_error,
            max_slope_deg,
            angular_radius,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChunkNeighbors {
    pub same_lod: [ChunkKey; 4],
}

impl ChunkNeighbors {
    pub fn new(same_lod: [ChunkKey; 4]) -> Self {
        Self { same_lod }
    }

    pub fn get(&self, edge: Edge) -> ChunkKey {
        match edge {
            Edge::NegU => self.same_lod[0],
            Edge::PosU => self.same_lod[1],
            Edge::NegV => self.same_lod[2],
            Edge::PosV => self.same_lod[3],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SurfaceClassKey {
    pub topology_class: u16,
    pub stitch_mask: u8,
    pub index_class: u8,
    pub material_class: u8,
    pub vertex_count: u32,
    pub index_count: u32,
    pub format_mask: u64,
    pub vertex_stride: usize,
    pub attribute_stride: usize,
    pub index_stride: usize,
    pub vertex_bytes: usize,
    pub attribute_bytes: usize,
    pub index_bytes: usize,
}

impl SurfaceClassKey {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        topology_class: u16,
        stitch_mask: u8,
        index_class: u8,
        material_class: u8,
        vertex_count: u32,
        index_count: u32,
        format_mask: u64,
        vertex_stride: usize,
        attribute_stride: usize,
        index_stride: usize,
    ) -> Self {
        Self {
            topology_class,
            stitch_mask,
            index_class,
            material_class,
            vertex_count,
            index_count,
            format_mask,
            vertex_stride,
            attribute_stride,
            index_stride,
            vertex_bytes: vertex_stride.saturating_mul(vertex_count as usize),
            attribute_bytes: attribute_stride.saturating_mul(vertex_count as usize),
            index_bytes: index_stride.saturating_mul(index_count as usize),
        }
    }

    pub fn canonical_chunk(
        stitch_mask: u8,
        material_class: u8,
        format_mask: u64,
        vertex_stride: usize,
        attribute_stride: usize,
        index_stride: usize,
    ) -> Result<Self, StitchError> {
        let topology = mesh_topology::canonical_chunk_topology();
        let index_count = topology.index_count_for_mask(stitch_mask)?;

        Ok(Self::new(
            CANONICAL_TOPOLOGY_CLASS,
            stitch_mask,
            stitch_mask,
            material_class,
            topology.vertex_count(),
            index_count,
            format_mask,
            vertex_stride,
            attribute_stride,
            index_stride,
        ))
    }

    pub fn compatibility_issues(&self, other: &Self) -> Vec<&'static str> {
        let mut issues = Vec::new();

        if self.topology_class != other.topology_class {
            issues.push("topology_class");
        }
        if self.stitch_mask != other.stitch_mask {
            issues.push("stitch_mask");
        }
        if self.index_class != other.index_class {
            issues.push("index_class");
        }
        if self.material_class != other.material_class {
            issues.push("material_class");
        }
        if self.vertex_count != other.vertex_count {
            issues.push("vertex_count");
        }
        if self.index_count != other.index_count {
            issues.push("index_count");
        }
        if self.format_mask != other.format_mask {
            issues.push("format_mask");
        }
        if self.vertex_stride != other.vertex_stride {
            issues.push("vertex_stride");
        }
        if self.attribute_stride != other.attribute_stride {
            issues.push("attribute_stride");
        }
        if self.index_stride != other.index_stride {
            issues.push("index_stride");
        }
        if self.vertex_bytes != other.vertex_bytes {
            issues.push("vertex_bytes");
        }
        if self.attribute_bytes != other.attribute_bytes {
            issues.push("attribute_bytes");
        }
        if self.index_bytes != other.index_bytes {
            issues.push("index_bytes");
        }

        issues
    }

    pub fn is_pool_compatible_with(&self, other: &Self) -> bool {
        self.compatibility_issues(other).is_empty()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChunkMeta {
    pub key: ChunkKey,
    pub bounds: ChunkBounds,
    pub metrics: ChunkMetrics,
    pub neighbors: ChunkNeighbors,
    pub surface_class: SurfaceClassKey,
}

impl ChunkMeta {
    pub fn new(
        key: ChunkKey,
        bounds: ChunkBounds,
        metrics: ChunkMetrics,
        surface_class: SurfaceClassKey,
    ) -> Result<Self, TopologyError> {
        Ok(Self {
            key,
            bounds,
            metrics,
            neighbors: topology::same_lod_neighbors(key)?,
            surface_class,
        })
    }

    pub fn refresh_same_lod_neighbors(&mut self) -> Result<(), TopologyError> {
        self.neighbors = topology::same_lod_neighbors(self.key)?;
        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CpuMeshBuffers {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub tangents: Vec<[f32; 4]>,
    pub uvs: Vec<[f32; 2]>,
    pub colors: Vec<[f32; 4]>,
    pub indices: Vec<i32>,
}

impl CpuMeshBuffers {
    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    pub fn index_count(&self) -> usize {
        self.indices.len()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChunkSample {
    pub unit_dir: DVec3,
    pub height: f32,
    pub biome0: f32,
    pub biome1: f32,
    pub slope_hint: f32,
}

impl ChunkSample {
    pub fn displaced_point(&self, planet_radius: f64) -> DVec3 {
        self.unit_dir * (planet_radius + f64::from(self.height))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChunkSampleGrid {
    pub samples_per_edge: u32,
    pub samples: Vec<ChunkSample>,
}

impl ChunkSampleGrid {
    pub fn new(samples_per_edge: u32, samples: Vec<ChunkSample>) -> Self {
        Self {
            samples_per_edge,
            samples,
        }
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn get(&self, x: u32, y: u32) -> &ChunkSample {
        let index = (y * self.samples_per_edge + x) as usize;
        &self.samples[index]
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PackedMeshRegions {
    pub vertex_region: Vec<u8>,
    pub attribute_region: Vec<u8>,
    pub index_region: Vec<u8>,
    pub vertex_stride: usize,
    pub attribute_stride: usize,
    pub index_stride: usize,
}

impl PackedMeshRegions {
    pub fn validate_for_surface_class(
        &self,
        surface_class: &SurfaceClassKey,
    ) -> Result<(), &'static str> {
        if self.vertex_stride != surface_class.vertex_stride {
            return Err("vertex_stride");
        }
        if self.attribute_stride != surface_class.attribute_stride {
            return Err("attribute_stride");
        }
        if self.index_stride != surface_class.index_stride {
            return Err("index_stride");
        }
        if self.vertex_region.len() != surface_class.vertex_bytes {
            return Err("vertex_bytes");
        }
        if self.attribute_region.len() != surface_class.attribute_bytes {
            return Err("attribute_bytes");
        }
        if self.index_region.len() != surface_class.index_bytes {
            return Err("index_bytes");
        }

        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GdPackedStaging {
    pub vertex_region: PackedByteArray,
    pub attribute_region: PackedByteArray,
    pub index_region: PackedByteArray,
}

impl GdPackedStaging {
    pub fn new_for_surface_class(surface_class: &SurfaceClassKey) -> Self {
        let mut vertex_region = PackedByteArray::new();
        vertex_region.resize(surface_class.vertex_bytes);

        let mut attribute_region = PackedByteArray::new();
        attribute_region.resize(surface_class.attribute_bytes);

        let mut index_region = PackedByteArray::new();
        index_region.resize(surface_class.index_bytes);

        Self {
            vertex_region,
            attribute_region,
            index_region,
        }
    }

    pub fn copy_from_regions(
        &mut self,
        regions: &PackedMeshRegions,
        surface_class: &SurfaceClassKey,
    ) -> Result<(), &'static str> {
        regions.validate_for_surface_class(surface_class)?;

        if self.vertex_region.len() != surface_class.vertex_bytes {
            return Err("vertex_staging_bytes");
        }
        if self.attribute_region.len() != surface_class.attribute_bytes {
            return Err("attribute_staging_bytes");
        }
        if self.index_region.len() != surface_class.index_bytes {
            return Err("index_staging_bytes");
        }

        self.vertex_region
            .as_mut_slice()
            .copy_from_slice(regions.vertex_region.as_slice());
        self.attribute_region
            .as_mut_slice()
            .copy_from_slice(regions.attribute_region.as_slice());
        self.index_region
            .as_mut_slice()
            .copy_from_slice(regions.index_region.as_slice());

        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AssetInstance {
    pub family_id: u16,
    pub origin: DVec3,
    pub basis_x: DVec3,
    pub basis_y: DVec3,
    pub basis_z: DVec3,
    pub scale: f32,
    pub color_seed: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenderLifecycleCommand {
    WarmReuseCurrent,
    WarmReusePooled,
    ColdCreate(RenderFallbackReason),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChunkPayload {
    pub surface_class: SurfaceClassKey,
    pub stitch_mask: u8,
    pub sample_count: usize,
    pub mesh: CpuMeshBuffers,
    pub packed_regions: Option<PackedMeshRegions>,
    pub gd_staging: Option<GdPackedStaging>,
    pub pooled_render_entry: Option<RenderPoolEntry>,
    pub assets: Vec<AssetInstance>,
    pub collider_vertices: Option<Vec<[f32; 3]>>,
    pub collider_indices: Option<Vec<i32>>,
    pub render_transform: Transform3D,
    pub render_lifecycle: RenderLifecycleCommand,
}

impl Default for ChunkPayload {
    fn default() -> Self {
        let surface_class = SurfaceClassKey::canonical_chunk(
            mesh_topology::BASE_STITCH_MASK,
            DEFAULT_RENDER_MATERIAL_CLASS,
            DEFAULT_RENDER_FORMAT_MASK,
            DEFAULT_RENDER_VERTEX_STRIDE,
            DEFAULT_RENDER_ATTRIBUTE_STRIDE,
            DEFAULT_RENDER_INDEX_STRIDE,
        )
        .expect("default surface class must be valid");

        Self {
            surface_class,
            stitch_mask: mesh_topology::BASE_STITCH_MASK,
            sample_count: 0,
            mesh: CpuMeshBuffers::default(),
            packed_regions: None,
            gd_staging: None,
            pooled_render_entry: None,
            assets: Vec::new(),
            collider_vertices: None,
            collider_indices: None,
            render_transform: Transform3D::IDENTITY,
            render_lifecycle: RenderLifecycleCommand::ColdCreate(
                RenderFallbackReason::MissingCurrentSurfaceClass,
            ),
        }
    }
}

impl ChunkPayload {
    pub fn upload_bytes(&self) -> usize {
        self.packed_regions
            .as_ref()
            .map(|regions| {
                regions.vertex_region.len()
                    + regions.attribute_region.len()
                    + regions.index_region.len()
            })
            .unwrap_or_else(|| {
                self.surface_class.vertex_bytes
                    + self.surface_class.attribute_bytes
                    + self.surface_class.index_bytes
            })
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChunkRidState {
    pub mesh_rid: Option<Rid>,
    pub render_instance_rid: Option<Rid>,
    pub physics_body_rid: Option<Rid>,
    pub physics_shape_rid: Option<Rid>,
    pub asset_multimesh_rids: Vec<Rid>,
    pub asset_instance_rids: Vec<Rid>,
    pub render_resident: bool,
    pub physics_resident: bool,
    pub active_surface_class: Option<SurfaceClassKey>,
    pub gd_staging: Option<GdPackedStaging>,
    pub pooled_surface_class: Option<SurfaceClassKey>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderPoolEntry {
    pub mesh_rid: Rid,
    pub render_instance_rid: Rid,
    pub surface_class: SurfaceClassKey,
    pub gd_staging: Option<GdPackedStaging>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PhysicsPoolEntry {
    pub physics_body_rid: Rid,
    pub physics_shape_rid: Rid,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RenderWarmPath {
    ReuseCurrentSurface,
    ReusePooledSurface(RenderPoolEntry),
    ColdPath(RenderFallbackReason),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenderFallbackReason {
    MissingCurrentSurfaceClass,
    IncompatibleCurrentSurfaceClass(Vec<&'static str>),
    NoCompatiblePooledSurface,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeConfig {
    pub max_lod: u8,
    pub metadata_precompute_max_lod: u8,
    pub payload_precompute_max_lod: u8,
    pub enable_godot_staging: bool,
    pub planet_radius: f64,
    pub height_amplitude: f64,
    pub split_threshold_px: f32,
    pub merge_threshold_px: f32,
    pub horizon_safety_margin: f64,
    pub physics_activation_radius: f64,
    pub commit_budget_per_frame: usize,
    pub upload_budget_bytes_per_frame: usize,
    pub render_material_class: u8,
    pub render_format_mask: u64,
    pub render_vertex_stride: usize,
    pub render_attribute_stride: usize,
    pub render_index_stride: usize,
    pub render_pool_watermark_per_class: usize,
    pub physics_pool_watermark: usize,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        let terrain = TerrainFieldSettings::default();

        Self {
            max_lod: topology::DEFAULT_MAX_LOD,
            metadata_precompute_max_lod: DEFAULT_METADATA_PRECOMPUTE_MAX_LOD,
            payload_precompute_max_lod: PAYLOAD_PRECOMPUTE_MAX_LOD,
            enable_godot_staging: true,
            planet_radius: terrain.planet_radius,
            height_amplitude: terrain.height_amplitude,
            split_threshold_px: DEFAULT_SPLIT_THRESHOLD_PX,
            merge_threshold_px: DEFAULT_MERGE_THRESHOLD_PX,
            horizon_safety_margin: f64::max(100.0, terrain.planet_radius * 0.00005),
            physics_activation_radius: DEFAULT_PHYSICS_ACTIVATION_RADIUS,
            commit_budget_per_frame: DEFAULT_COMMIT_BUDGET_PER_FRAME,
            upload_budget_bytes_per_frame: DEFAULT_UPLOAD_BUDGET_BYTES_PER_FRAME,
            render_material_class: DEFAULT_RENDER_MATERIAL_CLASS,
            render_format_mask: DEFAULT_RENDER_FORMAT_MASK,
            render_vertex_stride: DEFAULT_RENDER_VERTEX_STRIDE,
            render_attribute_stride: DEFAULT_RENDER_ATTRIBUTE_STRIDE,
            render_index_stride: DEFAULT_RENDER_INDEX_STRIDE,
            render_pool_watermark_per_class: DEFAULT_RENDER_POOL_WATERMARK_PER_CLASS,
            physics_pool_watermark: DEFAULT_PHYSICS_POOL_WATERMARK,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CameraState {
    pub position_planet: DVec3,
    pub forward_planet: DVec3,
    pub frustum_planes: [Plane; 6],
    pub projection_scale: f64,
}

impl CameraState {
    pub fn from_godot(
        transform: Transform3D,
        frustum_planes: [Plane; 6],
        fov_y_degrees: f32,
        viewport_height_px: f32,
    ) -> Self {
        let position_planet = vector3_to_dvec3(transform.origin);
        let forward_planet = -vector3_to_dvec3(transform.basis.col_c()).normalize_or_zero();
        let half_fov_radians = f64::from(fov_y_degrees).to_radians() * 0.5;
        let projection_scale =
            f64::from(viewport_height_px.max(1.0)) / (2.0 * half_fov_radians.tan().max(1.0e-6));

        Self {
            position_planet,
            forward_planet,
            frustum_planes,
            projection_scale,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CommitOp {
    kind: CommitOpKind,
    key: ChunkKey,
    upload_bytes: usize,
    priority_group: u8,
    distance_key_mm: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SelectionFrameState {
    pub tick: u64,
    pub desired_render_count: usize,
    pub desired_physics_count: usize,
    pub horizon_survivor_count: usize,
    pub frustum_survivor_count: usize,
    pub selected_leaf_count: usize,
    pub neighbor_split_count: usize,
    pub queued_commit_ops: usize,
    pub committed_commit_ops: usize,
    pub deferred_commit_ops: usize,
    pub upload_bytes_committed: usize,
    pub upload_bytes_deferred: usize,
    pub max_deferred_starvation_frames: u32,
    pub phase7_sampled_chunks: usize,
    pub phase7_meshed_chunks: usize,
    pub phase7_packed_chunks: usize,
    pub phase7_staged_chunks: usize,
    pub phase7_commit_payloads: usize,
    pub phase7_lifecycle_commands: usize,
    pub phase7_warm_current_reuse_hits: usize,
    pub phase7_warm_pool_reuse_hits: usize,
    pub phase7_cold_fallbacks: usize,
    pub phase8_render_warm_current_commits: usize,
    pub phase8_render_warm_pool_commits: usize,
    pub phase8_render_cold_commits: usize,
    pub phase8_physics_commits: usize,
    pub phase8_fallback_missing_current_surface_class: usize,
    pub phase8_fallback_incompatible_current_surface_class: usize,
    pub phase8_fallback_no_compatible_pooled_surface: usize,
    pub render_pool_entries: usize,
    pub physics_pool_entries: usize,
}

#[derive(Debug)]
pub struct PlanetRuntime {
    pub config: RuntimeConfig,
    pub scenario_rid: Rid,
    pub physics_space_rid: Rid,
    pub meta: HashMap<ChunkKey, ChunkMeta>,
    pub active_render: HashSet<ChunkKey>,
    pub active_physics: HashSet<ChunkKey>,
    pub resident_payloads: HashMap<ChunkKey, ChunkPayload>,
    pub rid_state: HashMap<ChunkKey, ChunkRidState>,
    pub render_pool: HashMap<SurfaceClassKey, VecDeque<RenderPoolEntry>>,
    pub physics_pool: VecDeque<PhysicsPoolEntry>,
    pub frame_state: SelectionFrameState,
    pub deferred_starvation: HashMap<DeferredOpKey, u32>,
}

impl Default for PlanetRuntime {
    fn default() -> Self {
        Self::new(RuntimeConfig::default(), Rid::Invalid, Rid::Invalid)
    }
}

impl PlanetRuntime {
    pub fn new(config: RuntimeConfig, scenario_rid: Rid, physics_space_rid: Rid) -> Self {
        let mut runtime = Self {
            config,
            scenario_rid,
            physics_space_rid,
            meta: HashMap::new(),
            active_render: HashSet::new(),
            active_physics: HashSet::new(),
            resident_payloads: HashMap::new(),
            rid_state: HashMap::new(),
            render_pool: HashMap::new(),
            physics_pool: VecDeque::new(),
            frame_state: SelectionFrameState::default(),
            deferred_starvation: HashMap::new(),
        };
        runtime
            .build_metadata_tree_through_lod(
                runtime
                    .config
                    .metadata_precompute_max_lod
                    .min(runtime.config.max_lod),
            )
            .expect("phase 7 metadata prebuild keys must always be valid");
        runtime
    }

    pub fn metadata_precompute_max_lod(&self) -> u8 {
        self.config.metadata_precompute_max_lod
    }

    pub fn payload_precompute_max_lod(&self) -> u8 {
        self.config.payload_precompute_max_lod
    }

    pub fn should_precompute_payload_for_lod(&self, lod: u8) -> bool {
        lod <= self.payload_precompute_max_lod()
    }

    pub fn has_valid_world_rids(&self) -> bool {
        self.scenario_rid != Rid::Invalid && self.physics_space_rid != Rid::Invalid
    }

    pub fn set_world_rids(&mut self, scenario_rid: Rid, physics_space_rid: Rid) {
        self.scenario_rid = scenario_rid;
        self.physics_space_rid = physics_space_rid;
    }

    pub fn frame_state(&self) -> &SelectionFrameState {
        &self.frame_state
    }

    pub fn build_metadata_tree_through_lod(&mut self, max_lod: u8) -> Result<usize, TopologyError> {
        let mut inserted = 0usize;

        for face in Face::ALL {
            for lod in 0..=max_lod.min(self.config.max_lod) {
                let resolution = ChunkKey::resolution_for_lod(lod);
                for y in 0..resolution {
                    for x in 0..resolution {
                        let key = ChunkKey::new(face, lod, x, y);
                        if self.meta.contains_key(&key) {
                            continue;
                        }

                        let meta = self.build_chunk_meta(key)?;
                        self.meta.insert(key, meta);
                        inserted += 1;
                    }
                }
            }
        }

        Ok(inserted)
    }

    pub fn ensure_chunk_meta(&mut self, key: ChunkKey) -> Result<&ChunkMeta, TopologyError> {
        if !self.meta.contains_key(&key) {
            let meta = self.build_chunk_meta(key)?;
            self.meta.insert(key, meta);
        }

        Ok(self
            .meta
            .get(&key)
            .expect("chunk metadata must exist after ensure_chunk_meta"))
    }

    pub fn step_visibility_selection(&mut self, camera: &CameraState) -> Result<(), TopologyError> {
        let mut frame_state = SelectionFrameState {
            tick: self.frame_state.tick.saturating_add(1),
            ..SelectionFrameState::default()
        };
        let desired_render = self.select_render_set(camera, &mut frame_state)?;
        let desired_physics = self.select_physics_set(camera, &desired_render)?;

        frame_state.desired_render_count = desired_render.len();
        frame_state.desired_physics_count = desired_physics.len();

        self.apply_budgeted_diffs(&desired_render, &desired_physics, camera, &mut frame_state)?;
        self.frame_state = frame_state;

        Ok(())
    }

    fn build_chunk_meta(&self, key: ChunkKey) -> Result<ChunkMeta, TopologyError> {
        let sample_dirs = self.chunk_sample_directions(key)?;
        let center_dir = sample_dirs
            .iter()
            .copied()
            .fold(DVec3::ZERO, |sum, dir| sum + dir)
            .normalize_or_zero();
        let min_radius = self.config.planet_radius - self.config.height_amplitude;
        let max_radius = self.config.planet_radius + self.config.height_amplitude;
        let center_planet = center_dir * self.config.planet_radius;

        let angular_radius = sample_dirs
            .iter()
            .copied()
            .map(|dir| center_dir.angle_between(dir))
            .fold(0.0_f64, f64::max);
        let radius = sample_dirs
            .iter()
            .copied()
            .flat_map(|dir| {
                [
                    (dir * min_radius - center_planet).length(),
                    (dir * max_radius - center_planet).length(),
                ]
            })
            .fold(0.0_f64, f64::max);
        let geometric_error =
            (2.0 * max_radius * angular_radius / f64::from(mesh_topology::QUADS_PER_EDGE)) as f32;
        let surface_class = SurfaceClassKey::canonical_chunk(
            mesh_topology::BASE_STITCH_MASK,
            self.config.render_material_class,
            self.config.render_format_mask,
            self.config.render_vertex_stride,
            self.config.render_attribute_stride,
            self.config.render_index_stride,
        )
        .expect("base stitch mask must always yield a surface class");

        ChunkMeta::new(
            key,
            ChunkBounds::new(
                center_planet,
                radius,
                -(self.config.height_amplitude as f32),
                self.config.height_amplitude as f32,
                min_radius,
                max_radius,
            ),
            ChunkMetrics::new(geometric_error, 0.0, angular_radius as f32),
            surface_class,
        )
    }

    fn chunk_sample_directions(&self, key: ChunkKey) -> Result<[DVec3; 9], TopologyError> {
        if !key.is_valid_for_lod() {
            return Err(TopologyError::InvalidChunkKey);
        }

        let sample_uvs = [
            (0.0, 0.0),
            (0.5, 0.0),
            (1.0, 0.0),
            (0.0, 0.5),
            (0.5, 0.5),
            (1.0, 0.5),
            (0.0, 1.0),
            (0.5, 1.0),
            (1.0, 1.0),
        ];
        let mut sample_dirs = [DVec3::ZERO; 9];

        for (index, (u, v)) in sample_uvs.into_iter().enumerate() {
            let face_uv = chunk_uv_to_face_uv(key, glam::DVec2::new(u, v))
                .map_err(|_| TopologyError::InvalidChunkKey)?;
            let face_st = face_uv_to_signed_coords(face_uv);
            let cube_point = cube_point_for_face(key.face, face_st);
            sample_dirs[index] = CubeProjection::Spherified.project_cube_point(cube_point);
        }

        Ok(sample_dirs)
    }

    fn sample_chunk_scalar_field(&self, key: ChunkKey) -> Result<ChunkSampleGrid, TopologyError> {
        if !key.is_valid_for_lod() {
            return Err(TopologyError::InvalidChunkKey);
        }

        let samples_per_edge = mesh_topology::SAMPLED_VERTICES_PER_EDGE;
        let visible_quads = f64::from(mesh_topology::QUADS_PER_EDGE);
        let border = f64::from(mesh_topology::BORDER_RING_QUADS);
        let mut samples = Vec::with_capacity((samples_per_edge * samples_per_edge) as usize);

        for y in 0..samples_per_edge {
            for x in 0..samples_per_edge {
                let chunk_uv = DVec2::new(
                    (f64::from(x) - border) / visible_quads,
                    (f64::from(y) - border) / visible_quads,
                );
                let face_uv = chunk_uv_to_face_uv(key, chunk_uv)
                    .map_err(|_| TopologyError::InvalidChunkKey)?;
                let cube_point = cube_point_for_face(key.face, face_uv_to_signed_coords(face_uv));
                let unit_dir = CubeProjection::Spherified
                    .project_cube_point(normalize_to_cube_surface(cube_point));
                let height = self
                    .terrain_settings()
                    .sample_height(unit_dir)
                    .clamp(-self.config.height_amplitude, self.config.height_amplitude)
                    as f32;
                let temperature = (1.0 - unit_dir.y.abs()) as f32;
                let moisture_signal =
                    (unit_dir.dot(DVec3::new(1.731, -0.613, 0.947)).sin() * 0.5 + 0.5) as f32;
                let biome0 = moisture_signal.clamp(0.0, 1.0);
                let biome1 = ((temperature * 0.75)
                    + ((height / self.config.height_amplitude as f32) * 0.25 + 0.25))
                    .clamp(0.0, 1.0);

                samples.push(ChunkSample {
                    unit_dir,
                    height,
                    biome0,
                    biome1,
                    slope_hint: 0.0,
                });
            }
        }

        let mut grid = ChunkSampleGrid::new(samples_per_edge, samples);
        self.fill_sample_slope_hints(&mut grid);

        Ok(grid)
    }

    fn fill_sample_slope_hints(&self, grid: &mut ChunkSampleGrid) {
        let samples_per_edge = grid.samples_per_edge;
        let heights = grid
            .samples
            .iter()
            .map(|sample| sample.height)
            .collect::<Vec<_>>();

        for y in 0..samples_per_edge {
            for x in 0..samples_per_edge {
                let left = heights[(clamp_grid_index(x as i32 - 1, samples_per_edge) as u32
                    + y * samples_per_edge) as usize];
                let right = heights[(clamp_grid_index(x as i32 + 1, samples_per_edge) as u32
                    + y * samples_per_edge) as usize];
                let down = heights[(x + clamp_grid_index(y as i32 - 1, samples_per_edge) as u32
                    * samples_per_edge) as usize];
                let up = heights[(x + clamp_grid_index(y as i32 + 1, samples_per_edge) as u32
                    * samples_per_edge) as usize];
                let gradient = ((right - left).powi(2) + (up - down).powi(2)).sqrt();
                let slope_hint = if self.config.height_amplitude <= f64::from(f32::EPSILON) {
                    0.0
                } else {
                    (gradient / (self.config.height_amplitude as f32 * 2.0)).clamp(0.0, 1.0)
                };
                let index = (y * samples_per_edge + x) as usize;
                grid.samples[index].slope_hint = slope_hint;
            }
        }
    }

    fn derive_cpu_mesh_buffers(
        &self,
        samples: &ChunkSampleGrid,
        stitch_mask: u8,
    ) -> Result<CpuMeshBuffers, StitchError> {
        let topology = mesh_topology::canonical_chunk_topology();
        let visible_edge = mesh_topology::VISIBLE_VERTICES_PER_EDGE;
        let mut mesh = CpuMeshBuffers {
            positions: Vec::with_capacity(topology.vertex_count() as usize),
            normals: Vec::with_capacity(topology.vertex_count() as usize),
            tangents: Vec::with_capacity(topology.vertex_count() as usize),
            uvs: Vec::with_capacity(topology.vertex_count() as usize),
            colors: Vec::with_capacity(topology.vertex_count() as usize),
            indices: topology.stitch_indices(stitch_mask)?.to_vec(),
        };

        for y in 0..visible_edge {
            for x in 0..visible_edge {
                let sample_x = x + mesh_topology::BORDER_RING_QUADS;
                let sample_y = y + mesh_topology::BORDER_RING_QUADS;
                let sample = samples.get(sample_x, sample_y);
                let displaced = sample.displaced_point(self.config.planet_radius);
                let left = samples
                    .get(sample_x - 1, sample_y)
                    .displaced_point(self.config.planet_radius);
                let right = samples
                    .get(sample_x + 1, sample_y)
                    .displaced_point(self.config.planet_radius);
                let down = samples
                    .get(sample_x, sample_y - 1)
                    .displaced_point(self.config.planet_radius);
                let up = samples
                    .get(sample_x, sample_y + 1)
                    .displaced_point(self.config.planet_radius);
                let tangent_u = (right - left).normalize_or_zero();
                let tangent_v = (up - down).normalize_or_zero();
                let normal = tangent_u.cross(tangent_v).normalize_or_zero();

                mesh.positions.push(dvec3_to_f32_array(displaced));
                mesh.normals.push(dvec3_to_f32_array(normal));
                mesh.tangents.push([
                    tangent_u.x as f32,
                    tangent_u.y as f32,
                    tangent_u.z as f32,
                    1.0,
                ]);
                mesh.uvs.push([
                    x as f32 / mesh_topology::QUADS_PER_EDGE as f32,
                    y as f32 / mesh_topology::QUADS_PER_EDGE as f32,
                ]);
                mesh.colors
                    .push([sample.biome0, sample.biome1, sample.slope_hint, 1.0]);
            }
        }

        Ok(mesh)
    }

    fn pack_mesh_regions(
        &self,
        mesh: &CpuMeshBuffers,
        surface_class: &SurfaceClassKey,
    ) -> Result<PackedMeshRegions, &'static str> {
        if mesh.vertex_count() != surface_class.vertex_count as usize {
            return Err("vertex_count");
        }
        if mesh.index_count() != surface_class.index_count as usize {
            return Err("index_count");
        }
        if surface_class.vertex_stride < DEFAULT_RENDER_VERTEX_STRIDE {
            return Err("vertex_stride");
        }
        if surface_class.attribute_stride < PACKED_NORMAL_BYTES + PACKED_UV_BYTES {
            return Err("attribute_stride");
        }
        if surface_class.index_stride < DEFAULT_RENDER_INDEX_STRIDE {
            return Err("index_stride");
        }

        let mut packed = PackedMeshRegions {
            vertex_region: vec![0; surface_class.vertex_bytes],
            attribute_region: vec![0; surface_class.attribute_bytes],
            index_region: vec![0; surface_class.index_bytes],
            vertex_stride: surface_class.vertex_stride,
            attribute_stride: surface_class.attribute_stride,
            index_stride: surface_class.index_stride,
        };

        for (index, position) in mesh.positions.iter().enumerate() {
            let offset = index * surface_class.vertex_stride;
            write_f32x3(
                &mut packed.vertex_region[offset..offset + DEFAULT_RENDER_VERTEX_STRIDE],
                *position,
            );
        }

        for index in 0..mesh.vertex_count() {
            let offset = index * surface_class.attribute_stride;
            write_f32x3(
                &mut packed.attribute_region[offset..offset + PACKED_NORMAL_BYTES],
                mesh.normals[index],
            );
            write_f32x2(
                &mut packed.attribute_region
                    [offset + PACKED_NORMAL_BYTES..offset + PACKED_NORMAL_BYTES + PACKED_UV_BYTES],
                mesh.uvs[index],
            );
            if surface_class.attribute_stride >= PACKED_COLOR_OFFSET + PACKED_COLOR_BYTES {
                write_rgba8(
                    &mut packed.attribute_region[offset + PACKED_COLOR_OFFSET
                        ..offset + PACKED_COLOR_OFFSET + PACKED_COLOR_BYTES],
                    mesh.colors[index],
                );
            }
        }

        for (index, triangle_index) in mesh.indices.iter().copied().enumerate() {
            let offset = index * surface_class.index_stride;
            packed.index_region[offset..offset + DEFAULT_RENDER_INDEX_STRIDE]
                .copy_from_slice(&triangle_index.to_le_bytes());
        }

        Ok(packed)
    }

    fn select_render_set(
        &mut self,
        camera: &CameraState,
        frame_state: &mut SelectionFrameState,
    ) -> Result<HashSet<ChunkKey>, TopologyError> {
        let mut selected = HashSet::new();

        for face in Face::ALL {
            self.select_render_chunk(
                ChunkKey::new(face, 0, 0, 0),
                camera,
                &mut selected,
                frame_state,
            )?;
        }

        frame_state.neighbor_split_count = self.normalize_neighbor_lod_delta(&mut selected)?;
        frame_state.selected_leaf_count = selected.len();

        Ok(selected)
    }

    fn select_render_chunk(
        &mut self,
        key: ChunkKey,
        camera: &CameraState,
        selected: &mut HashSet<ChunkKey>,
        frame_state: &mut SelectionFrameState,
    ) -> Result<(), TopologyError> {
        let meta = self.ensure_chunk_meta(key)?.clone();

        if !self.horizon_visible(camera, &meta) {
            return Ok(());
        }
        frame_state.horizon_survivor_count += 1;

        if !self.frustum_visible(camera, &meta) {
            return Ok(());
        }
        frame_state.frustum_survivor_count += 1;

        let error_px = self.projected_error_px(camera, &meta);
        let should_split = key.lod < self.config.max_lod && self.should_split_chunk(key, error_px);

        if should_split {
            for child in key
                .children()
                .expect("child keys must exist while below configured max lod")
            {
                self.select_render_chunk(child, camera, selected, frame_state)?;
            }
        } else {
            selected.insert(key);
        }

        Ok(())
    }

    fn should_split_chunk(&self, key: ChunkKey, error_px: f32) -> bool {
        let is_currently_split = self
            .active_render
            .iter()
            .any(|active| active != &key && active.is_descendant_of(&key));

        if is_currently_split {
            error_px >= self.config.merge_threshold_px
        } else {
            error_px > self.config.split_threshold_px
        }
    }

    fn normalize_neighbor_lod_delta(
        &mut self,
        selected: &mut HashSet<ChunkKey>,
    ) -> Result<usize, TopologyError> {
        let mut splits_applied = 0usize;

        loop {
            let mut split_targets = HashSet::new();

            for key in selected.iter().copied().collect::<Vec<_>>() {
                for edge in Edge::ALL {
                    let neighbor_same_lod = topology::same_lod_neighbor(key, edge)?;

                    if let Some(active_ancestor) =
                        Self::find_active_ancestor_covering(neighbor_same_lod, selected)
                    {
                        if key.lod > active_ancestor.lod + 1 {
                            split_targets.insert(active_ancestor);
                        }
                    }
                }
            }

            if split_targets.is_empty() {
                break;
            }

            for coarse_key in split_targets {
                if !selected.remove(&coarse_key) {
                    continue;
                }

                for child in coarse_key
                    .children()
                    .expect("normalization only splits non-leaf chunks")
                {
                    self.ensure_chunk_meta(child)?;
                    selected.insert(child);
                }

                splits_applied += 1;
            }
        }

        Ok(splits_applied)
    }

    fn find_active_ancestor_covering(
        mut key: ChunkKey,
        active_set: &HashSet<ChunkKey>,
    ) -> Option<ChunkKey> {
        loop {
            if active_set.contains(&key) {
                return Some(key);
            }

            key = key.parent()?;
        }
    }

    fn select_physics_set(
        &mut self,
        camera: &CameraState,
        desired_render: &HashSet<ChunkKey>,
    ) -> Result<HashSet<ChunkKey>, TopologyError> {
        let render_keys = desired_render.iter().copied().collect::<Vec<_>>();
        let mut candidates = Vec::with_capacity(render_keys.len());

        for key in render_keys {
            let meta = self.ensure_chunk_meta(key)?.clone();
            let distance = self.chunk_camera_distance(camera, &meta);
            candidates.push((key, distance));
        }
        candidates.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));

        let mut physics = HashSet::new();
        for (key, distance) in candidates {
            if distance <= self.config.physics_activation_radius {
                physics.insert(key);
            }
        }

        Ok(physics)
    }

    fn terrain_settings(&self) -> TerrainFieldSettings {
        TerrainFieldSettings {
            planet_radius: self.config.planet_radius,
            height_amplitude: self.config.height_amplitude,
            ..TerrainFieldSettings::default()
        }
    }

    fn required_surface_class_for_selection(
        &mut self,
        key: ChunkKey,
        desired_render: &HashSet<ChunkKey>,
    ) -> Result<SurfaceClassKey, TopologyError> {
        let meta = self.ensure_chunk_meta(key)?.clone();
        let mut neighbor_lods = [key.lod; Edge::ALL.len()];

        for (index, edge) in Edge::ALL.into_iter().enumerate() {
            let neighbor_same_lod = meta.neighbors.get(edge);
            if let Some(covering_key) =
                Self::find_active_ancestor_covering(neighbor_same_lod, desired_render)
            {
                neighbor_lods[index] = covering_key.lod;
            }
        }

        let stitch_mask = mesh_topology::stitch_mask_from_neighbor_lods(key.lod, neighbor_lods)
            .expect("selection normalization must keep neighbor lod deltas within one");

        SurfaceClassKey::canonical_chunk(
            stitch_mask,
            self.config.render_material_class,
            self.config.render_format_mask,
            self.config.render_vertex_stride,
            self.config.render_attribute_stride,
            self.config.render_index_stride,
        )
        .map_err(|_| TopologyError::InvalidChunkKey)
    }

    fn ensure_render_payload_for_selection(
        &mut self,
        key: ChunkKey,
        desired_render: &HashSet<ChunkKey>,
        frame_state: &mut SelectionFrameState,
    ) -> Result<usize, TopologyError> {
        let surface_class = self.required_surface_class_for_selection(key, desired_render)?;
        let existing_matches = self
            .resident_payloads
            .get(&key)
            .map(|payload| {
                payload.surface_class == surface_class && payload.packed_regions.is_some()
            })
            .unwrap_or(false);
        if existing_matches {
            return Ok(self
                .resident_payloads
                .get(&key)
                .expect("existing payload just matched")
                .upload_bytes());
        }

        let samples = self.sample_chunk_scalar_field(key)?;
        frame_state.phase7_sampled_chunks += 1;

        let mesh = self
            .derive_cpu_mesh_buffers(&samples, surface_class.stitch_mask)
            .expect("normalized stitch masks must map to canonical topology");
        frame_state.phase7_meshed_chunks += 1;

        let packed_regions = self
            .pack_mesh_regions(&mesh, &surface_class)
            .expect("phase 7 packer must match configured surface strides");
        frame_state.phase7_packed_chunks += 1;

        let current_surface_class = self
            .rid_state
            .get(&key)
            .and_then(|state| state.render_resident.then(|| state.active_surface_class.clone()))
            .flatten();
        let warm_path =
            self.choose_render_warm_path(current_surface_class.as_ref(), &surface_class);
        frame_state.phase7_commit_payloads += 1;
        frame_state.phase7_lifecycle_commands += 1;

        let render_lifecycle = match &warm_path {
            RenderWarmPath::ReuseCurrentSurface => {
                frame_state.phase7_warm_current_reuse_hits += 1;
                RenderLifecycleCommand::WarmReuseCurrent
            }
            RenderWarmPath::ReusePooledSurface(_) => {
                frame_state.phase7_warm_pool_reuse_hits += 1;
                RenderLifecycleCommand::WarmReusePooled
            }
            RenderWarmPath::ColdPath(reason) => {
                frame_state.phase7_cold_fallbacks += 1;
                match reason {
                    RenderFallbackReason::MissingCurrentSurfaceClass => {
                        frame_state.phase8_fallback_missing_current_surface_class += 1;
                    }
                    RenderFallbackReason::IncompatibleCurrentSurfaceClass(_) => {
                        frame_state.phase8_fallback_incompatible_current_surface_class += 1;
                    }
                    RenderFallbackReason::NoCompatiblePooledSurface => {
                        frame_state.phase8_fallback_no_compatible_pooled_surface += 1;
                    }
                }
                RenderLifecycleCommand::ColdCreate(reason.clone())
            }
        };

        let upload_bytes = packed_regions.vertex_region.len()
            + packed_regions.attribute_region.len()
            + packed_regions.index_region.len();
        let mut staging = None;
        if self.config.enable_godot_staging {
            let mut staged =
                self.stage_payload_bytes(key, &surface_class, &packed_regions, &warm_path);
            staged
                .copy_from_regions(&packed_regions, &surface_class)
                .expect("staging capacity must match the selected surface class");
            frame_state.phase7_staged_chunks += 1;
            staging = Some(staged);
        }

        let payload = ChunkPayload {
            surface_class: surface_class.clone(),
            stitch_mask: surface_class.stitch_mask,
            sample_count: samples.len(),
            mesh,
            packed_regions: Some(packed_regions),
            gd_staging: staging,
            pooled_render_entry: match warm_path {
                RenderWarmPath::ReusePooledSurface(entry) => Some(entry),
                _ => None,
            },
            assets: Vec::new(),
            collider_vertices: None,
            collider_indices: None,
            render_transform: Transform3D::IDENTITY,
            render_lifecycle,
        };
        if let Some(previous_payload) = self.resident_payloads.insert(key, payload) {
            self.reclaim_payload_resources(previous_payload);
        }

        Ok(upload_bytes)
    }

    fn stage_payload_bytes(
        &mut self,
        key: ChunkKey,
        surface_class: &SurfaceClassKey,
        _packed_regions: &PackedMeshRegions,
        warm_path: &RenderWarmPath,
    ) -> GdPackedStaging {
        match warm_path {
            RenderWarmPath::ReuseCurrentSurface => self
                .ensure_rid_state(key)
                .gd_staging
                .take()
                .unwrap_or_else(|| GdPackedStaging::new_for_surface_class(surface_class)),
            RenderWarmPath::ReusePooledSurface(entry) => entry
                .gd_staging
                .clone()
                .unwrap_or_else(|| GdPackedStaging::new_for_surface_class(surface_class)),
            RenderWarmPath::ColdPath(_) => GdPackedStaging::new_for_surface_class(surface_class),
        }
    }

    fn ensure_collision_payload(&mut self, key: ChunkKey) {
        let Some(payload) = self.resident_payloads.get_mut(&key) else {
            return;
        };

        if payload.collider_vertices.is_none() {
            payload.collider_vertices = Some(payload.mesh.positions.clone());
            payload.collider_indices = Some(payload.mesh.indices.clone());
        }
    }

    fn apply_budgeted_diffs(
        &mut self,
        desired_render: &HashSet<ChunkKey>,
        desired_physics: &HashSet<ChunkKey>,
        camera: &CameraState,
        frame_state: &mut SelectionFrameState,
    ) -> Result<(), TopologyError> {
        let mut ops =
            self.build_commit_ops(desired_render, desired_physics, camera, frame_state)?;
        ops.sort_by(|a, b| {
            a.priority_group
                .cmp(&b.priority_group)
                .then(a.distance_key_mm.cmp(&b.distance_key_mm))
                .then(a.key.cmp(&b.key))
        });

        frame_state.queued_commit_ops = ops.len();

        let mut committed = 0usize;
        let mut upload_bytes_committed = 0usize;
        let mut deferred_upload_bytes = 0usize;
        let mut deferred_now = HashSet::new();

        for op in ops {
            let over_commit_budget = committed >= self.config.commit_budget_per_frame;
            let over_upload_budget = op.upload_bytes > 0
                && upload_bytes_committed + op.upload_bytes
                    > self.config.upload_budget_bytes_per_frame;

            if over_commit_budget || over_upload_budget {
                deferred_upload_bytes += op.upload_bytes;
                deferred_now.insert(DeferredOpKey::new(op.kind, op.key));
                continue;
            }

            self.apply_commit_op(op, frame_state);
            committed += 1;
            upload_bytes_committed += op.upload_bytes;
        }

        for key in &deferred_now {
            let starvation = self.deferred_starvation.entry(*key).or_insert(0);
            *starvation = starvation.saturating_add(1);
        }
        self.deferred_starvation
            .retain(|key, _| deferred_now.contains(key));

        frame_state.committed_commit_ops = committed;
        frame_state.deferred_commit_ops = deferred_now.len();
        frame_state.upload_bytes_committed = upload_bytes_committed;
        frame_state.upload_bytes_deferred = deferred_upload_bytes;
        frame_state.max_deferred_starvation_frames = self
            .deferred_starvation
            .values()
            .copied()
            .max()
            .unwrap_or(0);
        frame_state.render_pool_entries = self.render_pool_entry_count();
        frame_state.physics_pool_entries = self.physics_pool.len();

        Ok(())
    }

    fn build_commit_ops(
        &mut self,
        desired_render: &HashSet<ChunkKey>,
        desired_physics: &HashSet<ChunkKey>,
        camera: &CameraState,
        frame_state: &mut SelectionFrameState,
    ) -> Result<Vec<CommitOp>, TopologyError> {
        let mut ops = Vec::new();

        let render_activations = desired_render
            .difference(&self.active_render)
            .copied()
            .collect::<Vec<_>>();
        for key in render_activations {
            let meta = self.ensure_chunk_meta(key)?.clone();
            let upload_bytes =
                self.ensure_render_payload_for_selection(key, desired_render, frame_state)?;
            ops.push(CommitOp {
                kind: CommitOpKind::ActivateRender,
                key,
                upload_bytes,
                priority_group: 0,
                distance_key_mm: distance_sort_key(self.chunk_camera_distance(camera, &meta)),
            });
        }

        let render_updates = desired_render
            .intersection(&self.active_render)
            .copied()
            .collect::<Vec<_>>();
        for key in render_updates {
            let required_surface_class =
                self.required_surface_class_for_selection(key, desired_render)?;
            let needs_refresh = self
                .resident_payloads
                .get(&key)
                .map(|payload| payload.surface_class != required_surface_class)
                .unwrap_or(true);
            if !needs_refresh {
                continue;
            }

            let meta = self.ensure_chunk_meta(key)?.clone();
            let upload_bytes =
                self.ensure_render_payload_for_selection(key, desired_render, frame_state)?;
            ops.push(CommitOp {
                kind: CommitOpKind::UpdateRender,
                key,
                upload_bytes,
                priority_group: 1,
                distance_key_mm: distance_sort_key(self.chunk_camera_distance(camera, &meta)),
            });
        }

        let physics_activations = desired_physics
            .difference(&self.active_physics)
            .copied()
            .collect::<Vec<_>>();
        for key in physics_activations {
            let meta = self.ensure_chunk_meta(key)?.clone();
            let distance = self.chunk_camera_distance(camera, &meta);
            self.ensure_collision_payload(key);
            ops.push(CommitOp {
                kind: CommitOpKind::ActivatePhysics,
                key,
                upload_bytes: 0,
                priority_group: 2,
                distance_key_mm: distance_sort_key(distance),
            });
        }

        let render_deactivations = self
            .active_render
            .difference(desired_render)
            .copied()
            .collect::<Vec<_>>();
        for key in render_deactivations {
            let meta = self.ensure_chunk_meta(key)?.clone();
            let distance = self.chunk_camera_distance(camera, &meta);
            ops.push(CommitOp {
                kind: CommitOpKind::DeactivateRender,
                key,
                upload_bytes: 0,
                priority_group: 3,
                distance_key_mm: distance_sort_key(distance),
            });
        }

        let physics_deactivations = self
            .active_physics
            .difference(desired_physics)
            .copied()
            .collect::<Vec<_>>();
        for key in physics_deactivations {
            let meta = self.ensure_chunk_meta(key)?.clone();
            let distance = self.chunk_camera_distance(camera, &meta);
            ops.push(CommitOp {
                kind: CommitOpKind::DeactivatePhysics,
                key,
                upload_bytes: 0,
                priority_group: 4,
                distance_key_mm: distance_sort_key(distance),
            });
        }

        Ok(ops)
    }

    fn apply_commit_op(&mut self, op: CommitOp, frame_state: &mut SelectionFrameState) {
        match op.kind {
            CommitOpKind::ActivateRender => {
                self.commit_render_payload(op.key, frame_state);
                self.active_render.insert(op.key);
                self.ensure_rid_state(op.key).render_resident = true;
            }
            CommitOpKind::UpdateRender => {
                self.commit_render_payload(op.key, frame_state);
                self.active_render.insert(op.key);
                self.ensure_rid_state(op.key).render_resident = true;
            }
            CommitOpKind::DeactivateRender => {
                self.deactivate_render_commit(op.key);
                self.active_render.remove(&op.key);
                self.ensure_rid_state(op.key).render_resident = false;
            }
            CommitOpKind::ActivatePhysics => {
                self.commit_physics_payload(op.key, frame_state);
                self.active_physics.insert(op.key);
                self.ensure_rid_state(op.key).physics_resident = true;
            }
            CommitOpKind::DeactivatePhysics => {
                self.deactivate_physics_commit(op.key);
                self.active_physics.remove(&op.key);
                self.ensure_rid_state(op.key).physics_resident = false;
            }
        }
    }

    fn should_commit_to_servers(&self) -> bool {
        self.has_valid_world_rids()
    }

    fn render_pool_entry_count(&self) -> usize {
        self.render_pool.values().map(VecDeque::len).sum()
    }

    fn commit_render_payload(&mut self, key: ChunkKey, frame_state: &mut SelectionFrameState) {
        let Some(payload) = self.resident_payloads.get_mut(&key) else {
            return;
        };

        let surface_class = payload.surface_class.clone();
        let render_transform = payload.render_transform;
        let render_lifecycle = payload.render_lifecycle.clone();
        let mesh = payload.mesh.clone();
        let packed_regions = payload.packed_regions.clone();
        let mut gd_staging = payload.gd_staging.take();
        let pooled_render_entry = payload.pooled_render_entry.take();

        if self.should_commit_to_servers() {
            let mut rendering_server = RenderingServer::singleton();
            match render_lifecycle {
                RenderLifecycleCommand::WarmReuseCurrent => {
                    let Some((mesh_rid, render_instance_rid)) = self.current_render_rids(key) else {
                        return;
                    };
                    let Some(staging) =
                        self.ensure_commit_staging(gd_staging.take(), packed_regions.as_ref(), &surface_class)
                    else {
                        return;
                    };

                    rendering_server.mesh_surface_update_vertex_region(
                        mesh_rid,
                        0,
                        0,
                        &staging.vertex_region,
                    );
                    rendering_server.mesh_surface_update_attribute_region(
                        mesh_rid,
                        0,
                        0,
                        &staging.attribute_region,
                    );
                    rendering_server.mesh_surface_update_index_region(
                        mesh_rid,
                        0,
                        0,
                        &staging.index_region,
                    );
                    rendering_server.instance_set_base(render_instance_rid, mesh_rid);
                    rendering_server.instance_set_scenario(render_instance_rid, self.scenario_rid);
                    rendering_server.instance_set_transform(render_instance_rid, render_transform);
                    rendering_server.instance_set_visible(render_instance_rid, true);

                    gd_staging = Some(staging);
                    frame_state.phase8_render_warm_current_commits += 1;
                }
                RenderLifecycleCommand::WarmReusePooled => {
                    let Some(entry) = pooled_render_entry else {
                        return;
                    };
                    if let Some(previous_entry) = self.take_current_render_entry(key) {
                        self.recycle_render_entry(previous_entry);
                    }

                    let Some(staging) =
                        self.ensure_commit_staging(gd_staging.take().or(entry.gd_staging), packed_regions.as_ref(), &surface_class)
                    else {
                        return;
                    };

                    rendering_server.mesh_surface_update_vertex_region(
                        entry.mesh_rid,
                        0,
                        0,
                        &staging.vertex_region,
                    );
                    rendering_server.mesh_surface_update_attribute_region(
                        entry.mesh_rid,
                        0,
                        0,
                        &staging.attribute_region,
                    );
                    rendering_server.mesh_surface_update_index_region(
                        entry.mesh_rid,
                        0,
                        0,
                        &staging.index_region,
                    );
                    rendering_server.instance_set_base(entry.render_instance_rid, entry.mesh_rid);
                    rendering_server.instance_set_scenario(entry.render_instance_rid, self.scenario_rid);
                    rendering_server.instance_set_transform(entry.render_instance_rid, render_transform);
                    rendering_server.instance_set_visible(entry.render_instance_rid, true);

                    self.install_render_entry(
                        key,
                        entry.mesh_rid,
                        entry.render_instance_rid,
                        surface_class.clone(),
                        Some(staging.clone()),
                    );
                    gd_staging = Some(staging);
                    frame_state.phase8_render_warm_pool_commits += 1;
                }
                RenderLifecycleCommand::ColdCreate(_) => {
                    if let Some(previous_entry) = self.take_current_render_entry(key) {
                        self.recycle_render_entry(previous_entry);
                    }

                    let mesh_rid = rendering_server.mesh_create();
                    let arrays = cpu_mesh_to_surface_arrays(&mesh);
                    rendering_server.mesh_add_surface_from_arrays(
                        mesh_rid,
                        PrimitiveType::TRIANGLES,
                        &arrays,
                    );
                    let render_instance_rid = rendering_server.instance_create();
                    rendering_server.instance_set_base(render_instance_rid, mesh_rid);
                    rendering_server.instance_set_scenario(render_instance_rid, self.scenario_rid);
                    rendering_server.instance_set_transform(render_instance_rid, render_transform);
                    rendering_server.instance_set_visible(render_instance_rid, true);

                    let staging =
                        self.ensure_commit_staging(gd_staging.take(), packed_regions.as_ref(), &surface_class);
                    self.install_render_entry(
                        key,
                        mesh_rid,
                        render_instance_rid,
                        surface_class.clone(),
                        staging.clone(),
                    );
                    gd_staging = staging;
                    frame_state.phase8_render_cold_commits += 1;
                }
            }
        } else {
            match render_lifecycle {
                RenderLifecycleCommand::WarmReuseCurrent => {
                    frame_state.phase8_render_warm_current_commits += 1;
                }
                RenderLifecycleCommand::WarmReusePooled => {
                    if let Some(previous_entry) = self.take_current_render_entry(key) {
                        self.recycle_render_entry(previous_entry);
                    }
                    if let Some(entry) = pooled_render_entry {
                        self.install_render_entry(
                            key,
                            entry.mesh_rid,
                            entry.render_instance_rid,
                            surface_class.clone(),
                            gd_staging.take().or(entry.gd_staging),
                        );
                    }
                    frame_state.phase8_render_warm_pool_commits += 1;
                }
                RenderLifecycleCommand::ColdCreate(_) => {
                    if let Some(previous_entry) = self.take_current_render_entry(key) {
                        self.recycle_render_entry(previous_entry);
                    }
                    frame_state.phase8_render_cold_commits += 1;
                }
            }
        }

        let rid_state = self.ensure_rid_state(key);
        rid_state.render_resident = true;
        rid_state.active_surface_class = Some(surface_class.clone());
        rid_state.pooled_surface_class = Some(surface_class);
        if rid_state.gd_staging.is_none() {
            rid_state.gd_staging = gd_staging;
        }
    }

    fn commit_physics_payload(&mut self, key: ChunkKey, frame_state: &mut SelectionFrameState) {
        self.ensure_collision_payload(key);

        let Some(payload) = self.resident_payloads.get(&key) else {
            return;
        };
        let Some(collider_vertices) = payload.collider_vertices.clone() else {
            return;
        };
        let Some(collider_indices) = payload.collider_indices.clone() else {
            return;
        };
        let render_transform = payload.render_transform;

        let pooled_entry = self.pop_physics_pool_entry();
        let (physics_body_rid, physics_shape_rid) = match pooled_entry {
            Some(entry) => (entry.physics_body_rid, entry.physics_shape_rid),
            None => {
                if self.should_commit_to_servers() {
                    let mut physics_server = PhysicsServer3D::singleton();
                    let body_rid = physics_server.body_create();
                    physics_server.body_set_mode(body_rid, BodyMode::STATIC);
                    let shape_rid = physics_server.concave_polygon_shape_create();
                    (body_rid, shape_rid)
                } else {
                    (Rid::Invalid, Rid::Invalid)
                }
            }
        };

        if self.should_commit_to_servers() {
            let collider_faces = collider_faces_from_indices(&collider_vertices, &collider_indices);
            let mut shape_data = Dictionary::<StringName, Variant>::new();
            shape_data.set("faces", &collider_faces.to_variant());
            shape_data.set("backface_collision", false);
            let mut physics_server = PhysicsServer3D::singleton();
            physics_server.shape_set_data(physics_shape_rid, &shape_data.to_variant());
            physics_server.body_clear_shapes(physics_body_rid);
            physics_server.body_add_shape(physics_body_rid, physics_shape_rid);
            physics_server.body_set_state(
                physics_body_rid,
                BodyState::TRANSFORM,
                &render_transform.to_variant(),
            );
            physics_server.body_set_space(physics_body_rid, self.physics_space_rid);
        }

        let rid_state = self.ensure_rid_state(key);
        rid_state.physics_body_rid = Some(physics_body_rid);
        rid_state.physics_shape_rid = Some(physics_shape_rid);
        rid_state.physics_resident = true;
        frame_state.phase8_physics_commits += 1;
    }

    fn deactivate_render_commit(&mut self, key: ChunkKey) {
        if let Some(entry) = self.take_current_render_entry(key) {
            self.recycle_render_entry(entry);
        }

        let rid_state = self.ensure_rid_state(key);
        rid_state.render_resident = false;
    }

    fn deactivate_physics_commit(&mut self, key: ChunkKey) {
        if let Some(entry) = self.take_current_physics_entry(key) {
            self.recycle_physics_entry(entry);
        }

        let rid_state = self.ensure_rid_state(key);
        rid_state.physics_resident = false;
    }

    fn current_render_rids(&mut self, key: ChunkKey) -> Option<(Rid, Rid)> {
        let rid_state = self.ensure_rid_state(key);
        match (rid_state.mesh_rid, rid_state.render_instance_rid) {
            (Some(mesh_rid), Some(render_instance_rid)) => Some((mesh_rid, render_instance_rid)),
            _ => None,
        }
    }

    fn take_current_render_entry(&mut self, key: ChunkKey) -> Option<RenderPoolEntry> {
        let rid_state = self.ensure_rid_state(key);
        let (Some(mesh_rid), Some(render_instance_rid), Some(surface_class)) = (
            rid_state.mesh_rid,
            rid_state.render_instance_rid,
            rid_state.active_surface_class.clone(),
        ) else {
            return None;
        };

        let gd_staging = rid_state.gd_staging.take();
        rid_state.mesh_rid = None;
        rid_state.render_instance_rid = None;
        rid_state.active_surface_class = None;
        rid_state.pooled_surface_class = None;
        rid_state.render_resident = false;

        Some(RenderPoolEntry {
            mesh_rid,
            render_instance_rid,
            surface_class,
            gd_staging,
        })
    }

    fn install_render_entry(
        &mut self,
        key: ChunkKey,
        mesh_rid: Rid,
        render_instance_rid: Rid,
        surface_class: SurfaceClassKey,
        gd_staging: Option<GdPackedStaging>,
    ) {
        let rid_state = self.ensure_rid_state(key);
        rid_state.mesh_rid = Some(mesh_rid);
        rid_state.render_instance_rid = Some(render_instance_rid);
        rid_state.active_surface_class = Some(surface_class.clone());
        rid_state.pooled_surface_class = Some(surface_class);
        rid_state.gd_staging = gd_staging;
        rid_state.render_resident = true;
    }

    fn recycle_render_entry(&mut self, entry: RenderPoolEntry) {
        if self.should_commit_to_servers() {
            let mut rendering_server = RenderingServer::singleton();
            rendering_server.instance_set_visible(entry.render_instance_rid, false);
        }

        let entries = self
            .render_pool
            .entry(entry.surface_class.clone())
            .or_default();
        if entries.len() < self.config.render_pool_watermark_per_class {
            entries.push_back(entry);
        } else {
            self.free_render_entry(entry);
        }
    }

    fn free_render_entry(&mut self, entry: RenderPoolEntry) {
        if self.should_commit_to_servers() {
            let mut rendering_server = RenderingServer::singleton();
            rendering_server.free_rid(entry.render_instance_rid);
            rendering_server.free_rid(entry.mesh_rid);
        }
    }

    fn take_current_physics_entry(&mut self, key: ChunkKey) -> Option<PhysicsPoolEntry> {
        let rid_state = self.ensure_rid_state(key);
        let (Some(physics_body_rid), Some(physics_shape_rid)) =
            (rid_state.physics_body_rid, rid_state.physics_shape_rid)
        else {
            return None;
        };

        rid_state.physics_body_rid = None;
        rid_state.physics_shape_rid = None;
        rid_state.physics_resident = false;

        Some(PhysicsPoolEntry {
            physics_body_rid,
            physics_shape_rid,
        })
    }

    fn recycle_physics_entry(&mut self, entry: PhysicsPoolEntry) {
        if self.should_commit_to_servers() {
            let mut physics_server = PhysicsServer3D::singleton();
            physics_server.body_set_space(entry.physics_body_rid, Rid::Invalid);
            physics_server.body_clear_shapes(entry.physics_body_rid);
        }

        if self.physics_pool.len() < self.config.physics_pool_watermark {
            self.physics_pool.push_back(entry);
        } else {
            self.free_physics_entry(entry);
        }
    }

    fn free_physics_entry(&mut self, entry: PhysicsPoolEntry) {
        if self.should_commit_to_servers() {
            let mut physics_server = PhysicsServer3D::singleton();
            physics_server.free_rid(entry.physics_body_rid);
            physics_server.free_rid(entry.physics_shape_rid);
        }
    }

    fn ensure_commit_staging(
        &self,
        staging: Option<GdPackedStaging>,
        packed_regions: Option<&PackedMeshRegions>,
        surface_class: &SurfaceClassKey,
    ) -> Option<GdPackedStaging> {
        let mut staging = staging.unwrap_or_else(|| GdPackedStaging::new_for_surface_class(surface_class));
        if let Some(packed_regions) = packed_regions {
            staging
                .copy_from_regions(packed_regions, surface_class)
                .ok()?;
        }
        Some(staging)
    }

    fn reclaim_payload_resources(&mut self, mut payload: ChunkPayload) {
        if let Some(entry) = payload.pooled_render_entry.take() {
            self.recycle_render_entry(entry);
        }
    }

    fn horizon_visible(&self, camera: &CameraState, meta: &ChunkMeta) -> bool {
        let camera_distance = camera.position_planet.length();
        let occluder_radius = self.config.planet_radius + self.config.horizon_safety_margin;

        if camera_distance <= occluder_radius {
            return true;
        }

        let beta = (occluder_radius / camera_distance).clamp(-1.0, 1.0).acos();
        let theta = camera
            .position_planet
            .normalize_or_zero()
            .angle_between(meta.bounds.center_planet.normalize_or_zero());

        theta <= beta + f64::from(meta.metrics.angular_radius)
    }

    fn frustum_visible(&self, camera: &CameraState, meta: &ChunkMeta) -> bool {
        let center = dvec3_to_vector3(meta.bounds.center_planet);
        let radius = meta.bounds.radius as f32;

        camera
            .frustum_planes
            .iter()
            .all(|plane| plane.distance_to(center) <= radius)
    }

    fn projected_error_px(&self, camera: &CameraState, meta: &ChunkMeta) -> f32 {
        let distance = self
            .chunk_camera_distance(camera, meta)
            .max(f64::from(f32::EPSILON));

        (f64::from(meta.metrics.geometric_error) * camera.projection_scale / distance) as f32
    }

    fn chunk_camera_distance(&self, camera: &CameraState, meta: &ChunkMeta) -> f64 {
        (meta.bounds.center_planet - camera.position_planet).length()
    }

    pub fn register_chunk_meta(
        &mut self,
        mut meta: ChunkMeta,
    ) -> Result<Option<ChunkMeta>, TopologyError> {
        meta.refresh_same_lod_neighbors()?;
        Ok(self.meta.insert(meta.key, meta))
    }

    pub fn insert_payload(&mut self, key: ChunkKey, payload: ChunkPayload) -> Option<ChunkPayload> {
        let previous = self.resident_payloads.insert(key, payload);
        if let Some(payload) = previous.as_ref().cloned() {
            self.reclaim_payload_resources(payload);
        }
        previous
    }

    pub fn remove_payload(&mut self, key: &ChunkKey) -> Option<ChunkPayload> {
        let removed = self.resident_payloads.remove(key);
        if let Some(payload) = removed.as_ref().cloned() {
            self.reclaim_payload_resources(payload);
        }
        removed
    }

    pub fn enforce_payload_residency_budget(
        &mut self,
        retained_keys: &HashSet<ChunkKey>,
        max_payloads: usize,
    ) -> Vec<ChunkKey> {
        if self.resident_payloads.len() <= max_payloads {
            return Vec::new();
        }

        let mut eviction_candidates = self
            .resident_payloads
            .keys()
            .copied()
            .filter(|key| !retained_keys.contains(key))
            .collect::<Vec<_>>();
        eviction_candidates.sort_unstable();

        let mut evicted = Vec::new();
        let mut payload_count = self.resident_payloads.len();

        for key in eviction_candidates {
            if payload_count <= max_payloads {
                break;
            }

            if let Some(payload) = self.resident_payloads.remove(&key) {
                self.reclaim_payload_resources(payload);
                evicted.push(key);
                payload_count -= 1;
            }
        }

        evicted
    }

    pub fn ensure_rid_state(&mut self, key: ChunkKey) -> &mut ChunkRidState {
        self.rid_state.entry(key).or_default()
    }

    pub fn activate_render(&mut self, key: ChunkKey) -> bool {
        self.active_render.insert(key)
    }

    pub fn deactivate_render(&mut self, key: &ChunkKey) -> bool {
        self.active_render.remove(key)
    }

    pub fn activate_physics(&mut self, key: ChunkKey) -> bool {
        self.active_physics.insert(key)
    }

    pub fn deactivate_physics(&mut self, key: &ChunkKey) -> bool {
        self.active_physics.remove(key)
    }

    pub fn push_render_pool_entry(&mut self, entry: RenderPoolEntry) {
        self.render_pool
            .entry(entry.surface_class.clone())
            .or_default()
            .push_back(entry);
    }

    pub fn pop_render_pool_entry(
        &mut self,
        surface_class: &SurfaceClassKey,
    ) -> Option<RenderPoolEntry> {
        let entries = self.render_pool.get_mut(surface_class)?;
        let entry = entries.pop_front();

        if entries.is_empty() {
            self.render_pool.remove(surface_class);
        }

        entry
    }

    pub fn choose_render_warm_path(
        &mut self,
        current_surface_class: Option<&SurfaceClassKey>,
        required_surface_class: &SurfaceClassKey,
    ) -> RenderWarmPath {
        if let Some(current_surface_class) = current_surface_class {
            let issues = current_surface_class.compatibility_issues(required_surface_class);
            if issues.is_empty() {
                return RenderWarmPath::ReuseCurrentSurface;
            }

            if let Some(entry) = self.pop_render_pool_entry(required_surface_class) {
                return RenderWarmPath::ReusePooledSurface(entry);
            }

            return RenderWarmPath::ColdPath(
                RenderFallbackReason::IncompatibleCurrentSurfaceClass(issues),
            );
        }

        if let Some(entry) = self.pop_render_pool_entry(required_surface_class) {
            return RenderWarmPath::ReusePooledSurface(entry);
        }

        RenderWarmPath::ColdPath(RenderFallbackReason::MissingCurrentSurfaceClass)
    }

    pub fn push_physics_pool_entry(&mut self, entry: PhysicsPoolEntry) {
        self.physics_pool.push_back(entry);
    }

    pub fn pop_physics_pool_entry(&mut self) -> Option<PhysicsPoolEntry> {
        self.physics_pool.pop_front()
    }

    pub fn release_server_resources(&mut self) {
        if self.should_commit_to_servers() {
            let mut rendering_server = RenderingServer::singleton();
            let mut physics_server = PhysicsServer3D::singleton();

            for payload in self.resident_payloads.values_mut() {
                if let Some(entry) = payload.pooled_render_entry.take() {
                    rendering_server.free_rid(entry.render_instance_rid);
                    rendering_server.free_rid(entry.mesh_rid);
                }
            }

            for rid_state in self.rid_state.values_mut() {
                if let Some(render_instance_rid) = rid_state.render_instance_rid.take() {
                    rendering_server.instance_set_visible(render_instance_rid, false);
                    rendering_server.free_rid(render_instance_rid);
                }
                if let Some(mesh_rid) = rid_state.mesh_rid.take() {
                    rendering_server.free_rid(mesh_rid);
                }
                if let Some(physics_body_rid) = rid_state.physics_body_rid.take() {
                    physics_server.body_set_space(physics_body_rid, Rid::Invalid);
                    physics_server.free_rid(physics_body_rid);
                }
                if let Some(physics_shape_rid) = rid_state.physics_shape_rid.take() {
                    physics_server.free_rid(physics_shape_rid);
                }

                rid_state.active_surface_class = None;
                rid_state.pooled_surface_class = None;
                rid_state.gd_staging = None;
                rid_state.render_resident = false;
                rid_state.physics_resident = false;
            }

            for (_, entries) in self.render_pool.drain() {
                for entry in entries {
                    rendering_server.free_rid(entry.render_instance_rid);
                    rendering_server.free_rid(entry.mesh_rid);
                }
            }
            for entry in self.physics_pool.drain(..) {
                physics_server.body_set_space(entry.physics_body_rid, Rid::Invalid);
                physics_server.free_rid(entry.physics_body_rid);
                physics_server.free_rid(entry.physics_shape_rid);
            }
        } else {
            for payload in self.resident_payloads.values_mut() {
                payload.pooled_render_entry = None;
            }
            for rid_state in self.rid_state.values_mut() {
                rid_state.mesh_rid = None;
                rid_state.render_instance_rid = None;
                rid_state.physics_body_rid = None;
                rid_state.physics_shape_rid = None;
                rid_state.active_surface_class = None;
                rid_state.pooled_surface_class = None;
                rid_state.gd_staging = None;
                rid_state.render_resident = false;
                rid_state.physics_resident = false;
            }
            self.render_pool.clear();
            self.physics_pool.clear();
        }

        self.active_render.clear();
        self.active_physics.clear();
    }

    pub fn meta_count(&self) -> usize {
        self.meta.len()
    }

    pub fn active_render_count(&self) -> usize {
        self.active_render.len()
    }

    pub fn active_physics_count(&self) -> usize {
        self.active_physics.len()
    }

    pub fn desired_render_count(&self) -> usize {
        self.frame_state.desired_render_count
    }

    pub fn desired_physics_count(&self) -> usize {
        self.frame_state.desired_physics_count
    }

    pub fn deferred_commit_count(&self) -> usize {
        self.frame_state.deferred_commit_ops
    }

    pub fn deferred_upload_bytes(&self) -> usize {
        self.frame_state.upload_bytes_deferred
    }

    pub fn resident_payload_count(&self) -> usize {
        self.resident_payloads.len()
    }

    pub fn rid_state_count(&self) -> usize {
        self.rid_state.len()
    }
}

fn dvec3_to_vector3(value: DVec3) -> Vector3 {
    Vector3::new(value.x as f32, value.y as f32, value.z as f32)
}

fn vector3_to_dvec3(value: Vector3) -> DVec3 {
    DVec3::new(f64::from(value.x), f64::from(value.y), f64::from(value.z))
}

fn dvec3_to_f32_array(value: DVec3) -> [f32; 3] {
    [value.x as f32, value.y as f32, value.z as f32]
}

fn normalize_to_cube_surface(cube_point: DVec3) -> DVec3 {
    let max_axis = cube_point.abs().max_element();
    if max_axis <= f64::EPSILON {
        DVec3::ZERO
    } else {
        cube_point / max_axis
    }
}

fn clamp_grid_index(value: i32, samples_per_edge: u32) -> i32 {
    value.clamp(0, samples_per_edge.saturating_sub(1) as i32)
}

fn write_f32x3(bytes: &mut [u8], value: [f32; 3]) {
    bytes[0..4].copy_from_slice(&value[0].to_le_bytes());
    bytes[4..8].copy_from_slice(&value[1].to_le_bytes());
    bytes[8..12].copy_from_slice(&value[2].to_le_bytes());
}

fn write_f32x2(bytes: &mut [u8], value: [f32; 2]) {
    bytes[0..4].copy_from_slice(&value[0].to_le_bytes());
    bytes[4..8].copy_from_slice(&value[1].to_le_bytes());
}

fn write_rgba8(bytes: &mut [u8], value: [f32; 4]) {
    for (index, channel) in value.into_iter().enumerate() {
        bytes[index] = (channel.clamp(0.0, 1.0) * 255.0).round() as u8;
    }
}

fn distance_sort_key(distance: f64) -> u64 {
    if !distance.is_finite() {
        u64::MAX
    } else {
        (distance.max(0.0) * 1_000.0) as u64
    }
}

fn cpu_mesh_to_surface_arrays(mesh: &CpuMeshBuffers) -> Array<Variant> {
    let vertices = PackedVector3Array::from_iter(
        mesh.positions
            .iter()
            .copied()
            .map(|position| Vector3::new(position[0], position[1], position[2])),
    );
    let normals = PackedVector3Array::from_iter(
        mesh.normals
            .iter()
            .copied()
            .map(|normal| Vector3::new(normal[0], normal[1], normal[2])),
    );
    let colors = PackedColorArray::from_iter(mesh.colors.iter().copied().map(|color| {
        Color::from_rgba(color[0], color[1], color[2], color[3])
    }));
    let uvs = PackedVector2Array::from_iter(
        mesh.uvs
            .iter()
            .copied()
            .map(|uv| Vector2::new(uv[0], uv[1])),
    );
    let indices = PackedInt32Array::from_iter(mesh.indices.iter().copied());

    Array::from_iter([
        vertices.to_variant(),
        normals.to_variant(),
        Variant::nil(),
        colors.to_variant(),
        uvs.to_variant(),
        Variant::nil(),
        Variant::nil(),
        Variant::nil(),
        Variant::nil(),
        Variant::nil(),
        Variant::nil(),
        Variant::nil(),
        indices.to_variant(),
    ])
}

fn collider_faces_from_indices(
    collider_vertices: &[[f32; 3]],
    collider_indices: &[i32],
) -> PackedVector3Array {
    PackedVector3Array::from_iter(collider_indices.iter().filter_map(|index| {
        let index = usize::try_from(*index).ok()?;
        let position = *collider_vertices.get(index)?;
        Some(Vector3::new(position[0], position[1], position[2]))
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_surface_class() -> SurfaceClassKey {
        SurfaceClassKey::canonical_chunk(0b0101, 3, DEFAULT_RENDER_FORMAT_MASK, 12, 24, 4).unwrap()
    }

    fn test_runtime() -> PlanetRuntime {
        PlanetRuntime::new(
            RuntimeConfig {
                metadata_precompute_max_lod: 0,
                enable_godot_staging: false,
                ..RuntimeConfig::default()
            },
            Rid::Invalid,
            Rid::Invalid,
        )
    }

    fn sample_key() -> ChunkKey {
        ChunkKey::new(Face::Px, 2, 1, 1)
    }

    fn sample_meta(key: ChunkKey, surface_class: SurfaceClassKey) -> ChunkMeta {
        ChunkMeta::new(
            key,
            ChunkBounds::new(DVec3::new(1.0, 2.0, 3.0), 50.0, -20.0, 40.0, 80.0, 140.0),
            ChunkMetrics::new(2.5, 42.0, 0.25),
            surface_class,
        )
        .unwrap()
    }

    fn sample_payload(surface_class: &SurfaceClassKey, fill: u8) -> ChunkPayload {
        ChunkPayload {
            mesh: CpuMeshBuffers {
                positions: vec![[0.0, 0.0, 0.0]; surface_class.vertex_count as usize],
                indices: vec![0; surface_class.index_count as usize],
                ..CpuMeshBuffers::default()
            },
            packed_regions: Some(PackedMeshRegions {
                vertex_region: vec![fill; surface_class.vertex_bytes],
                attribute_region: vec![fill; surface_class.attribute_bytes],
                index_region: vec![fill; surface_class.index_bytes],
                vertex_stride: 12,
                attribute_stride: 24,
                index_stride: 4,
            }),
            ..ChunkPayload::default()
        }
    }

    fn huge_test_frustum() -> [Plane; 6] {
        [
            Plane::from_point_normal(
                Vector3::new(0.0, 0.0, 20_000.0),
                Vector3::new(0.0, 0.0, 1.0),
            ),
            Plane::from_point_normal(
                Vector3::new(0.0, 0.0, -20_000.0),
                Vector3::new(0.0, 0.0, -1.0),
            ),
            Plane::from_point_normal(
                Vector3::new(-20_000.0, 0.0, 0.0),
                Vector3::new(-1.0, 0.0, 0.0),
            ),
            Plane::from_point_normal(
                Vector3::new(0.0, 20_000.0, 0.0),
                Vector3::new(0.0, 1.0, 0.0),
            ),
            Plane::from_point_normal(
                Vector3::new(20_000.0, 0.0, 0.0),
                Vector3::new(1.0, 0.0, 0.0),
            ),
            Plane::from_point_normal(
                Vector3::new(0.0, -20_000.0, 0.0),
                Vector3::new(0.0, -1.0, 0.0),
            ),
        ]
    }

    fn orbit_camera_state() -> CameraState {
        CameraState {
            position_planet: DVec3::new(0.0, 0.0, 3_000.0),
            forward_planet: DVec3::new(0.0, 0.0, -1.0),
            frustum_planes: huge_test_frustum(),
            projection_scale: 1_200.0,
        }
    }

    #[test]
    fn chunk_key_validates_coords_against_lod_resolution() {
        assert!(ChunkKey::new(Face::Px, 3, 7, 7).is_valid_for_lod());
        assert!(!ChunkKey::new(Face::Px, 3, 8, 7).is_valid_for_lod());
        assert_eq!(ChunkKey::resolution_for_lod(5), 32);
    }

    #[test]
    fn surface_class_mismatch_detection_is_strict() {
        let base = sample_surface_class();
        let mismatched =
            SurfaceClassKey::canonical_chunk(0b0101, 3, DEFAULT_RENDER_FORMAT_MASK, 16, 24, 4)
                .unwrap();

        let issues = base.compatibility_issues(&mismatched);

        assert_eq!(issues, vec!["vertex_stride", "vertex_bytes"]);
        assert!(!base.is_pool_compatible_with(&mismatched));
        assert!(base.is_pool_compatible_with(&base));
    }

    #[test]
    fn packed_region_validation_uses_precomputed_byte_expectations() {
        let surface_class = sample_surface_class();
        let valid = PackedMeshRegions {
            vertex_region: vec![0; surface_class.vertex_bytes],
            attribute_region: vec![0; surface_class.attribute_bytes],
            index_region: vec![0; surface_class.index_bytes],
            vertex_stride: 12,
            attribute_stride: 24,
            index_stride: 4,
        };
        let invalid = PackedMeshRegions {
            vertex_region: vec![0; surface_class.vertex_bytes.saturating_sub(1)],
            ..valid.clone()
        };

        assert_eq!(valid.validate_for_surface_class(&surface_class), Ok(()));
        assert_eq!(
            invalid.validate_for_surface_class(&surface_class),
            Err("vertex_bytes")
        );

        let wrong_stride = PackedMeshRegions {
            vertex_stride: 16,
            ..valid.clone()
        };
        assert_eq!(
            wrong_stride.validate_for_surface_class(&surface_class),
            Err("vertex_stride")
        );
    }

    #[test]
    fn runtime_map_transitions_are_deterministic() {
        let key = sample_key();
        let surface_class = sample_surface_class();
        let meta = sample_meta(key, surface_class.clone());
        let payload = sample_payload(&surface_class, 1);

        let mut runtime = test_runtime();
        let initial_meta_count = runtime.meta_count();
        runtime.register_chunk_meta(meta).unwrap();
        runtime.activate_render(key);
        runtime.activate_physics(key);
        runtime.insert_payload(key, payload);
        runtime.ensure_rid_state(key).render_resident = true;
        runtime.ensure_rid_state(key).physics_resident = true;

        assert_eq!(runtime.meta_count(), initial_meta_count + 1);
        assert_eq!(runtime.active_render_count(), 1);
        assert_eq!(runtime.active_physics_count(), 1);
        assert_eq!(runtime.resident_payload_count(), 1);
        assert_eq!(runtime.rid_state_count(), 1);

        assert!(runtime.deactivate_render(&key));
        assert!(runtime.deactivate_physics(&key));
        assert!(runtime.remove_payload(&key).is_some());

        assert_eq!(runtime.active_render_count(), 0);
        assert_eq!(runtime.active_physics_count(), 0);
        assert_eq!(runtime.resident_payload_count(), 0);
        assert_eq!(runtime.rid_state_count(), 1);
    }

    #[test]
    fn payload_precompute_window_stays_bounded() {
        let runtime = PlanetRuntime::default();

        assert!(runtime.should_precompute_payload_for_lod(PAYLOAD_PRECOMPUTE_MAX_LOD));
        assert!(!runtime.should_precompute_payload_for_lod(PAYLOAD_PRECOMPUTE_MAX_LOD + 1));
    }

    #[test]
    fn metadata_precompute_window_is_explicit_in_runtime_config() {
        let runtime = PlanetRuntime::new(
            RuntimeConfig {
                metadata_precompute_max_lod: 2,
                enable_godot_staging: false,
                ..RuntimeConfig::default()
            },
            Rid::Invalid,
            Rid::Invalid,
        );

        assert_eq!(runtime.metadata_precompute_max_lod(), 2);
        assert_eq!(runtime.meta_count(), 6 * (1 + 4 + 16));
    }

    #[test]
    fn phase7_pipeline_builds_sample_mesh_and_packed_payloads() {
        let mut runtime = test_runtime();
        let key = ChunkKey::new(Face::Pz, 2, 1, 1);
        let desired_render = [key].into_iter().collect::<HashSet<_>>();
        let mut frame_state = SelectionFrameState::default();

        let upload_bytes = runtime
            .ensure_render_payload_for_selection(key, &desired_render, &mut frame_state)
            .unwrap();
        let payload = runtime.resident_payloads.get(&key).unwrap();

        assert_eq!(
            payload.sample_count as u32,
            mesh_topology::SAMPLED_VERTICES_PER_EDGE * mesh_topology::SAMPLED_VERTICES_PER_EDGE
        );
        assert_eq!(
            payload.mesh.vertex_count() as u32,
            mesh_topology::canonical_chunk_topology().vertex_count()
        );
        assert_eq!(
            payload.mesh.index_count(),
            payload.surface_class.index_count as usize
        );
        assert_eq!(payload.stitch_mask, mesh_topology::BASE_STITCH_MASK);
        assert_eq!(payload.upload_bytes(), upload_bytes);
        assert!(payload.packed_regions.is_some());
        assert_eq!(frame_state.phase7_sampled_chunks, 1);
        assert_eq!(frame_state.phase7_meshed_chunks, 1);
        assert_eq!(frame_state.phase7_packed_chunks, 1);
        assert_eq!(frame_state.phase7_staged_chunks, 0);
        assert_eq!(frame_state.phase7_commit_payloads, 1);
    }

    #[test]
    fn selection_driven_surface_class_marks_fine_to_coarse_stitch_edges() {
        let mut runtime = test_runtime();
        let fine_key = ChunkKey::new(Face::Px, 2, 1, 1);
        let coarse_neighbor_cover = ChunkKey::new(Face::Px, 1, 0, 0);
        let desired_render = [fine_key, coarse_neighbor_cover]
            .into_iter()
            .collect::<HashSet<_>>();

        let surface_class = runtime
            .required_surface_class_for_selection(fine_key, &desired_render)
            .unwrap();

        assert_eq!(
            surface_class.stitch_mask,
            mesh_topology::stitch_mask_bit(Edge::NegU) | mesh_topology::stitch_mask_bit(Edge::NegV)
        );
    }

    #[test]
    fn payload_residency_budget_stays_bounded_under_mock_camera_churn() {
        let surface_class = sample_surface_class();
        let mut runtime = test_runtime();

        for x in 0..8 {
            let key = ChunkKey::new(Face::Px, 3, x, 0);
            runtime.insert_payload(key, sample_payload(&surface_class, x as u8));
        }

        for center_x in 1..=6 {
            let retained_keys = [
                ChunkKey::new(Face::Px, 3, center_x - 1, 0),
                ChunkKey::new(Face::Px, 3, center_x, 0),
                ChunkKey::new(Face::Px, 3, center_x + 1, 0),
            ]
            .into_iter()
            .collect::<HashSet<_>>();

            for key in retained_keys.iter().copied() {
                runtime
                    .resident_payloads
                    .entry(key)
                    .or_insert_with(|| sample_payload(&surface_class, key.x as u8));
            }

            let _ = runtime.enforce_payload_residency_budget(&retained_keys, 4);

            assert!(runtime.resident_payload_count() <= 4);
            assert!(retained_keys
                .iter()
                .all(|key| runtime.resident_payloads.contains_key(key)));
        }
    }

    #[test]
    fn incompatible_warm_reuse_routes_to_compatible_pool_or_cold_path() {
        let current_surface_class = sample_surface_class();
        let required_surface_class =
            SurfaceClassKey::canonical_chunk(0b0011, 3, DEFAULT_RENDER_FORMAT_MASK, 12, 24, 4)
                .unwrap();
        let pooled_entry = RenderPoolEntry {
            mesh_rid: Rid::Invalid,
            render_instance_rid: Rid::Invalid,
            gd_staging: None,
            surface_class: required_surface_class.clone(),
        };

        let mut runtime = test_runtime();
        runtime.push_render_pool_entry(pooled_entry.clone());

        let reused =
            runtime.choose_render_warm_path(Some(&current_surface_class), &required_surface_class);
        assert_eq!(reused, RenderWarmPath::ReusePooledSurface(pooled_entry));

        let mut runtime = test_runtime();
        let cold_path =
            runtime.choose_render_warm_path(Some(&current_surface_class), &required_surface_class);
        assert_eq!(
            cold_path,
            RenderWarmPath::ColdPath(RenderFallbackReason::IncompatibleCurrentSurfaceClass(vec![
                "stitch_mask",
                "index_class",
                "index_count",
                "index_bytes"
            ]))
        );
    }

    #[test]
    fn register_chunk_meta_recomputes_neighbors_from_phase4_topology() {
        let key = ChunkKey::new(Face::Px, 2, 0, 0);
        let surface_class = sample_surface_class();
        let mut meta = ChunkMeta::new(
            key,
            ChunkBounds::new(DVec3::new(0.0, 0.0, 0.0), 10.0, -1.0, 1.0, 999.0, 1001.0),
            ChunkMetrics::new(1.0, 5.0, 0.1),
            surface_class,
        )
        .unwrap();

        meta.neighbors = ChunkNeighbors::new([
            ChunkKey::new(Face::Nx, 2, 0, 0),
            ChunkKey::new(Face::Nx, 2, 0, 0),
            ChunkKey::new(Face::Nx, 2, 0, 0),
            ChunkKey::new(Face::Nx, 2, 0, 0),
        ]);

        let mut runtime = test_runtime();
        runtime.register_chunk_meta(meta).unwrap();

        let stored = runtime.meta.get(&key).unwrap();
        assert_eq!(
            stored.neighbors.get(Edge::NegU),
            ChunkKey::new(Face::Pz, 2, 3, 0)
        );
        assert_eq!(
            stored.neighbors.get(Edge::PosU),
            ChunkKey::new(Face::Px, 2, 1, 0)
        );
        assert_eq!(
            stored.neighbors.get(Edge::NegV),
            ChunkKey::new(Face::Ny, 2, 3, 3)
        );
        assert_eq!(
            stored.neighbors.get(Edge::PosV),
            ChunkKey::new(Face::Px, 2, 0, 1)
        );
    }

    #[test]
    fn ensure_chunk_meta_lazily_builds_phase6_bounds_and_surface_class() {
        let mut runtime = test_runtime();
        let key = ChunkKey::new(Face::Pz, 3, 5, 2);
        let initial_meta_count = runtime.meta_count();

        let meta = runtime.ensure_chunk_meta(key).unwrap().clone();

        assert_eq!(meta.key, key);
        assert_eq!(runtime.meta_count(), initial_meta_count + 1);
        assert!(meta.bounds.radius > 0.0);
        assert!(meta.metrics.angular_radius > 0.0);
        assert!(meta.metrics.geometric_error > 0.0);
        assert_eq!(
            meta.surface_class.stitch_mask,
            mesh_topology::BASE_STITCH_MASK
        );
        assert_eq!(
            meta.surface_class.vertex_stride,
            DEFAULT_RENDER_VERTEX_STRIDE
        );
    }

    #[test]
    fn selector_normalizes_neighbor_lod_delta_to_one() {
        let mut runtime = test_runtime();
        let camera = orbit_camera_state();

        runtime.step_visibility_selection(&camera).unwrap();

        for key in runtime.active_render.iter().copied() {
            for edge in Edge::ALL {
                let neighbor_same_lod = topology::same_lod_neighbor(key, edge).unwrap();
                if let Some(neighbor) = PlanetRuntime::find_active_ancestor_covering(
                    neighbor_same_lod,
                    &runtime.active_render,
                ) {
                    assert!(
                        key.lod.abs_diff(neighbor.lod) <= 1,
                        "neighbor lod delta exceeded 1: {key:?} vs {neighbor:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn budgeted_selector_defers_work_when_frame_budget_is_tight() {
        let mut runtime = PlanetRuntime::new(
            RuntimeConfig {
                metadata_precompute_max_lod: 0,
                enable_godot_staging: false,
                commit_budget_per_frame: 2,
                upload_budget_bytes_per_frame: 128,
                ..RuntimeConfig::default()
            },
            Rid::Invalid,
            Rid::Invalid,
        );
        let camera = orbit_camera_state();

        runtime.step_visibility_selection(&camera).unwrap();

        assert!(runtime.desired_render_count() > runtime.active_render_count());
        assert!(runtime.deferred_commit_count() > 0);
        assert!(runtime.deferred_upload_bytes() > 0);
        assert!(runtime.frame_state.max_deferred_starvation_frames >= 1);
    }

    #[test]
    fn physics_active_set_stays_separate_from_render_set() {
        let mut runtime = test_runtime();
        let camera = orbit_camera_state();

        for _ in 0..8 {
            runtime.step_visibility_selection(&camera).unwrap();
            if runtime.active_physics_count() > 0 {
                break;
            }
        }

        assert!(runtime.active_render_count() > 0);
        assert!(runtime.active_physics_count() > 0);
        assert!(runtime.active_physics_count() < runtime.active_render_count());
        assert!(runtime
            .active_physics
            .iter()
            .all(|key| runtime.active_render.contains(key)));
    }

    #[test]
    fn phase8_warm_pooled_commit_recycles_previous_render_entry() {
        let mut runtime = PlanetRuntime::new(
            RuntimeConfig {
                metadata_precompute_max_lod: 0,
                enable_godot_staging: false,
                render_pool_watermark_per_class: 4,
                ..RuntimeConfig::default()
            },
            Rid::Invalid,
            Rid::Invalid,
        );
        let key = sample_key();
        let previous_surface_class = sample_surface_class();
        let next_surface_class =
            SurfaceClassKey::canonical_chunk(0b0011, 3, DEFAULT_RENDER_FORMAT_MASK, 12, 24, 4)
                .unwrap();
        let previous_mesh_rid = Rid::Invalid;
        let previous_instance_rid = Rid::new(101);
        let pooled_mesh_rid = Rid::new(202);
        let pooled_instance_rid = Rid::new(303);

        runtime.install_render_entry(
            key,
            previous_mesh_rid,
            previous_instance_rid,
            previous_surface_class.clone(),
            None,
        );
        runtime.resident_payloads.insert(
            key,
            ChunkPayload {
                surface_class: next_surface_class.clone(),
                pooled_render_entry: Some(RenderPoolEntry {
                    mesh_rid: pooled_mesh_rid,
                    render_instance_rid: pooled_instance_rid,
                    surface_class: next_surface_class.clone(),
                    gd_staging: None,
                }),
                render_lifecycle: RenderLifecycleCommand::WarmReusePooled,
                ..sample_payload(&next_surface_class, 7)
            },
        );

        let mut frame_state = SelectionFrameState::default();
        runtime.commit_render_payload(key, &mut frame_state);

        let rid_state = runtime.rid_state.get(&key).unwrap();
        assert_eq!(rid_state.mesh_rid, Some(pooled_mesh_rid));
        assert_eq!(rid_state.render_instance_rid, Some(pooled_instance_rid));
        assert_eq!(
            rid_state.active_surface_class.as_ref(),
            Some(&next_surface_class)
        );
        assert_eq!(frame_state.phase8_render_warm_pool_commits, 1);

        let pooled_previous = runtime.pop_render_pool_entry(&previous_surface_class).unwrap();
        assert_eq!(pooled_previous.mesh_rid, previous_mesh_rid);
        assert_eq!(pooled_previous.render_instance_rid, previous_instance_rid);
    }

    #[test]
    fn phase8_pool_watermarks_bound_recycled_entries() {
        let surface_class = sample_surface_class();
        let mut runtime = PlanetRuntime::new(
            RuntimeConfig {
                metadata_precompute_max_lod: 0,
                enable_godot_staging: false,
                render_pool_watermark_per_class: 1,
                physics_pool_watermark: 1,
                ..RuntimeConfig::default()
            },
            Rid::Invalid,
            Rid::Invalid,
        );

        runtime.recycle_render_entry(RenderPoolEntry {
            mesh_rid: Rid::new(1),
            render_instance_rid: Rid::new(2),
            surface_class: surface_class.clone(),
            gd_staging: None,
        });
        runtime.recycle_render_entry(RenderPoolEntry {
            mesh_rid: Rid::new(3),
            render_instance_rid: Rid::new(4),
            surface_class: surface_class.clone(),
            gd_staging: None,
        });
        runtime.recycle_physics_entry(PhysicsPoolEntry {
            physics_body_rid: Rid::new(5),
            physics_shape_rid: Rid::new(6),
        });
        runtime.recycle_physics_entry(PhysicsPoolEntry {
            physics_body_rid: Rid::new(7),
            physics_shape_rid: Rid::new(8),
        });

        assert_eq!(runtime.render_pool_entry_count(), 1);
        assert_eq!(runtime.physics_pool.len(), 1);
    }
}
