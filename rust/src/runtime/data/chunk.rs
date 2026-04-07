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

    pub fn label(self) -> &'static str {
        match self {
            Self::NegU => "neg_u",
            Self::PosU => "pos_u",
            Self::NegV => "neg_v",
            Self::PosV => "pos_v",
        }
    }
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
