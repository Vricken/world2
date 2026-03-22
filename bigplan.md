
## 1) Recommended project shape

Use this scene layout:

```text
PlanetRoot (Node3D / Rust GodotClass)
├── DebugRoot (Node3D, optional)
├── CameraAnchor / gameplay nodes
└── No per-chunk terrain or collision nodes
```

This remains the core structural decision. `PlanetRoot` is still a thin shell used to own the Rust planet runtime, fetch `World3D`, cache the rendering scenario RID and physics space RID, drive the update loop, and expose debug/editor controls. Terrain chunks still do **not** exist as `MeshInstance3D` or `StaticBody3D` nodes. Their visible state lives in `RenderingServer` resources and instance RIDs attached to a scenario, while their collision state lives in `PhysicsServer3D` body and shape RIDs attached to a physics space. Godot’s docs explicitly support bypassing the scene tree this way and also state that this helps only when the scene system is actually the bottleneck. ([Godot Engine documentation][1])

Use a **cube-sphere with 6 face quadtrees**. That still gives you square chunks, clean per-face addressing, deterministic neighbor traversal, and practical chunked LOD without pole singularities. The production default projection should still be the **modified / spherified cube** mapping rather than naive normalized-cube, while still keeping projection as a swappable strategy so you can test alternatives later without destabilizing the rest of the engine. This is a design choice rather than a Godot API requirement, but it stays unchanged because it is still the best fit for chunked terrain, seam handling, and asset placement.

The top-level Godot scene should still stay small. All chunk creation, destruction, visibility, transform updates, pooling, region updates, packed staging buffer reuse, and collision residency are handled by Rust-owned metadata plus Godot server RIDs. The only scene-tree objects left should still be the shell nodes you genuinely need for gameplay, camera, debug visualization, audio, and editor integration. Godot’s own server documentation describes the scene system as optional and the servers as the low-level layer the scene system itself uses. ([Godot Engine documentation][2])

## 2) Data model

Keep all core terrain data in Rust and only mirror the currently active render and physics sets into Godot’s servers.

```rust
use glam::{DVec2, DVec3};
use std::collections::{HashMap, HashSet, VecDeque};

// Pseudocode shape only.

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
    pub same_lod: [ChunkKey; 4], // [NegU, PosU, NegV, PosV]
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

The original rules still apply:

1. `ChunkMeta` exists for **every possible chunk** you may activate.
2. `ChunkPayload` exists only for chunks you choose to precompute and keep resident.
3. `ChunkRidState` is the authoritative bridge between Rust chunk state and Godot server objects.
4. No chunk identity should live primarily in scene-tree nodes anymore.
5. Pool ownership is keyed by **surface compatibility**, not just “a free RID exists somewhere.”
6. Packed staging buffers are also keyed by **surface compatibility** and live with the pooled render entry so the warm path can reuse Godot-owned memory instead of rebuilding temporary arrays.

That sixth rule is the new addition. gdext packed arrays are contiguous, expose `resize()`, `as_slice()`, and `as_mut_slice()`, and use copy-on-write semantics. The docs also show conversions from slices and `Vec<T>`, but they do not document a general raw-pointer adoption path that would let you safely design around zero-copy wrapping of arbitrary Rust-owned buffers. That is why `PackedMeshRegions` remains a Rust-side packing format, while `GdPackedStaging` is the reusable Godot-owned hot-path staging format. ([Godot Rust][3])

For a static planet, I would still precompute **all metadata for all LODs**, then precompute render-resident payloads for all chunks up to a practical max LOD during the loading phase. The new addition is that I would also precompute the `SurfaceClassKey` for every chunk, prebuild any per-class packing helpers and stride constants, and allocate per-class or per-worker scratch arenas up front. That keeps runtime work limited to visibility/LOD selection, RID activation/deactivation, region updates against compatible surfaces, and bounded queue processing instead of repeated allocator pressure. Godot exposes the relevant stride helpers and region update calls directly on `RenderingServer`, while gdext packed arrays are designed for reusable contiguous storage. ([Godot Engine documentation][1])

## 3) Face basis and chunk-local coordinates

Define each face by a right-handed basis `(n, u, v)` where `n` is the face normal.

```rust
#[derive(Clone, Copy)]
pub struct FaceBasis {
    pub n: DVec3,
    pub u: DVec3,
    pub v: DVec3,
}

