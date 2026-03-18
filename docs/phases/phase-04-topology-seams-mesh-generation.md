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

## Checklist

- [ ] Build basis-derived edge transform table for all face-edge pairs.
- [ ] Validate deterministic mapping and reversal behavior.
- [ ] Store same-LOD neighbors in metadata for all chunks.
- [ ] Remove any manual edge-case mapping logic from runtime paths.
- [ ] Use this graph as authoritative adjacency source across systems.

## Prerequisites

- [ ] Phase 03 face basis and sampling math completed.

## Ordered Build Steps

1. [ ] Implement basis-derived edge transform derivation.
2. [ ] Build and cache all directed edge transform entries.
3. [ ] Implement same-LOD neighbor lookup using transform table.
4. [ ] Persist same-LOD neighbor keys into chunk metadata.

## Validation and Test Gates

- [ ] 24 directed edge mappings exist and are deterministic.
- [ ] Round-trip neighbor traversal tests pass.
- [ ] Full-grid stress test passes up to configured max LOD.

## Definition of Done

- [ ] No hand-authored edge-case mapping branches remain in runtime path.
- [ ] Neighbor graph is stable and used by seam/LOD/pool compatibility logic.

## Test Record (Fill In)

- [ ] Date:
- [ ] Result summary:
- [ ] Round-trip determinism notes:
- [ ] Follow-up actions:

## References

- [Phase 03 face basis assumptions](phase-03-chunk-keys-and-neighbor-mapping.md)
