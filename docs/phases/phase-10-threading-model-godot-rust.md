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

## Implemented Precision/Origin Contract

The shipped runtime now keeps one explicit precision and origin policy:

- planet-space chunk authority stays in Rust as `f64`
- chunk mesh/collider buffers are converted to chunk-local `f32` offsets at the final local-buffer boundary
- render and physics transforms are derived from the same shared origin snapshot before server commit
- the current shipping mode is thresholded camera-relative origin shifting, not Godot large-world coordinates

Operationally, this means:

- `ChunkPayload` stores `chunk_origin_planet: DVec3` as the authoritative anchor for committed geometry
- pooled render entries only retain reusable RIDs/staging; they do not become the source of truth for absolute position
- when the shared origin recenters, active render instances and physics bodies are rebound to fresh relative transforms before the next visibility/commit pass
- frustum checks convert chunk centers to render-relative coordinates using the same origin snapshot used for render commit

Current explicit decision:

- `project.godot` remains on the normal engine precision path
- `RuntimeConfig::use_large_world_coordinates` defaults to `false`
- the runtime instead uses one shared camera-relative origin with `origin_recenter_distance = 1024.0`

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

- [x] Keep `f64` authority in simulation/math layers.
- [x] Restrict `f32` use to upload/local buffer boundaries.
- [x] Rebind pooled instance transforms at activation.
- [x] Keep single cross-system origin policy.
- [x] Evaluate large-world coordinates as explicit decision.
- [x] Document precision boundaries and conversion points.

## Prerequisites

- [x] Phase 09 threading and commit ownership model completed.

## Ordered Build Steps

1. [x] Enforce `f64` authority in planet-space math.
2. [x] Enforce explicit `f64 -> f32` conversion boundaries.
3. [x] Implement render-relative transform conversion at commit boundary.
4. [x] Implement physics-relative transform conversion at physics commit boundary.
5. [x] Ensure pooled entries never become source of truth for absolute position.

## Validation and Test Gates

- [x] Far-distance camera movement remains stable.
- [x] Render and physics transforms remain consistent for same chunk keys.
- [x] Culling decisions remain stable under origin shifts.

## Definition of Done

- [x] Precision policy is implemented and measurable.
- [x] Single origin policy is used consistently across subsystems.

## Deviations From Earlier Plan Text

- The earlier plan text allowed a choice between large-world coordinates and origin shifting. The shipped implementation makes that decision explicit: shared camera-relative origin shifting is enabled by default and large-world coordinates remain off.
- The earlier wording implied render-relative conversion while filling render buffers. The shipped runtime uses a slightly stricter split: workers generate local chunk buffers from `f64` planet-space plus a stable chunk anchor, and render/physics server transforms are then derived from the shared origin snapshot at commit/rebind time.

## Test Record (Fill In)

- [x] Date: 2026-03-22
- [x] Result summary: `cargo test` passed `42/42`; `./scripts/build_rust.sh` built successfully; `./scripts/run_godot.sh --headless --quit-after 5` loaded the extension and logged `origin_mode=shared_camera_relative`, `large_world_coordinates=false`, `origin_recenter_distance=1024`, `origin_rebases=1`, `render_rebinds=0`, `physics_rebinds=0`, `worker_jobs=5`, `render_cold_commits=5`, and `physics_commits=0`.
- [x] Origin policy mode: Shared camera-relative origin shifting with chunk-local `f32` buffers and `f64` chunk anchors.
- [x] Follow-up actions: Phase 11 can now assume one shared precision/origin contract for seam validation, warm-path compatibility, and future asset placement.

## References

- [Large world coordinates - Godot docs](https://docs.godotengine.org/en/stable/tutorials/physics/large_world_coordinates.html)
- [Optimizing 3D performance - Godot docs](https://docs.godotengine.org/en/latest/tutorials/performance/optimizing_3d_performance.html)
- [godot-rust built-in types (packed arrays)](https://godot-rust.github.io/book/godot-api/builtins.html)