pub fn face_basis(face: Face) -> FaceBasis {
    match face {
        Face::Px => FaceBasis {
            n: DVec3::new( 1.0, 0.0, 0.0),
            u: DVec3::new( 0.0, 0.0,-1.0),
            v: DVec3::new( 0.0, 1.0, 0.0),
        },
        Face::Nx => FaceBasis {
            n: DVec3::new(-1.0, 0.0, 0.0),
            u: DVec3::new( 0.0, 0.0, 1.0),
            v: DVec3::new( 0.0, 1.0, 0.0),
        },
        Face::Py => FaceBasis {
            n: DVec3::new( 0.0, 1.0, 0.0),
            u: DVec3::new( 1.0, 0.0, 0.0),
            v: DVec3::new( 0.0, 0.0,-1.0),
        },
        Face::Ny => FaceBasis {
            n: DVec3::new( 0.0,-1.0, 0.0),
            u: DVec3::new( 1.0, 0.0, 0.0),
            v: DVec3::new( 0.0, 0.0, 1.0),
        },
        Face::Pz => FaceBasis {
            n: DVec3::new( 0.0, 0.0, 1.0),
            u: DVec3::new( 1.0, 0.0, 0.0),
            v: DVec3::new( 0.0, 1.0, 0.0),
        },
        Face::Nz => FaceBasis {
            n: DVec3::new( 0.0, 0.0,-1.0),
            u: DVec3::new(-1.0, 0.0, 0.0),
            v: DVec3::new( 0.0, 1.0, 0.0),
        },
    }
}
```

For a chunk-local sample `(i, j)`:

* convert to chunk UV in `[0, 1]`
* convert to face UV in `[0, 1]`
* convert to signed face coords `(s, t)` in `[-1, 1]`
* build cube point `c = n + s*u + t*v`
* apply your cube-to-sphere warp to get unit direction
* sample **3D noise** in that direction or scaled planet-space position
* displace along the unit direction

Do **not** evaluate your terrain in 2D face UV space. The spherical signal should still come from 3D planet-space sampling, not 2D face-space noise. That remains the cleanest way to avoid seams and obvious scale distortion in the procedural field.

This section does not materially change under server-driven chunk management, pooled render state, or reusable packed staging buffers, because the planet math remains identical. The chunk’s render backend changes; its geometric domain does not.

## 4) Chunk keys and deterministic neighbor mapping across faces

Do not hardcode 24 edge cases by hand. Derive them once from the face bases and store a tiny lookup table.

```rust
#[derive(Clone, Copy, Debug)]
pub struct EdgeXform {
    pub neighbor_face: Face,
    pub neighbor_edge: Edge,
    pub flip: bool,
}

fn face_from_normal(n: DVec3) -> Face {
    match (n.x as i32, n.y as i32, n.z as i32) {
        ( 1, 0, 0) => Face::Px,
        (-1, 0, 0) => Face::Nx,
        ( 0, 1, 0) => Face::Py,
        ( 0,-1, 0) => Face::Ny,
        ( 0, 0, 1) => Face::Pz,
        ( 0, 0,-1) => Face::Nz,
        _ => unreachable!(),
    }
}

fn edge_param_to_neighbor(face: Face, edge: Edge, q: f64) -> (Face, Edge, f64) {
    let b = face_basis(face);

    let (s, t, outward) = match edge {
        Edge::NegU => (-1.0, q * 2.0 - 1.0, -b.u),
        Edge::PosU => ( 1.0, q * 2.0 - 1.0,  b.u),
        Edge::NegV => (q * 2.0 - 1.0, -1.0, -b.v),
        Edge::PosV => (q * 2.0 - 1.0,  1.0,  b.v),
    };

    let c = b.n + s * b.u + t * b.v;

    let nf = face_from_normal(outward);

    let nb = face_basis(nf);
    let s2 = c.dot(nb.u);
    let t2 = c.dot(nb.v);

    let eps = 1e-9;
    if (s2 + 1.0).abs() < eps {
        (nf, Edge::NegU, (t2 + 1.0) * 0.5)
    } else if (s2 - 1.0).abs() < eps {
        (nf, Edge::PosU, (t2 + 1.0) * 0.5)
    } else if (t2 + 1.0).abs() < eps {
        (nf, Edge::NegV, (s2 + 1.0) * 0.5)
    } else if (t2 - 1.0).abs() < eps {
        (nf, Edge::PosV, (s2 + 1.0) * 0.5)
    } else {
        unreachable!()
    }
}

