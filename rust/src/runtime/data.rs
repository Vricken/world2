use super::*;

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

    pub fn ancestor_at_lod(&self, target_lod: u8) -> Option<Self> {
        if target_lod > self.lod {
            return None;
        }

        let mut current = *self;
        while current.lod > target_lod {
            current = current.parent()?;
        }
        Some(current)
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

#[derive(Clone, Copy, Debug, PartialEq)]
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

#[derive(Clone, Copy, Debug, PartialEq)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct StoredChunkMeta {
    pub bounds: ChunkBounds,
    pub metrics: ChunkMetrics,
    pub neighbors: ChunkNeighbors,
}

impl StoredChunkMeta {
    pub(crate) fn from_chunk_meta(meta: &ChunkMeta) -> Self {
        Self {
            bounds: meta.bounds,
            metrics: meta.metrics,
            neighbors: meta.neighbors,
        }
    }

    pub(crate) fn to_chunk_meta(
        &self,
        key: ChunkKey,
        surface_class: SurfaceClassKey,
    ) -> Result<ChunkMeta, TopologyError> {
        Ok(ChunkMeta {
            key,
            bounds: self.bounds,
            metrics: self.metrics,
            neighbors: self.neighbors,
            surface_class,
        })
    }
}

#[derive(Clone, Debug)]
pub struct MetadataStore {
    dense_max_lod: u8,
    dense_levels: Vec<[Vec<Option<StoredChunkMeta>>; 6]>,
    sparse_levels: HashMap<ChunkKey, StoredChunkMeta>,
    len: usize,
}

