# Phase 03 - Face Basis and Chunk-Local Coordinates

## Goal

Restore full geometric detail for face basis definitions and chunk-local coordinate conversion, including concrete API-level guidance.

## Face Basis

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

## Chunk-Local Sample Pipeline

For each chunk-local sample `(i, j)`:

1. Convert to chunk UV in `[0, 1]`.
2. Convert to face UV in `[0, 1]`.
3. Convert to signed face coords `(s, t)` in `[-1, 1]`.
4. Build cube point: `c = n + s*u + t*v`.
5. Apply cube-to-sphere warp to get unit direction.
6. Sample 3D noise in direction/scaled planet-space.
7. Displace along unit direction.

Do not evaluate terrain in 2D face UV space. The spherical signal should come from 3D planet-space sampling, not 2D face-space noise. This remains the cleanest way to avoid seams and scale distortion artifacts.

Server-driven chunk management, pooled render state, and reusable packed staging buffers do not change this geometry domain.

## Checklist

- [ ] Implement deterministic face basis table for all 6 faces.
- [ ] Keep sampling pipeline identical across faces and LODs.
- [ ] Keep terrain signal in 3D planet-space domain.
- [ ] Validate basis handedness and unit-direction normalization.
- [ ] Record any epsilon policy used for edge/corner stability.

## Prerequisites

- [ ] Phase 02 data model in place (`Face`, `Edge`, `ChunkKey`, metadata fields).

## Ordered Build Steps

1. [ ] Implement face basis table exactly.
2. [ ] Implement local sample mapping path `(i,j) -> chunk UV -> face UV -> (s,t) -> cube point`.
3. [ ] Implement projection call site using strategy default.
4. [ ] Implement 3D field sampling and displacement.

## Validation and Test Gates

- [ ] Basis orthogonality and handedness assertions pass.
- [ ] Unit direction normalization checks pass within epsilon.
- [ ] Cross-face shared border sample continuity test passes.

## Definition of Done

- [ ] Sampling math is deterministic across faces and LODs.
- [ ] 2D face-space-only noise path is not used for primary terrain signal.

## Test Record (Fill In)

- [ ] Date:
- [ ] Result summary:
- [ ] Seam continuity notes:
- [ ] Follow-up actions:

## References

- [Math and geometry constraints in local phase docs](./README.md)