fn build_edge_xform(face: Face, edge: Edge) -> EdgeXform {
    let (f0, e0, q0) = edge_param_to_neighbor(face, edge, 0.0);
    let (f1, e1, q1) = edge_param_to_neighbor(face, edge, 1.0);
    assert!(f0 == f1 && e0 == e1);

    EdgeXform {
        neighbor_face: f0,
        neighbor_edge: e0,
        flip: q1 < q0,
    }
}
```

Now same-LOD neighbor lookup is still trivial:

```rust
fn same_lod_neighbor(key: ChunkKey, edge: Edge, xf: EdgeXform) -> ChunkKey {
    let n = 1u32 << key.lod;

    let p = match edge {
        Edge::NegU | Edge::PosU => key.y,
        Edge::NegV | Edge::PosV => key.x,
    };

    let p2 = if xf.flip { (n - 1) - p } else { p };

    let (x2, y2) = match xf.neighbor_edge {
        Edge::NegU => (0,      p2),
        Edge::PosU => (n - 1,  p2),
        Edge::NegV => (p2,     0),
        Edge::PosV => (p2,     n - 1),
    };

    ChunkKey {
        face: xf.neighbor_face,
        lod: key.lod,
        x: x2,
        y: y2,
    }
}
```

This still gives you a deterministic cross-face neighbor key for every chunk edge at every LOD, with correct reversal handled by `flip`.

The FFI and reuse path make this even more important. The chunk key, neighbor graph, and surface class are the sole source of truth for stitching, neighbor LOD delta rules, horizon/frustum eligibility, pool compatibility, staging buffer compatibility, physics residency, and RID lifetime. There is still no scene-tree hierarchy to fall back on for adjacency or ownership.

## 5) Visible grid, border ring, and stitch variants

Use one canonical chunk topology:

* `QUADS_PER_EDGE = 32` to start
* visible vertices per edge = `33`
* sampled vertices per edge with border ring = `35`

So each chunk samples a `35 x 35` grid but only renders the inner `33 x 33` vertices. The extra ring still exists only so normals, tangents, and biome gradients can be computed from consistent data across chunk boundaries.

Precompute these index buffers once:

* `base_indices`
* 16 stitch variants for the visible mesh:

  * bit 0 = stitch NegU
  * bit 1 = stitch PosU
  * bit 2 = stitch NegV
  * bit 3 = stitch PosV

Only the **finer** chunk still stitches upward to the coarser neighbor. If you enforce `max LOD delta = 1`, the coarser chunk still never needs special handling.

This section now has three explicit compatibility implications:

* `vertex_count` is constant for all visible terrain chunks within a topology class,
* `index_count` is constant within each stitch class,
* `vertex_bytes`, `attribute_bytes`, and `index_bytes` are constant within each exact `SurfaceClassKey`.

That is why the render pool and the staging-buffer pool must be keyed per compatible class. If two chunks have the same visible grid size but different stitch masks, material contracts, or array format flags, you still may need different warm-path resources and different packed staging buffer sizes. Godot’s region-update APIs operate on existing surfaces and expect the byte payloads to match the surface layout they were created with. ([Godot Engine documentation][1])

This section still has the earlier performance role: because the chunk topology is canonical and reused, runtime never rebuilds topology definitions. It only switches stitch/index class, updates data against compatible surfaces, and fills already-sized staging buffers.

## 6) Visibility selection and LOD

Runtime LOD is still a cheap selector over precomputed chunk metadata, and the selection pipeline remains:

```text
1. Start from 6 roots.
2. Horizon-cull.
3. Frustum-cull survivors.
4. For each surviving chunk:
   - compute projected error in pixels
   - if error > split_threshold and lod < max_lod: split
   - else keep
5. Enforce max neighbor LOD delta = 1.
6. Build new active render set.
7. Build near-player active physics set.
8. Diff against previous active sets and apply RID changes.
```

Per chunk, still precompute:

* bounding sphere
* min/max height
* min/max radius
* angular extent / conservative angular radius
* geometric error
* prebuilt seam state
* optional asset density stats
* `surface_class`

### Horizon culling

Because the world is a sphere, frustum culling alone is still not enough. Godot’s built-in 3D optimization guidance covers frustum culling as the baseline engine-level visibility reduction; your planet runtime still needs its own pre-frustum stage for globe-scale occlusion. Horizon culling therefore stays in the Rust selector before frustum and LOD. ([Godot Engine documentation][4])

Start with the same conservative angular test based on the camera’s distance from the planet center and the chunk’s precomputed angular radius:

* let `d = |camera_pos_from_planet_center|`
* let `beta = acos(R_occ / d)` where `R_occ` is the occluding radius, usually `planet_radius + safety_margin`
* let `theta = angle(camera_dir_from_center, chunk_bound_center_dir)`
* keep chunk if `theta <= beta + chunk_angular_radius`

That is still the simple conservative version. It is fast, cheap, and easy to vectorize over many chunks.

Then add a second refinement later if needed:

* use a tighter per-chunk bound than a pure sphere,
* or precompute a tighter occludee-style horizon bound for each chunk.

Do not overcomplicate this on day one. A good conservative horizon test still buys you more than many micro-optimizations elsewhere.

### Frustum culling

After horizon culling, frustum-cull the survivors. Start with bounding spheres because they are trivial and cheap. If profiling shows too many partially visible or false-positive chunks survive, add an optional OBB path for higher LOD chunks only.

### LOD error

A practical projected-error test is still:

```text
projected_error_px = geometric_error_world * projection_scale / distance_to_camera
```

Use hysteresis:

* split at `> 8 px`
* merge at `< 4 px`

That still keeps the active set stable.

### Physics residency

Render LOD and physics residency should still be selected independently:

* render: based on horizon/frustum/error,
* physics: based on player proximity, gameplay relevance, and collision budget.

Do **not** keep physics on every visible render chunk. Near-player only. Godot’s physics docs still describe `ConcavePolygonShape3D` as the slowest 3D collision shape, while `HeightMapShape3D` is faster than concave but still slower than primitives. ([Godot Engine documentation][5])

### Commit budgeting

This section still gets one explicit runtime budget:

* after active-set diffing, do not necessarily commit every spawn/despawn immediately,
* maintain a priority queue by screen impact and proximity,
* apply a bounded number of RID create/update/free operations per frame.

Now add a second hot-path budget:

* also cap how many **staging buffer fills** and **region uploads** you allow per frame,
* since even when pooling eliminates allocator churn, buffer packing and upload bandwidth can still spike.

That keeps the steady-state warm path from becoming the new source of frame spikes.

## 7) Mesh generation pipeline

This is the updated full pipeline.

### Stage A: build metadata tree

At load:

```text
for each face:
    for lod in 0..=MAX_LOD:
        for each chunk on that face/lod:
            - compute same-lod neighbors
            - compute bounds skeleton
            - compute angular radius / horizon metadata
            - compute surface class
            - allocate metadata slot
