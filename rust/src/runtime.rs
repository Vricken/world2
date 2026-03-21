use std::collections::{HashMap, HashSet, VecDeque};

use glam::DVec3;
use godot::builtin::{PackedByteArray, Rid};

/// Treat Rust->Godot packed-array transfer as copy-possible unless the docs
/// explicitly guarantee a zero-copy ownership handoff for the exact API used.
pub const PAYLOAD_PRECOMPUTE_MAX_LOD: u8 = 5;

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
    pub lod_class: u8,
    pub stitch_mask: u8,
    pub material_class: u8,
    pub vertex_count: u32,
    pub index_count: u32,
    pub format_mask: u64,
    pub vertex_bytes: usize,
    pub attribute_bytes: usize,
    pub index_bytes: usize,
}

impl SurfaceClassKey {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        lod_class: u8,
        stitch_mask: u8,
        material_class: u8,
        vertex_count: u32,
        index_count: u32,
        format_mask: u64,
        vertex_stride: usize,
        attribute_stride: usize,
        index_stride: usize,
    ) -> Self {
        Self {
            lod_class,
            stitch_mask,
            material_class,
            vertex_count,
            index_count,
            format_mask,
            vertex_bytes: vertex_stride.saturating_mul(vertex_count as usize),
            attribute_bytes: attribute_stride.saturating_mul(vertex_count as usize),
            index_bytes: index_stride.saturating_mul(index_count as usize),
        }
    }

    pub fn compatibility_issues(&self, other: &Self) -> Vec<&'static str> {
        let mut issues = Vec::new();

        if self.lod_class != other.lod_class {
            issues.push("lod_class");
        }
        if self.stitch_mask != other.stitch_mask {
            issues.push("stitch_mask");
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

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChunkPayload {
    pub mesh: CpuMeshBuffers,
    pub packed_regions: Option<PackedMeshRegions>,
    pub assets: Vec<AssetInstance>,
    pub collider_vertices: Option<Vec<[f32; 3]>>,
    pub collider_indices: Option<Vec<i32>>,
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
    pub pooled_surface_class: Option<SurfaceClassKey>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderPoolEntry {
    pub mesh_rid: Rid,
    pub render_instance_rid: Rid,
    pub surface_class: SurfaceClassKey,
    pub gd_staging: GdPackedStaging,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PhysicsPoolEntry {
    pub physics_body_rid: Rid,
    pub physics_shape_rid: Rid,
}

#[derive(Debug)]
pub struct PlanetRuntime {
    pub scenario_rid: Rid,
    pub physics_space_rid: Rid,
    pub meta: HashMap<ChunkKey, ChunkMeta>,
    pub active_render: HashSet<ChunkKey>,
    pub active_physics: HashSet<ChunkKey>,
    pub resident_payloads: HashMap<ChunkKey, ChunkPayload>,
    pub rid_state: HashMap<ChunkKey, ChunkRidState>,
    pub render_pool: HashMap<SurfaceClassKey, VecDeque<RenderPoolEntry>>,
    pub physics_pool: VecDeque<PhysicsPoolEntry>,
}

impl Default for PlanetRuntime {
    fn default() -> Self {
        Self::new(Rid::Invalid, Rid::Invalid)
    }
}

impl PlanetRuntime {
    pub fn new(scenario_rid: Rid, physics_space_rid: Rid) -> Self {
        Self {
            scenario_rid,
            physics_space_rid,
            meta: HashMap::new(),
            active_render: HashSet::new(),
            active_physics: HashSet::new(),
            resident_payloads: HashMap::new(),
            rid_state: HashMap::new(),
            render_pool: HashMap::new(),
            physics_pool: VecDeque::new(),
        }
    }

    pub fn payload_precompute_max_lod(&self) -> u8 {
        PAYLOAD_PRECOMPUTE_MAX_LOD
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

    pub fn register_chunk_meta(&mut self, meta: ChunkMeta) -> Option<ChunkMeta> {
        self.meta.insert(meta.key, meta)
    }

    pub fn insert_payload(&mut self, key: ChunkKey, payload: ChunkPayload) -> Option<ChunkPayload> {
        self.resident_payloads.insert(key, payload)
    }

    pub fn remove_payload(&mut self, key: &ChunkKey) -> Option<ChunkPayload> {
        self.resident_payloads.remove(key)
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

            if self.resident_payloads.remove(&key).is_some() {
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

    pub fn push_physics_pool_entry(&mut self, entry: PhysicsPoolEntry) {
        self.physics_pool.push_back(entry);
    }

    pub fn pop_physics_pool_entry(&mut self) -> Option<PhysicsPoolEntry> {
        self.physics_pool.pop_front()
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

    pub fn resident_payload_count(&self) -> usize {
        self.resident_payloads.len()
    }

    pub fn rid_state_count(&self) -> usize {
        self.rid_state.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_surface_class() -> SurfaceClassKey {
        SurfaceClassKey::new(2, 0b0101, 3, 64, 96, 0x1F, 12, 24, 4)
    }

    fn sample_key() -> ChunkKey {
        ChunkKey::new(Face::Px, 2, 1, 1)
    }

    fn sample_meta(key: ChunkKey, surface_class: SurfaceClassKey) -> ChunkMeta {
        ChunkMeta {
            key,
            bounds: ChunkBounds::new(DVec3::new(1.0, 2.0, 3.0), 50.0, -20.0, 40.0, 80.0, 140.0),
            metrics: ChunkMetrics::new(2.5, 42.0, 0.25),
            neighbors: ChunkNeighbors::new([
                ChunkKey::new(Face::Px, 2, 0, 1),
                ChunkKey::new(Face::Px, 2, 2, 1),
                ChunkKey::new(Face::Px, 2, 1, 0),
                ChunkKey::new(Face::Px, 2, 1, 2),
            ]),
            surface_class,
        }
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

    #[test]
    fn chunk_key_validates_coords_against_lod_resolution() {
        assert!(ChunkKey::new(Face::Px, 3, 7, 7).is_valid_for_lod());
        assert!(!ChunkKey::new(Face::Px, 3, 8, 7).is_valid_for_lod());
        assert_eq!(ChunkKey::resolution_for_lod(5), 32);
    }

    #[test]
    fn surface_class_mismatch_detection_is_strict() {
        let base = sample_surface_class();
        let mismatched = SurfaceClassKey::new(2, 0b0101, 3, 64, 96, 0x1F, 16, 24, 4);

        let issues = base.compatibility_issues(&mismatched);

        assert_eq!(issues, vec!["vertex_bytes"]);
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
    }

    #[test]
    fn runtime_map_transitions_are_deterministic() {
        let key = sample_key();
        let surface_class = sample_surface_class();
        let meta = sample_meta(key, surface_class.clone());
        let payload = sample_payload(&surface_class, 1);

        let mut runtime = PlanetRuntime::default();
        runtime.register_chunk_meta(meta);
        runtime.activate_render(key);
        runtime.activate_physics(key);
        runtime.insert_payload(key, payload);
        runtime.ensure_rid_state(key).render_resident = true;
        runtime.ensure_rid_state(key).physics_resident = true;

        assert_eq!(runtime.meta_count(), 1);
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
    fn payload_residency_budget_stays_bounded_under_mock_camera_churn() {
        let surface_class = sample_surface_class();
        let mut runtime = PlanetRuntime::default();

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
}
