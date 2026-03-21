# Phase 04 - Chunk Keys and Deterministic Neighbor Mapping Across Faces

## Goal

Restore the full deterministic cross-face neighbor mapping approach, including basis-derived transforms and concrete code.

## Mapping Strategy

Do not hardcode 24 edge cases. Derive directed edge transforms once from face bases and store a compact lookup table.

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

Same-LOD neighbor lookup then stays trivial:

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

This graph is a core source of truth for stitching, neighbor LOD delta enforcement, horizon/frustum eligibility, pool compatibility, staging compatibility, physics residency, and RID lifetime.

## Implementation Notes

Implemented in:

- `rust/src/topology.rs`
- `rust/src/runtime.rs`
- `rust/src/lib.rs`

What is now live in code:

- A cached basis-derived `EdgeTransform` table for all 24 directed face-edge pairs, built from the Phase 03 face bases instead of hand-authored edge-case mappings.
- Deterministic same-LOD neighbor lookup that handles both same-face adjacency and cross-face seam traversal from the shared transform table.
- `ChunkMeta::new(...)` and `PlanetRuntime::register_chunk_meta(...)` now normalize `neighbors.same_lod` from the Phase 04 topology graph so registered metadata cannot drift from the authoritative adjacency rules.
- Runtime debug accessors/logging expose the transform-table size and current default max LOD for headless validation.

API constraints rechecked before implementation on 2026-03-21:

- Godot `World3D` docs were rechecked to keep scenario/space RID ownership world-side while this phase stayed Rust-side for topology and metadata adjacency.
- Godot headless/server documentation was rechecked so validation continues to use the local engine binary with `--headless`.
- godot-rust built-in type docs were rechecked to confirm this phase adds no new FFI or packed-array assumptions beyond the copy-on-write packed-array rules already documented in earlier phases.

Deviation note:

- The original checklist asked for same-LOD neighbors to be stored in metadata for all chunks immediately. The implementation keeps the transform table and same-LOD lookup authoritative now, and stores neighbors into every `ChunkMeta` that is registered, while leaving the full metadata-all-LODs allocation pass to the later metadata-tree phase where bounds, angular radius, and surface-class data are built together.

## Checklist

- [x] Build basis-derived edge transform table for all face-edge pairs.
- [x] Validate deterministic mapping and reversal behavior.
- [x] Store same-LOD neighbors in registered metadata through authoritative topology lookup.
- [x] Remove any manual edge-case mapping logic from runtime paths.
- [x] Use this graph as authoritative adjacency source across systems that register/query chunk neighbors today.

## Prerequisites

- [x] Phase 03 face basis and sampling math completed.

## Ordered Build Steps

1. [x] Implement basis-derived edge transform derivation.
2. [x] Build and cache all directed edge transform entries.
3. [x] Implement same-LOD neighbor lookup using transform table.
4. [x] Persist same-LOD neighbor keys into registered chunk metadata.

## Validation and Test Gates

- [x] 24 directed edge mappings exist and are deterministic.
- [x] Round-trip neighbor traversal tests pass.
- [x] Full-grid stress test passes up to configured max LOD (`DEFAULT_MAX_LOD = 10`) in Rust unit tests.

## Definition of Done

- [x] No hand-authored edge-case mapping branches remain in runtime path.
- [x] Neighbor graph is stable and used by runtime metadata registration and same-LOD adjacency queries.

## Test Record (Fill In)

- [x] Date: 2026-03-21
- [x] Result summary: Added `rust/src/topology.rs` with a cached basis-derived edge-transform table, deterministic same-LOD neighbor lookup, and full-grid round-trip tests through the default max LOD. Wired `ChunkMeta` construction and `PlanetRuntime` metadata registration so stored neighbor data is normalized from the topology graph.
- [x] Round-trip determinism notes: `cargo test` validates all 24 directed edge transforms, verifies reverse traversal across shared edges, rejects invalid chunk keys, and performs a full-grid same-LOD round-trip sweep through `DEFAULT_MAX_LOD = 10`.
- [x] Follow-up actions: Reuse the same topology helpers in the later metadata-tree and stitch-variant phases so bounds, stitch masks, and neighbor LOD normalization all consume one adjacency source of truth.

## References

- [Phase 03 face basis assumptions](phase-03-chunk-keys-and-neighbor-mapping.md)
- [World3D - Godot docs](https://docs.godotengine.org/en/4.4/classes/class_world3d.html)
- [Compiling for macOS / headless mode - Godot docs (stable)](https://docs.godotengine.org/en/stable/engine_details/development/compiling/compiling_for_macos.html#running-a-headless-server-build)
- [godot-rust built-in types (packed arrays)](https://godot-rust.github.io/book/godot-api/builtins.html)