```

### Stage B: sample scalar fields

For each chunk you want resident:

```text
for each sample in (QUADS_PER_EDGE + 3)^2:
    1. map sample to face-space (with border ring)
    2. cube -> sphere projection
    3. evaluate height from 3D noise
    4. evaluate biome masks / moisture / temperature / rockness
    5. store height + masks in chunk-local sample grid
```

Keep these samples in a POD Rust buffer:

```rust
pub struct Sample {
    pub unit_dir: DVec3,
    pub height: f32,
    pub biome0: f32,
    pub biome1: f32,
    pub slope_hint: f32,
}
```

### Stage C: derive mesh buffers

From the sample grid:

```text
1. build visible positions
2. compute normals from the sampled height field
3. compute tangents if your material needs them
4. compute color/mask channels
5. choose stitch index buffer from current edge state
6. emit CpuMeshBuffers
```

Normal computation should still be from the **global field**, not just local triangle winding. The border ring is still what makes adjacent chunks agree.

### Stage D: optionally prepack Rust byte regions

This stage remains a first-class optimization stage:

```text
1. read the chunk's surface class / format mask
2. determine vertex / attribute / index strides
3. pack positions into the vertex region format expected by the surface
4. pack normals/tangents/uvs/colors into the attribute region format expected by the surface
5. pack indices into the index region format expected by the surface
6. store PackedMeshRegions beside CpuMeshBuffers
```

The important rule still holds: **do not assume one arbitrary interleaved blob format**. Godot exposes separate update calls for vertex, attribute, skin, and index regions and exposes stride queries for those regions, so byte packing should be built around the actual surface format class you created, not around an invented interleaved layout. The docs explicitly state that vertex positions are stored consecutively and are **not** interleaved with normals and tangents in the vertex buffer. ([Godot Engine documentation][1])

### Stage E: fill reusable Godot staging buffers

This is the new FFI-focused stage:

```text
1. fetch a compatible pooled render entry or per-class staging template
2. ensure its PackedByteArray buffers are resized to the required byte counts
3. fill vertex_region in place via as_mut_slice()
4. fill attribute_region in place via as_mut_slice()
5. fill index_region in place via as_mut_slice()
6. avoid constructing fresh PackedByteArrays from transient Vec<u8> in the hot path
```

This is the key change from the previous revision. gdext packed arrays are contiguous and explicitly expose `as_mut_slice()` for fast encoding and `resize()` for sizing. The docs also show `From<Vec<T>>`, but they do not promise that the resulting Godot array aliases the Rust allocation instead of copying into Godot-managed storage. That is why the hot path should fill **reusable Godot-owned buffers** in place rather than assume general zero-copy adoption of arbitrary Rust vectors. ([Godot Rust][3])

### Stage F: build server-commit payloads

This stage still exists, but is now more explicit:

```text
1. package CpuMeshBuffers
2. package PackedMeshRegions for optional cold-path packing or diagnostics
3. package GdPackedStaging handles if the chunk is pool/update compatible
4. package collision payload, if this chunk is physics-eligible
5. package asset instance transforms
6. package final chunk transform relative to current render origin
7. package RID lifecycle command
```

Workers still do **not** produce “nodes to add.” They produce plain Rust payloads plus a desired server state transition and, when possible, prepared staging contents for in-place updates.

### Stage G: create or update server objects

On the commit side:

```text
1. try to reuse a compatible pooled render entry
2. if reuse succeeds:
   - update vertex region
   - update attribute region
   - update index region if needed
   - set base/scenario/transform/material state
3. if reuse fails:
   - create mesh resource
   - create render instance RID
   - attach render instance to scenario
