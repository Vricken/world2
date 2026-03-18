# Phase 02 - Data Model

## Goal

Restore the full runtime data model and ownership contracts so this phase contains complete API and structural detail instead of checklist shorthand.

## Full Data Model Shape

Keep all core terrain data in Rust and only mirror currently active render/physics sets into Godot servers.

```rust
use glam::{DVec2, DVec3};
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Face {
    Px, Nx, Py, Ny, Pz, Nz,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Edge {
    NegU, PosU, NegV, PosV,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChunkKey {
    pub face: Face,
    pub lod: u8,
    pub x: u32,
    pub y: u32,
}

#[derive(Clone, Debug)]
pub struct ChunkBounds {
    pub center_planet: DVec3,
    pub radius: f64,
    pub min_height: f32,
    pub max_height: f32,
    pub min_radius: f64,
    pub max_radius: f64,
}

#[derive(Clone, Debug)]
pub struct ChunkMetrics {
    pub geometric_error: f32,
    pub max_slope_deg: f32,
    pub angular_radius: f32,
}

#[derive(Clone, Debug)]
pub struct ChunkNeighbors {
    pub same_lod: [ChunkKey; 4],
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

#[derive(Clone, Debug)]
pub struct ChunkMeta {
    pub key: ChunkKey,
    pub bounds: ChunkBounds,
    pub metrics: ChunkMetrics,
    pub neighbors: ChunkNeighbors,
    pub surface_class: SurfaceClassKey,
}

#[derive(Clone, Debug)]
pub struct CpuMeshBuffers {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub tangents: Vec<[f32; 4]>,
    pub uvs: Vec<[f32; 2]>,
    pub colors: Vec<[f32; 4]>,
    pub indices: Vec<i32>,
}

#[derive(Clone, Debug)]
pub struct PackedMeshRegions {
    pub vertex_region: Vec<u8>,
    pub attribute_region: Vec<u8>,
    pub index_region: Vec<u8>,
    pub vertex_stride: usize,
    pub attribute_stride: usize,
    pub index_stride: usize,
}

#[derive(Clone, Debug)]
pub struct GdPackedStaging {
    pub vertex_region: PackedByteArray,
    pub attribute_region: PackedByteArray,
    pub index_region: PackedByteArray,
}

#[derive(Clone, Debug)]
pub struct AssetInstance {
    pub family_id: u16,
    pub origin: DVec3,
    pub basis_x: DVec3,
    pub basis_y: DVec3,
    pub basis_z: DVec3,
    pub scale: f32,
    pub color_seed: u32,
}

#[derive(Clone, Debug)]
pub struct ChunkPayload {
    pub mesh: CpuMeshBuffers,
    pub packed_regions: Option<PackedMeshRegions>,
    pub assets: Vec<AssetInstance>,
    pub collider_vertices: Option<Vec<[f32; 3]>>,
    pub collider_indices: Option<Vec<i32>>,
}

#[derive(Clone, Debug, Default)]
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

#[derive(Clone, Debug)]
pub struct RenderPoolEntry {
    pub mesh_rid: Rid,
    pub render_instance_rid: Rid,
    pub surface_class: SurfaceClassKey,
    pub gd_staging: GdPackedStaging,
}

#[derive(Clone, Debug)]
pub struct PhysicsPoolEntry {
    pub physics_body_rid: Rid,
    pub physics_shape_rid: Rid,
}

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
```

## Core Rules

1. `ChunkMeta` exists for every possible chunk you may activate.
2. `ChunkPayload` exists only for chunks you choose to precompute and keep resident.
3. `ChunkRidState` is the authoritative bridge between Rust chunk state and Godot server objects.
4. No chunk identity should live primarily in scene-tree nodes.
5. Pool ownership is keyed by surface compatibility, not by loose RID availability.
6. Packed staging buffers are keyed by surface compatibility and live with pooled render entries.

This sixth rule is critical. gdext packed arrays are contiguous, support `resize()`, `as_slice()`, and `as_mut_slice()`, and use copy-on-write semantics. The docs do not guarantee a general raw-pointer adoption path for zero-copy wrapping of arbitrary Rust allocations, so architecture must not depend on undocumented zero-copy behavior.

For static planets, precompute all metadata for all LODs, but cap render-resident payload precompute with an explicit window. Default to `PAYLOAD_PRECOMPUTE_MAX_LOD = 5`. For LODs above that window, generate payloads on demand and keep residency bounded by runtime budgets and cache policy. Also precompute `SurfaceClassKey` plus stride helpers/class constants to reduce runtime allocator pressure.

## Checklist

- [ ] Implement all core types shown above.
- [ ] Keep render and physics active sets independent.
- [ ] Key pools by strict surface compatibility.
- [ ] Keep Godot-owned packed staging buffers reusable per class.
- [ ] Document copy-possible FFI boundary assumptions in code/docs.
- [ ] Precompute class stride/byte expectations during metadata build.
- [ ] Keep payload precompute window bounded (`PAYLOAD_PRECOMPUTE_MAX_LOD = 5` default).

## Prerequisites

- [ ] Phase 01 completed and architecture contract enforced.
- [ ] Rust runtime crate has core type modules in place.

## Ordered Build Steps

1. [ ] Implement identity, bounds, metrics, and neighbor structs.
2. [ ] Implement payload, RID state, and pool entry structs.
3. [ ] Implement `PlanetRuntime` ownership maps/sets/queues.
4. [ ] Implement strict `SurfaceClassKey` compatibility fields.
5. [ ] Implement reusable staging ownership model (`GdPackedStaging`).

## Validation and Test Gates

- [ ] Type construction/unit tests pass for all core structs.
- [ ] Surface class mismatch detection test passes.
- [ ] Runtime map ownership transitions are deterministic in mock lifecycle tests.
- [ ] Payload residency stays bounded during aggressive camera movement tests.

## Definition of Done

- [ ] Data model compiles and is integration-ready for later phases.
- [ ] No scene-tree-primary chunk identity remains.
- [ ] FFI boundary assumptions are explicitly documented in code/docs.

## Test Record (Fill In)

- [ ] Date:
- [ ] Result summary:
- [ ] Compatibility edge cases validated:
- [ ] Follow-up actions:

## References

- [RenderingServer - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_renderingserver.html)
- [godot-rust built-in types (packed arrays)](https://godot-rust.github.io/book/godot-api/builtins.html)
