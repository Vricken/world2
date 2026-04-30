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

## Implementation Notes

Implemented in:

- `rust/src/geometry.rs`
- `rust/src/lib.rs`

What is now live in code:

- Deterministic face-basis lookup for all 6 faces using the documented right-handed `(n, u, v)` convention.
- Chunk-local sample mapping helpers for `(i, j) -> chunk UV -> face UV -> (s, t) -> cube point`.
- A swappable cube projection enum with spherified cube as the default strategy and normalized-cube retained as an explicit fallback.
- Deterministic 3D planet-space displacement sampling that operates on unit directions and base planet-space positions instead of 2D face-space noise.
- Shared-edge continuity tests that compare border samples across every directed face-edge pair at the root LOD.

API constraints verified before implementation on 2026-03-21:

- Godot `World3D` docs were checked to confirm that visual scenario and physics space RIDs remain world-owned, so this phase kept chunk identity in Rust and left scene-tree shape unchanged.
- Godot headless/server documentation was checked to confirm `--headless` is the supported way to run the local engine binary for validation without a window.
- godot-rust built-in type docs/source were rechecked to keep the Phase 02 packed-array copy-on-write assumption explicit while Phase 03 geometry stayed entirely on the Rust side of the FFI boundary.

Current epsilon policy:

- Snap face-UV derived coordinates within `1e-12` of `0`, `1`, or `-1` to the exact boundary value before edge/corner comparisons.
- Require projected unit directions to remain within `1e-12` of length `1.0` in tests.

Deviation note:

- The phase now uses a deterministic analytic 3D field as the default displacement signal. The current field is parameterized in `TerrainFieldSettings` and layers continent, hill, mountain ridge, and detail signals in 3D planet space without adding a noise crate.
- The same `RuntimeConfig::terrain_settings()` source feeds metadata bounds, render payload workers, test sampling, and asset placement. This avoids divergent terrain math between selection, rendering, collision derivation, and placement filters.

## Checklist

- [x] Implement deterministic face basis table for all 6 faces.
- [x] Keep sampling pipeline identical across faces and LODs.
- [x] Keep terrain signal in 3D planet-space domain.
- [x] Validate basis handedness and unit-direction normalization.
- [x] Record epsilon policy used for edge/corner stability.

## Prerequisites

- [x] Phase 02 data model in place (`Face`, `Edge`, `ChunkKey`, metadata fields).

## Ordered Build Steps

1. [x] Implement face basis table exactly.
2. [x] Implement local sample mapping path `(i,j) -> chunk UV -> face UV -> (s,t) -> cube point`.
3. [x] Implement projection call site using strategy default.
4. [x] Implement 3D field sampling and displacement.

## Validation and Test Gates

- [x] Basis orthogonality and handedness assertions pass.
- [x] Unit direction normalization checks pass within epsilon.
- [x] Cross-face shared border sample continuity test passes.

## Definition of Done

- [x] Sampling math is deterministic across faces and LODs.
- [x] 2D face-space-only noise path is not used for primary terrain signal.

## Test Record (Fill In)

- [x] Date: 2026-03-21
- [x] Result summary: Added `rust/src/geometry.rs` with deterministic face bases, chunk-local sample mapping helpers, a swappable cube projection enum with spherified default, and a seam-safe 3D planet-space displacement sampler. Exported the new module from `rust/src/lib.rs`.
- [x] Seam continuity notes: `cargo test` passed shared-border continuity checks across every directed face-edge pair at root LOD using the default spherified projection and 3D displacement field.
- [x] Follow-up actions: Reuse the phase-03 face basis and edge continuity math in Phase 04 to derive the compact cross-face neighbor transform table instead of hardcoding face-edge cases.

## References

- [Math and geometry constraints in local phase docs](./README.md)
- [World3D - Godot docs](https://docs.godotengine.org/en/4.4/classes/class_world3d.html)
- [Compiling for macOS / headless mode - Godot docs (stable)](https://docs.godotengine.org/en/stable/engine_details/development/compiling/compiling_for_macos.html#running-a-headless-server-build)
- [godot-rust built-in types (packed arrays)](https://godot-rust.github.io/book/godot-api/builtins.html)