4. create/update physics shape/body RID if needed
5. attach body to physics space if needed
6. create/update chunk-owned asset multimeshes / instances
7. move inactive state into pools instead of immediately freeing when appropriate
8. free obsolete RIDs only when pool caps are exceeded or compatibility rules fail
```

Because your planets are still static, most heavy work still happens at load or during initial residency build. Runtime should still be dominated by set selection and conservative RID churn, but now the steady-state hot path should prefer:

* pool reuse over create/free,
* region updates over full mesh rebuilds,
* reusable Godot staging buffers over transient conversion objects,
* and reusable Rust-side scratch arenas over fresh large allocations.

Godot exposes the relevant region update APIs on both `RenderingServer` and `ArrayMesh`, while gdext packed arrays are designed for reusable contiguous mutation. ([Godot Engine documentation][1])

## 8) Server-side render and collision commit pattern

Use Godot’s procedural mesh path to build terrain mesh resources, then attach them to `RenderingServer` instances instead of wrapping them in `MeshInstance3D` nodes. This remains the core render path. `ArrayMesh` supports creation from arrays and surface region updates, while `RenderingServer` supports lower-level surface creation plus region updates directly on mesh RIDs. Visible 3D objects still require both a base resource and a scenario-attached instance. ([Godot Engine documentation][6])

A concrete commit model is now:

```rust
enum RenderCommitMode {
    ColdCreate,
    WarmReuseRegionUpdate,
}

fn create_render_chunk_cold(...) -> ChunkRenderState {
    // 1. Build mesh resource.
    //    Option A: ArrayMesh.add_surface_from_arrays()
    //    Option B: RenderingServer.mesh_add_surface_from_arrays()
    // 2. Create render instance RID.
    // 3. instance_set_base(instance_rid, mesh_rid)
    // 4. instance_set_scenario(instance_rid, scenario_rid)
    // 5. instance_set_transform(instance_rid, transform)
    // 6. return both RIDs
}

