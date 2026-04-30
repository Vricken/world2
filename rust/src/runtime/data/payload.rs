use super::*;

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
    pub height_norm: f32,
    pub moisture: f32,
    pub land_mask: f32,
    pub slope_hint: f32,
}

impl ChunkSample {
    pub fn displaced_point(&self, planet_radius: f64) -> DVec3 {
        self.unit_dir * (planet_radius + f64::from(self.height))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CachedAabb {
    pub position: [f32; 3],
    pub size: [f32; 3],
}

impl CachedAabb {
    pub fn from_min_max(min: Vector3, max: Vector3) -> Self {
        Self {
            position: [min.x, min.y, min.z],
            size: [max.x - min.x, max.y - min.y, max.z - min.z],
        }
    }

    pub fn to_aabb(self) -> Aabb {
        Aabb::new(
            Vector3::new(self.position[0], self.position[1], self.position[2]),
            Vector3::new(self.size[0], self.size[1], self.size[2]),
        )
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

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    pub fn get(&self, x: u32, y: u32) -> &ChunkSample {
        let index = (y * self.samples_per_edge + x) as usize;
        &self.samples[index]
    }

    pub fn to_render_tile_payload(&self) -> ChunkRenderTilePayload {
        ChunkRenderTilePayload::from_samples(self.samples_per_edge, &self.samples)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChunkRenderTilePayload {
    pub samples_per_edge: u32,
    pub height_tile: Vec<f32>,
    pub material_tile: Option<Vec<[f32; 4]>>,
    pub normal_tile: Option<Vec<[f32; 3]>>,
}

impl ChunkRenderTilePayload {
    pub fn from_samples(samples_per_edge: u32, samples: &[ChunkSample]) -> Self {
        let height_tile = samples
            .iter()
            .map(|sample| sample.height)
            .collect::<Vec<_>>();
        let material_tile = samples
            .iter()
            .map(|sample| {
                [
                    sample.height_norm,
                    sample.slope_hint,
                    sample.moisture,
                    sample.land_mask,
                ]
            })
            .collect::<Vec<_>>();

        Self {
            samples_per_edge,
            height_tile,
            material_tile: Some(material_tile),
            normal_tile: None,
        }
    }

    pub fn sample_count(&self) -> usize {
        (self.samples_per_edge * self.samples_per_edge) as usize
    }

    pub fn validate_layout(&self) -> Result<(), &'static str> {
        let expected = self.sample_count();
        if self.height_tile.len() != expected {
            return Err("height_tile");
        }
        if self
            .material_tile
            .as_ref()
            .is_some_and(|tile| tile.len() != expected)
        {
            return Err("material_tile");
        }
        if self
            .normal_tile
            .as_ref()
            .is_some_and(|tile| tile.len() != expected)
        {
            return Err("normal_tile");
        }

        Ok(())
    }

    pub fn byte_len(&self) -> usize {
        self.height_tile.len() * std::mem::size_of::<f32>()
            + self
                .material_tile
                .as_ref()
                .map(|tile| tile.len() * std::mem::size_of::<[f32; 4]>())
                .unwrap_or(0)
            + self
                .normal_tile
                .as_ref()
                .map(|tile| tile.len() * std::mem::size_of::<[f32; 3]>())
                .unwrap_or(0)
    }

    pub fn height_at(&self, x: u32, y: u32) -> f32 {
        self.height_tile[(y * self.samples_per_edge + x) as usize]
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
    pub render_tile: ChunkRenderTilePayload,
    pub render_tile_handle: Option<RenderTileHandle>,
    pub gpu_custom_aabb: Option<CachedAabb>,
    pub assets: Vec<AssetInstance>,
    pub collision: ChunkCollisionPayload,
    pub render_lifecycle: RenderLifecycleCommand,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PayloadBuildRequirements {
    pub cpu_render_data: bool,
    pub collision_mesh: bool,
}

impl PayloadBuildRequirements {
    pub const fn new(cpu_render_data: bool, collision_mesh: bool) -> Self {
        Self {
            cpu_render_data,
            collision_mesh,
        }
    }

    pub const fn requires_cpu_mesh(self) -> bool {
        self.cpu_render_data || self.collision_mesh
    }

    pub const fn satisfies(self, required: Self) -> bool {
        (!required.cpu_render_data || self.cpu_render_data)
            && (!required.collision_mesh || self.collision_mesh)
    }
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
            render_tile: ChunkRenderTilePayload::default(),
            render_tile_handle: None,
            gpu_custom_aabb: None,
            assets: Vec::new(),
            collision: ChunkCollisionPayload::default(),
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

    pub fn render_tile_bytes(&self) -> usize {
        self.render_tile.byte_len()
    }

    pub fn has_cpu_render_data(&self) -> bool {
        self.packed_regions.is_some()
    }

    pub fn has_collision_mesh_data(&self) -> bool {
        !self.mesh.positions.is_empty() && !self.mesh.indices.is_empty()
    }

    pub fn build_requirements(&self) -> PayloadBuildRequirements {
        PayloadBuildRequirements::new(self.has_cpu_render_data(), self.has_collision_mesh_data())
    }

    pub fn collision_resident_bytes(&self) -> usize {
        let mut bytes = 0usize;
        if let Some(vertices) = self.collision.collider_vertices.as_ref() {
            bytes = bytes.saturating_add(vertices.len() * std::mem::size_of::<[f32; 3]>());
        } else if self.has_collision_mesh_data() {
            bytes =
                bytes.saturating_add(self.mesh.positions.len() * std::mem::size_of::<[f32; 3]>());
        }

        if let Some(indices) = self.collision.collider_indices.as_ref() {
            bytes = bytes.saturating_add(indices.len() * std::mem::size_of::<i32>());
        } else if self.has_collision_mesh_data() {
            bytes = bytes.saturating_add(self.mesh.indices.len() * std::mem::size_of::<i32>());
        }

        if let Some(faces) = self.collision.collider_faces.as_ref() {
            bytes = bytes.saturating_add(faces.len() * std::mem::size_of::<[f32; 3]>());
        }

        bytes
    }

    pub fn clear_collision_data(&mut self) {
        self.mesh = CpuMeshBuffers::default();
        self.collision = ChunkCollisionPayload::default();
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChunkCollisionPayload {
    pub collider_vertices: Option<Vec<[f32; 3]>>,
    pub collider_indices: Option<Vec<i32>>,
    pub collider_faces: Option<Vec<[f32; 3]>>,
}
