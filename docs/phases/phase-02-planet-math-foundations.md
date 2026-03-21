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

## Implementation Notes

Implemented in:

- `rust/src/runtime.rs`
- `rust/src/lib.rs`

What is now live in code:

- Core runtime types for chunk identity, bounds, metrics, neighbors, payloads, pooled RID ownership, and reusable Godot-owned packed staging buffers.
- Strict `SurfaceClassKey` compatibility rules with precomputed byte expectations derived from stride inputs.
- `PlanetRuntime` ownership maps/sets/queues for metadata, active render/physics sets, resident payloads, RID state, render pools, and physics pools.
- A deterministic payload residency budget helper so bounded payload caches can already be exercised in tests before later streaming phases wire in camera-driven selection.
- `PlanetRoot` runtime ownership plus debug accessors for runtime counts and cached world RID validity.

API constraints verified before implementation on 2026-03-21:

- Godot `RenderingServer` docs were checked for RID ownership expectations and server-managed resource lifetime.
- godot-rust packed-array docs/source were checked for `PackedByteArray::resize()`, `as_slice()`, `as_mut_slice()`, and copy-on-write behavior.

Implementation boundary kept explicit:

- This phase implements the data model and residency/pooling contracts, but planet-wide metadata population still depends on the face-basis and neighbor-mapping work in Phases 03 and 04.

## Checklist

- [x] Implement all core types shown above.
- [x] Keep render and physics active sets independent.
- [x] Key pools by strict surface compatibility.
- [x] Keep Godot-owned packed staging buffers reusable per class.
- [x] Document copy-possible FFI boundary assumptions in code/docs.
- [x] Precompute class stride/byte expectations during metadata build.
- [x] Keep payload precompute window bounded (`PAYLOAD_PRECOMPUTE_MAX_LOD = 5` default).

## Prerequisites

- [x] Phase 01 completed and architecture contract enforced.
- [x] Rust runtime crate has core type modules in place.

## Ordered Build Steps

1. [x] Implement identity, bounds, metrics, and neighbor structs.
2. [x] Implement payload, RID state, and pool entry structs.
3. [x] Implement `PlanetRuntime` ownership maps/sets/queues.
4. [x] Implement strict `SurfaceClassKey` compatibility fields.
5. [x] Implement reusable staging ownership model (`GdPackedStaging`).

## Validation and Test Gates

- [x] Type construction/unit tests pass for all core structs.
- [x] Surface class mismatch detection test passes.
- [x] Runtime map ownership transitions are deterministic in mock lifecycle tests.
- [x] Payload residency stays bounded during aggressive camera movement tests.

## Definition of Done

- [x] Data model compiles and is integration-ready for later phases.
- [x] No scene-tree-primary chunk identity remains.
- [x] FFI boundary assumptions are explicitly documented in code/docs.

## Test Record (Fill In)

- [x] Date: 2026-03-21
- [x] Result summary: Added `rust/src/runtime.rs` with the Phase 02 runtime data model, strict surface-class compatibility checks, reusable packed staging ownership, bounded payload residency helpers, and `PlanetRoot` runtime wiring/debug accessors in `rust/src/lib.rs`.
- [x] Compatibility edge cases validated: `cargo test` passed 6 unit tests covering chunk-key LOD bounds, strict surface-class mismatch detection, packed-region byte validation, deterministic runtime ownership transitions, payload precompute-window bounds, and bounded payload residency under mock camera churn.
- [x] Follow-up actions: Use the Phase 02 data model in Phase 03 face-basis and chunk-local-coordinate work, then fill `ChunkMeta`/neighbor data planet-wide once cross-face mapping rules are implemented.

## References

- [RenderingServer - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_renderingserver.html)
- [godot-rust built-in types (packed arrays)](https://godot-rust.github.io/book/godot-api/builtins.html)