fn update_render_chunk_warm(...) {
    // Preconditions:
    // - pooled entry surface class matches
    // - vertex/index capacities match expected class
    // - material/shader contract matches
    //
    // 1. mesh_surface_update_vertex_region(...)
    // 2. mesh_surface_update_attribute_region(...)
    // 3. mesh_surface_update_index_region(...) if needed
    // 4. instance_set_base(...) if rebinding is required
    // 5. instance_set_scenario(...)
    // 6. instance_set_transform(...)
}
```

The two-path rule remains explicit.

### Cold path

Use this when:

* no compatible pooled entry exists,
* the chunk class changed incompatibly,
* you are warming the cache at startup,
* or you are deliberately rebuilding a surface.

This path should stay simple and correct. It is still acceptable to use `ArrayMesh.add_surface_from_arrays()` or `RenderingServer.mesh_add_surface_from_arrays()` here because this is not the hot steady-state path. Godot’s docs explicitly support both the arrays-based creation path and the region-update path. ([Godot Engine documentation][6])

### Warm path

Use this when:

* a compatible pooled mesh/instance pair is available,
* the surface format matches,
* the vertex count matches,
* the index count or stitch class is compatible with the reused slot.

This path should prefer byte-region updates and reusable Godot staging buffers. The region update APIs take `PackedByteArray`, and gdext packed arrays are mutable in place through `as_mut_slice()`. That combination is what should define your hot path, not speculative zero-copy wrapping of arbitrary `Vec<u8>` allocations. ([Godot Engine documentation][1])

### FFI boundary rule

This is the new explicit backend rule:

* treat the Rust↔Godot boundary as a place where copies may occur unless the docs clearly promise otherwise,
* do **not** base the runtime architecture on undocumented zero-copy ownership transfer,
* keep hot-path packed buffers Godot-owned and reusable,
* fill them in place.

The docs show `PackedArray::from(&[T])`, `PackedArray::from(Vec<T>)`, `resize()`, `as_slice()`, `as_mut_slice()`, and also note value semantics with copy-on-write. They do **not** document a general “adopt this Rust pointer as my internal storage without copy” guarantee for `PackedByteArray`. ([Godot Rust][3])

### Pool policy

Pool by at least:

* `format_mask`
* `vertex_count`
* `index_count`
* `stitch_mask` or reduced stitch/index class
* `material_class`

And now also keep with the pool entry:

* pre-sized or readily resizable `PackedByteArray` staging buffers,
* cached byte-count expectations for each region,
* optional scratch metadata for partial updates.

That is the minimum reliable compatibility contract for reuse.

### Transform and scenario rebinding

Reused render instances still need their transform and scenario state refreshed when they are reactivated. Godot documents `instance_set_scenario()` and `instance_set_transform()` for this exact purpose. ([Godot Engine documentation][1])

### Collision path

The collision side still remains separate and conservative:

```rust
fn create_or_update_physics_chunk(...) {
    // 1. Create or fetch pooled body/shape if your policy allows it.
    // 2. Fill or replace shape data.
    // 3. body_add_shape(...)
    // 4. body_set_state(... transform ...)
    // 5. body_set_space(...)
}
```

I would still be more conservative about pooling physics than rendering. Render pooling is a clearer win. Physics pooling is situational because shape-data replacement can still be costly, and the broad/narrow-phase cost of the resulting shapes still dominates if you keep too many complex terrain colliders alive. Godot’s docs still describe `ConcavePolygonShape3D` as the slowest 3D collision shape and `HeightMapShape3D` as cheaper than concave but slower than primitives. ([Godot Engine documentation][5])

### Collision shape policy

Because your terrain is still static and chunked, collision should still be much more conservative than rendering:

* first choice for correctness: coarse concave collision, near-player only,
* experimental option: local chunk-frame heightmap collision if your patch representation fits it cleanly,
* never use render-resolution collision as the default,
* do not aggressively pool collision unless profiling shows shape/body allocation churn is a real issue.

## 9) Threading model in godot-rust

Use this model:

* **workers**: pure Rust tasks, `f64` math, no scene-tree mutation,
* **commit side**: server-oriented object/resource updates,
* **handoff**: deterministic worker queue plus ordered result drain.

Example runtime commands:

```rust
pub enum PlanetCommand {
    CreateOrUpdateRenderChunk {
        key: ChunkKey,
        payload: ChunkPayload,
        transform: Transform3D,
    },
    RemoveRenderChunk {
        key: ChunkKey,
    },
    CreateOrUpdatePhysicsChunk {
        key: ChunkKey,
        collider_vertices: Vec<[f32; 3]>,
        collider_indices: Vec<i32>,
        transform: Transform3D,
    },
    RemovePhysicsChunk {
        key: ChunkKey,
    },
    UpdateAssets {
        key: ChunkKey,
        instances: Vec<AssetInstance>,
    },
}
```

Godot’s thread-safety docs still explicitly say the global-scope singletons are thread-safe by default, and that accessing servers from threads is supported once the relevant project settings are enabled. They also explicitly say this makes servers suitable for code that creates and controls very large numbers of instances directly, while the active scene tree remains the wrong place for concurrent mutation. ([Godot Engine documentation][7])

The current codebase intentionally ships only one operating mode.

### Mode A: safer default

* workers generate plain Rust buffers and optional Rust-packed regions,
* one commit lane performs all Godot server calls in a controlled batch,
* that same lane owns the Godot staging buffers and fills them via `as_mut_slice()`,
* no worker ever touches the scene tree.

This is still the easiest path to debug and usually good enough.

Now add the warm-path synchronization rules:

* workers may only use the warm-path region updates against **prevalidated compatible pool classes**,
* the pool manager itself must remain synchronized and deterministic,
* staging buffers may never be shared mutably across simultaneous chunk commits,
* each worker should therefore use per-worker Rust scratch buffers plus a single commit lane for Godot staging, or its own isolated Godot staging set for the exact surface classes it handles.

That prevents racey “grab some RID and some packed array and hope it fits” behavior.

### Worker allocation policy

This is the new explicit FFI-side performance rule:

* do not allocate fresh large `Vec`s for every worker job if you can avoid it,
* keep per-worker reusable `CpuMeshBuffers`,
* keep per-worker reusable byte-packing scratch buffers,
* reset and refill those buffers rather than reconstructing them every update,
* only convert into Godot-owned staging arrays at the final commit boundary.

This part is engineering guidance rather than an explicit Godot API rule, but it is the correct consequence of the gdext packed-array API and the absence of a documented zero-copy ownership transfer path.

Current implementation status:

* persistent Rust worker threads pre-spawn at runtime startup,
* work submission uses a mutex/condvar queue with deterministic sequence numbers,
* worker results are sorted back into request order before commit-lane ownership changes,
* warm-path compatibility checks and all Godot staging/server writes remain commit-lane only,
* per-worker sample/mesh/pack scratch is reused and logged through queue/wait/scratch metrics,
* Mode B is not implemented in shipping code.

## 10) Precision strategy

Use:

* `f64` for all planet-space math in Rust,
* `f32` only for GPU upload and local chunk buffers,
* camera-relative or render-origin-relative positions when filling render buffers,
* Godot large world coordinates only if you truly need engine-wide precision.

Godot’s large-world docs state that large world coordinates increase the precision of floating-point computations within the engine, and Godot’s optimization guidance also points to origin-centering / shifting techniques when large-world precision is otherwise a problem. ([Godot Engine documentation][8])

The earlier precision logic remains the same. The new staging-buffer path changes one operational detail: pooled render entries and their packed staging buffers should not store long-lived absolute planet-space transforms as their source of truth. Their true position still lives in stable Rust planet space, and the pooled render instance gets rebound each activation with the current render-relative transform.

Do not let each subsystem invent its own local origin rules. The chunk payload should still be generated in stable planet space, then converted once into render-relative and physics-relative transforms before commit.

## 11) Seam handling rules

Use all three of these together:

1. **Global border sampling**
   every shared border vertex must come from the same face-space coordinate rule

2. **Border ring for shading**
   sample one hidden ring outside the visible chunk

3. **Fine-to-coarse stitch indices**
   only finer chunks stitch

That still removes:

* geometric cracks
* face-edge mismatches
* normal seams

Do not use skirts unless you need a temporary shortcut.

There are now two compatibility implications:

* seam state must be part of the **surface compatibility classification**, and
* the packed byte sizes for the index region must also match that class.

If a warm-path pooled slot assumes one stitch/index class and the incoming chunk needs another incompatible class, you either:

* switch to another pool keyed for that class,
* or fall back to cold creation / rebuild.

Do not try to jam incompatible stitch topologies into a reused surface just because the vertex count matches.

## 12) Asset placement

Use chunk-local deterministic placement.

For each chunk, after terrain is sampled:

```text
1. divide chunk into placement cells
2. hash (planet_seed, chunk_key, cell_id, family_id)
3. generate candidate point(s)
4. project candidate to terrain
5. reject by:
   - biome
   - slope
   - curvature
   - altitude
   - mask textures
   - exclusion radius
