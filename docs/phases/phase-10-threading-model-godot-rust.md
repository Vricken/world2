# Phase 10 - Precision Strategy

## Goal

Restore complete precision and origin strategy detail, including `f64` authority, explicit conversion boundaries, render/physics-relative transforms, and pooled-entry transform rebinding rules.

## Continuity From Phases 01-09

Phases 01-09 establish deterministic chunk identity, visibility selection, warm/cold commit behavior, and the worker/commit ownership contract. Phase 10 must keep those contracts stable while formalizing one precision policy across render, physics, and culling.

## Precision Rules

Use:

- `f64` for all planet-space math in Rust
- `f32` only for GPU upload and local chunk buffers
- camera-relative or render-origin-relative positions in render buffers
- Godot large world coordinates only when needed for engine-wide precision

Godot large-world docs note improved precision for floating-point computations in-engine, while optimization guidance also references origin-centering/shifting techniques.

## Source of Truth and Commit Conversion

The staging-buffer reuse path does not change coordinate authority:

- stable planet-space state remains authoritative in Rust
- pooled render entries must not store stale absolute positions as truth
- instance transform is rebound each activation using current render-relative transform

Keep one origin policy across subsystems. Do not let render, physics, culling, and assets drift into independent origin conventions.

## Recommended Conversion Flow

1. Generate chunk payload in stable planet-space.
2. Convert once to render-relative transform before render commit.
3. Convert once to physics-relative transform before physics commit.
4. Avoid repeated round-trip precision conversion across frame stages.

## Checklist

- [ ] Keep `f64` authority in simulation/math layers.
- [ ] Restrict `f32` use to upload/local buffer boundaries.
- [ ] Rebind pooled instance transforms at activation.
- [ ] Keep single cross-system origin policy.
- [ ] Evaluate large-world coordinates as explicit decision.
- [ ] Document precision boundaries and conversion points.

## Prerequisites

- [x] Phase 09 threading and commit ownership model completed.

## Ordered Build Steps

1. [ ] Enforce `f64` authority in planet-space math.
2. [ ] Enforce explicit `f64 -> f32` conversion boundaries.
3. [ ] Implement render-relative transform conversion at commit boundary.
4. [ ] Implement physics-relative transform conversion at physics commit boundary.
5. [ ] Ensure pooled entries never become source of truth for absolute position.

## Validation and Test Gates

- [ ] Far-distance camera movement remains stable.
- [ ] Render and physics transforms remain consistent for same chunk keys.
- [ ] Culling decisions remain stable under origin shifts.

## Definition of Done

- [ ] Precision policy is implemented and measurable.
- [ ] Single origin policy is used consistently across subsystems.

## Test Record (Fill In)

- [ ] Date:
- [ ] Result summary:
- [ ] Origin policy mode:
- [ ] Follow-up actions:

## References

- [Large world coordinates - Godot docs](https://docs.godotengine.org/en/stable/tutorials/physics/large_world_coordinates.html)
- [Optimizing 3D performance - Godot docs](https://docs.godotengine.org/en/latest/tutorials/performance/optimizing_3d_performance.html)