impl MetadataStore {
    pub fn new(runtime_max_lod: u8, dense_max_lod: u8) -> Self {
        let dense_max_lod = dense_max_lod.min(runtime_max_lod);
        let level_count = usize::from(dense_max_lod) + 1;
        let dense_levels = (0..level_count)
            .map(|_| std::array::from_fn(|_| Vec::new()))
            .collect();
        Self {
            dense_max_lod,
            dense_levels,
            sparse_levels: HashMap::new(),
            len: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn dense_max_lod(&self) -> u8 {
        self.dense_max_lod
    }

    pub fn sparse_count(&self) -> usize {
        self.sparse_levels.len()
    }

    pub fn level_is_built(&self, lod: u8) -> bool {
        if !self.is_dense_lod(lod) {
            return false;
        }

        self.dense_levels
            .get(usize::from(lod))
            .map(|faces| faces.iter().all(|entries| !entries.is_empty()))
            .unwrap_or(false)
    }

    pub fn contains_key(&self, key: &ChunkKey) -> bool {
        self.get_stored(key).is_some()
    }

    pub fn center_planet(&self, key: &ChunkKey) -> Option<DVec3> {
        self.get_stored(key)
            .map(|stored| stored.bounds.center_planet)
    }

    pub fn neighbors(&self, key: &ChunkKey) -> Option<ChunkNeighbors> {
        self.get_stored(key).map(|stored| stored.neighbors)
    }

    pub fn get_chunk_meta(
        &self,
        key: ChunkKey,
        surface_class: SurfaceClassKey,
    ) -> Result<Option<ChunkMeta>, TopologyError> {
        self.get_stored(&key)
            .map(|stored| stored.to_chunk_meta(key, surface_class))
            .transpose()
    }

    pub fn insert_chunk_meta(
        &mut self,
        meta: ChunkMeta,
        prevalidated_neighbors: bool,
    ) -> Result<Option<ChunkMeta>, TopologyError> {
        let mut meta = meta;
        if !prevalidated_neighbors {
            meta.refresh_same_lod_neighbors()?;
        }

        if self.is_dense_lod(meta.key.lod) {
            let (lod_index, face_index, slot_index, slot_count) =
                self.slot_components(&meta.key)
                    .ok_or(TopologyError::InvalidChunkKey)?;
            let face_slots = &mut self.dense_levels[lod_index][face_index];
            if face_slots.is_empty() {
                face_slots.resize(slot_count, None);
            }

            let previous = face_slots[slot_index]
                .replace(StoredChunkMeta::from_chunk_meta(&meta))
                .map(|stored| stored.to_chunk_meta(meta.key, meta.surface_class.clone()))
                .transpose()?;
            if previous.is_none() {
                self.len += 1;
            }
            Ok(previous)
        } else {
            let previous = self
                .sparse_levels
                .insert(meta.key, StoredChunkMeta::from_chunk_meta(&meta))
                .map(|stored| stored.to_chunk_meta(meta.key, meta.surface_class.clone()))
                .transpose()?;
            if previous.is_none() {
                self.len += 1;
            }
            Ok(previous)
        }
    }

    pub(crate) fn set_face_level(
        &mut self,
        face: Face,
        lod: u8,
        entries: Vec<StoredChunkMeta>,
    ) -> Result<(), TopologyError> {
        if !self.is_dense_lod(lod) {
            return Err(TopologyError::InvalidChunkKey);
        }

        let slot_count = self.face_slot_count(lod);
        if entries.len() != slot_count {
            return Err(TopologyError::InvalidChunkKey);
        }

        let lod_index = usize::from(lod);
        let face_index = Self::face_index(face);
        let previous_count = self.dense_levels[lod_index][face_index]
            .iter()
            .filter(|entry| entry.is_some())
            .count();
        self.len = self.len.saturating_sub(previous_count);
        self.len += entries.len();
        self.dense_levels[lod_index][face_index] = entries.into_iter().map(Some).collect();
        Ok(())
    }

    fn get_stored(&self, key: &ChunkKey) -> Option<&StoredChunkMeta> {
        if self.is_dense_lod(key.lod) {
            let (lod_index, face_index, slot_index, _) = self.slot_components(key)?;
            self.dense_levels
                .get(lod_index)?
                .get(face_index)?
                .get(slot_index)?
                .as_ref()
        } else {
            self.sparse_levels.get(key)
        }
    }

    fn is_dense_lod(&self, lod: u8) -> bool {
        lod <= self.dense_max_lod
    }

    fn slot_components(&self, key: &ChunkKey) -> Option<(usize, usize, usize, usize)> {
        if !key.is_valid_for_lod() {
            return None;
        }
        let lod_index = usize::from(key.lod);
        let face_index = Self::face_index(key.face);
        let resolution = ChunkKey::resolution_for_lod(key.lod) as usize;
        let slot_index = key.y as usize * resolution + key.x as usize;
        Some((lod_index, face_index, slot_index, resolution * resolution))
    }

    fn face_slot_count(&self, lod: u8) -> usize {
        let resolution = ChunkKey::resolution_for_lod(lod) as usize;
        resolution * resolution
    }

    fn face_index(face: Face) -> usize {
        match face {
            Face::Px => 0,
            Face::Nx => 1,
            Face::Py => 2,
            Face::Ny => 3,
            Face::Pz => 4,
            Face::Nz => 5,
        }
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OriginPolicyMode {
    SharedCameraRelative,
    EngineLargeWorld,
}

impl OriginPolicyMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::SharedCameraRelative => "shared_camera_relative",
            Self::EngineLargeWorld => "engine_large_world",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OriginSnapshot {
    pub mode: OriginPolicyMode,
    pub render_origin_planet: DVec3,
    pub physics_origin_planet: DVec3,
}

impl OriginSnapshot {
    pub fn for_config(config: &RuntimeConfig, camera_position_planet: DVec3) -> Self {
        if config.use_large_world_coordinates {
            Self {
                mode: OriginPolicyMode::EngineLargeWorld,
                render_origin_planet: DVec3::ZERO,
                physics_origin_planet: DVec3::ZERO,
            }
        } else {
            Self {
                mode: OriginPolicyMode::SharedCameraRelative,
                render_origin_planet: camera_position_planet,
                physics_origin_planet: camera_position_planet,
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenderLifecycleCommand {
    WarmReuseCurrent,
    WarmReusePooled,
    ColdCreate(RenderFallbackReason),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChunkPayload {
    pub payload_epoch: u64,
    pub surface_class: SurfaceClassKey,
    pub stitch_mask: u8,
    pub sample_count: usize,
    pub chunk_origin_planet: DVec3,
    pub mesh: CpuMeshBuffers,
    pub packed_regions: Option<PackedMeshRegions>,
    pub gd_staging: Option<GdPackedStaging>,
    pub pooled_render_entry: Option<RenderPoolEntry>,
    pub assets: Vec<AssetInstance>,
    pub collider_vertices: Option<Vec<[f32; 3]>>,
    pub collider_indices: Option<Vec<i32>>,
    pub collider_faces: Option<Vec<[f32; 3]>>,
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
            payload_epoch: 0,
            surface_class,
            stitch_mask: mesh_topology::BASE_STITCH_MASK,
            sample_count: 0,
            chunk_origin_planet: DVec3::ZERO,
            mesh: CpuMeshBuffers::default(),
            packed_regions: None,
            gd_staging: None,
            pooled_render_entry: None,
            assets: Vec::new(),
            collider_vertices: None,
            collider_indices: None,
            collider_faces: None,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeamDebugSnapshot {
    pub active_render_chunks: usize,
    pub active_chunks_with_surface_class: usize,
    pub active_chunks_missing_surface_class: usize,
    pub active_stitched_chunks: usize,
    pub pending_surface_class_mismatch_chunks: usize,
    pub active_stitch_mask_counts: [usize; mesh_topology::STITCH_VARIANT_COUNT],
    pub active_stitched_edge_counts: [usize; Edge::ALL.len()],
    pub pooled_render_entries: usize,
    pub pooled_stitch_mask_counts: [usize; mesh_topology::STITCH_VARIANT_COUNT],
}

impl Default for SeamDebugSnapshot {
    fn default() -> Self {
        Self {
            active_render_chunks: 0,
            active_chunks_with_surface_class: 0,
            active_chunks_missing_surface_class: 0,
            active_stitched_chunks: 0,
            pending_surface_class_mismatch_chunks: 0,
            active_stitch_mask_counts: [0; mesh_topology::STITCH_VARIANT_COUNT],
            active_stitched_edge_counts: [0; Edge::ALL.len()],
            pooled_render_entries: 0,
            pooled_stitch_mask_counts: [0; mesh_topology::STITCH_VARIANT_COUNT],
        }
    }
}

impl SeamDebugSnapshot {
    pub fn active_stitch_mask_summary(&self) -> String {
        stitch_mask_summary(&self.active_stitch_mask_counts)
    }

    pub fn pooled_stitch_mask_summary(&self) -> String {
        stitch_mask_summary(&self.pooled_stitch_mask_counts)
    }

    pub fn active_stitched_edge_summary(&self) -> String {
        Edge::ALL
            .into_iter()
            .enumerate()
            .map(|(index, edge)| {
                format!(
                    "{}:{}",
                    edge.label(),
                    self.active_stitched_edge_counts[index]
                )
            })
            .collect::<Vec<_>>()
            .join("|")
    }

    pub(crate) fn record_active_mask(&mut self, stitch_mask: u8) {
        self.active_chunks_with_surface_class += 1;
        record_mask(
            stitch_mask,
            &mut self.active_stitch_mask_counts,
            Some((
                &mut self.active_stitched_chunks,
                &mut self.active_stitched_edge_counts,
            )),
        );
    }

    pub(crate) fn record_pooled_mask(&mut self, stitch_mask: u8) {
        self.pooled_render_entries += 1;
        record_mask(stitch_mask, &mut self.pooled_stitch_mask_counts, None);
    }
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
            render_backend: RenderBackendKind::ServerPool,
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
    pub lod_reference_height_px_override: Option<f32>,
    pub origin: OriginSnapshot,
}

impl CameraState {
    pub fn from_godot(
        transform: Transform3D,
        frustum_planes: [Plane; 6],
        fov_y_degrees: f32,
        viewport_height_px: f32,
        lod_reference_height_px_override: Option<f32>,
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
            lod_reference_height_px_override,
            origin,
        }
    }

    pub fn effective_lod_reference_height_px(&self, default_reference_height_px: f32) -> f32 {
        self.lod_reference_height_px_override
            .unwrap_or(default_reference_height_px)
            .max(1.0)
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

impl Edge {
    pub fn label(self) -> &'static str {
        match self {
            Self::NegU => "neg_u",
            Self::PosU => "pos_u",
            Self::NegV => "neg_v",
            Self::PosV => "pos_v",
        }
    }
}

fn stitch_mask_summary(counts: &[usize; mesh_topology::STITCH_VARIANT_COUNT]) -> String {
    let mut parts = Vec::new();

    for (mask, count) in counts.iter().copied().enumerate() {
        if count > 0 {
            parts.push(format!("{mask}:{count}"));
        }
    }

    if parts.is_empty() {
        "none".to_string()
    } else {
        parts.join("|")
    }
}

fn record_mask(
    stitch_mask: u8,
    counts: &mut [usize; mesh_topology::STITCH_VARIANT_COUNT],
    stitched_detail: Option<(&mut usize, &mut [usize; Edge::ALL.len()])>,
) {
    let mask_index = usize::from(stitch_mask);
    if mask_index >= counts.len() {
        return;
    }

    counts[mask_index] += 1;

    let Some((stitched_chunks, stitched_edges)) = stitched_detail else {
        return;
    };
    if stitch_mask == mesh_topology::BASE_STITCH_MASK {
        return;
    }

    *stitched_chunks += 1;
    for (index, edge) in Edge::ALL.into_iter().enumerate() {
        if stitch_mask & mesh_topology::stitch_mask_bit(edge) != 0 {
            stitched_edges[index] += 1;
        }
    }
}