6. store accepted transforms in chunk payload
```

Keep assets attached to the chunk that owns them. That still gives you deterministic streaming and simple invalidation.

For repeated assets, still use **MultiMesh**, but keep the earlier warning: `MultiMesh` is fast because it can draw many instances with low overhead, but individual instances inside one multimesh are not culled independently, so widely separated instances should be split into multiple spatially compact groups. Godot’s docs make this tradeoff explicit. ([Godot Engine documentation][1])

So the recommendation still stands, just with server ownership instead of scene ownership:

* **not one multimesh for the whole planet**
* one multimesh per `(chunk_group, asset_family)` or another spatially compact grouping
* near: optional higher-quality handling for important/interactable assets
* mid: multimesh
* far: impostor, billboard, or none

Because chunk visibility is now server-driven, asset residency should still follow the chunk lifecycle rather than node visibility callbacks.

Now add one more performance rule:

* asset multimeshes may have their own lightweight pool or reuse path at the chunk-group level,
* but only after the terrain chunk and staging-buffer reuse path is stable,
* since terrain chunk reuse is still the primary win.

## 13) Default numbers I would start with

Use these first:

```text
MAX_LOD                         = 9 or 10
QUADS_PER_EDGE                  = 32
SAMPLED_EDGE                    = 35   // 33 visible + 2 border
SPLIT_THRESHOLD_PX              = 8
MERGE_THRESHOLD_PX              = 4
HORIZON_SAFETY_MARGIN           = small positive value to avoid over-culling
COLLISION_LOD_RADIUS            = near-player only
ASSET_CELL_GRID                 = 8x8 per chunk
COMMIT_BUDGET_PER_FRAME         = cap RID churn to avoid spikes
UPLOAD_BUDGET_PER_FRAME         = cap staging fills + region uploads
POOL_WATERMARK_PER_CLASS        = small bounded free-list per surface class
PHYSICS_POOL_WATERMARK          = lower than render pool watermark
WORKER_SCRATCH_COUNT            = one reusable scratch set per worker
```

This is still a good first target because:

* 32 quads per chunk keeps index buffers small and reusable
* 33 visible vertices is enough for stable normals/materials
* the border ring solves most seam/shading issues immediately
* 16 stitch index buffers is manageable

The new additions are:

* `UPLOAD_BUDGET_PER_FRAME`
* `WORKER_SCRATCH_COUNT`

Those join the earlier pool watermarks as back-pressure controls. If inactive pooled objects exceed watermarks, free the extras. If upload work exceeds the budget, defer lower-priority warm updates rather than letting one frame absorb all region fills and transfers.

`COMMIT_BUDGET_PER_FRAME` is still essential. Even with server-managed chunks, pooling, and reusable packed staging buffers, you still do not want unbounded update churn if the camera moves violently.

Only increase `QUADS_PER_EDGE` after you have profiling data.

## 14) Build order

Implement in this order:

1. face basis + chunk key + neighbor mapping
2. default modified / spherified cube projection
3. cube-face sample coordinates
4. 3D noise displacement on sphere
5. border ring + normal generation
6. base chunk mesh generation
7. same-LOD neighbor validation across face edges
8. stitch index buffers
9. metadata tree + bounds + angular radius + surface class
10. horizon culling
11. frustum culling
12. projected-error LOD selection
13. render/physics active-set separation
14. cold server-side render commit path
15. warm pooled render path
16. Rust byte-region packing helpers
17. reusable Godot packed staging buffers
18. in-place staging fills via resize() + as_mut_slice()
19. byte-region vertex / attribute / index updates
20. server-side physics commit path
21. chunk-group asset multimesh path
22. worker scratch reuse
23. commit budgeting / upload budgeting / pool watermarks / hysteresis / caching polish

That order still lets you validate the hard math early before you spend time on backend plumbing, but it now cleanly separates:

* the cold creation path,
* the warm pooled reuse path,
* the Rust packing path,
* the Godot staging-buffer path,
* and the bounded-churn runtime controls.

That separation matters because you want a correct engine before you want a maximally optimized one.

## 15) One important refinement

Keep the **cube-to-sphere projection as a strategy object**, with the modified / spherified cube mapping as the default implementation:

```rust
pub trait CubeProjection {
    fn project(&self, cube_point: DVec3) -> DVec3;
}

pub struct SpherifiedCubeProjection;

impl CubeProjection for SpherifiedCubeProjection {
    fn project(&self, cube_point: DVec3) -> DVec3 {
        let x = cube_point.x;
        let y = cube_point.y;
        let z = cube_point.z;

        let x2 = x * x;
        let y2 = y * y;
        let z2 = z * z;

        let sx = x * (1.0 - y2 * 0.5 - z2 * 0.5 + (y2 * z2) / 3.0).sqrt();
        let sy = y * (1.0 - z2 * 0.5 - x2 * 0.5 + (z2 * x2) / 3.0).sqrt();
        let sz = z * (1.0 - x2 * 0.5 - y2 * 0.5 + (x2 * y2) / 3.0).sqrt();

        DVec3::new(sx, sy, sz).normalize()
    }
}
```

That still lets you:

* start with a lower-distortion projection by default,
* later swap in another projection for experiments,
* keep every other system unchanged.

And keep the visibility strategy layer:

```rust
pub trait ChunkVisibilityStrategy {
    fn horizon_visible(&self, camera: &CameraState, meta: &ChunkMeta) -> bool;
    fn frustum_visible(&self, camera: &CameraState, meta: &ChunkMeta) -> bool;
    fn screen_error_px(&self, camera: &CameraState, meta: &ChunkMeta) -> f32;
}
```

And keep the backend strategy layer, but now make the FFI path explicit:

```rust
pub trait ChunkRenderBackend {
    fn cold_create(&mut self, class: &SurfaceClassKey, payload: &ChunkPayload, xform: Transform3D);
    fn warm_reuse_update(&mut self, class: &SurfaceClassKey, payload: &ChunkPayload, xform: Transform3D);
    fn deactivate(&mut self, key: ChunkKey);
}
```

Add one more hot-path policy layer conceptually, even if you do not formalize it as a trait immediately:

```rust
pub trait PackedStagingPolicy {
    fn acquire_staging(&mut self, class: &SurfaceClassKey) -> &mut GdPackedStaging;
    fn fill_staging_from_payload(&mut self, class: &SurfaceClassKey, payload: &ChunkPayload);
}
```

That gives you the same flexibility on the buffer side that the projection strategy gives you on the geometry side:

* start with conservative arrays-based cold creation,
* later tune region-update packing,
* later vary per-class staging ownership,
* keep the runtime testable and data-oriented.

That is now the final shape I would use:

* **quadsphere with modified / spherified cube projection by default**
* **6 face quadtrees**
* **fixed grid chunks**
* **3D noise in planet space**
* **horizon culling before frustum and LOD**
* **edge stitching with neighbor delta limited to 1**
* **global-space normals**
* **Rust worker threads for generation**
* **server-managed render chunks through `RenderingServer`**
* **per-class chunk render pools**
* **cold creation path plus warm byte-region update path**
* **reusable Godot-owned `PackedByteArray` staging buffers**
* **no runtime dependence on undocumented zero-copy adoption of Rust buffers**
* **reusable Rust-side scratch buffers for worker jobs**
* **server-managed near-player physics chunks through `PhysicsServer3D`**
* **chunk-group multimesh assets, also server-managed where beneficial**
* **render-origin-relative transforms**
* **bounded RID churn, upload budgets, and pool watermarks**

The final caveat still stays explicit: moving chunks to the servers, adding pooling, and tightening the FFI boundary are the right architectural direction for a very large planet renderer, but they still do not reduce the importance of the visibility stack. Horizon culling, active-set control, collision residency, bounded commit churn, and bounded upload churn will still move the needle more than almost any single low-level API trick. Godot’s own docs are explicit that server use helps when the scene system is the bottleneck, not as a universal free speedup. ([Godot Engine documentation][1])

The next useful artifact is the Rust skeleton for this exact revised runtime: `PlanetRoot`, `PlanetRuntime` with per-class pools and staging buffers, the visibility pass, the warm/cold render commit paths, and the reusable worker scratch model.

[1]: https://docs.godotengine.org/en/stable/classes/class_renderingserver.html "RenderingServer — Godot Engine (stable) documentation in English"
[2]: https://docs.godotengine.org/en/stable/tutorials/performance/using_servers.html?utm_source=chatgpt.com "Optimization using Servers - Godot Docs"
[3]: https://godot-rust.github.io/book/godot-api/builtins.html "Built-in types - The godot-rust book"
[4]: https://docs.godotengine.org/en/latest/tutorials/performance/optimizing_3d_performance.html?utm_source=chatgpt.com "Optimizing 3D performance - Godot Docs"
[5]: https://docs.godotengine.org/en/stable/classes/class_concavepolygonshape3d.html?utm_source=chatgpt.com "ConcavePolygonShape3D - Godot Docs"
[6]: https://docs.godotengine.org/en/stable/classes/class_arraymesh.html "ArrayMesh — Godot Engine (stable) documentation in English"
[7]: https://docs.godotengine.org/en/latest/tutorials/performance/thread_safe_apis.html?utm_source=chatgpt.com "Thread-safe APIs - Godot Docs"
[8]: https://docs.godotengine.org/en/stable/tutorials/physics/large_world_coordinates.html?utm_source=chatgpt.com "Large world coordinates - Godot Docs"
