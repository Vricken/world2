# Phase 06 - Visibility Selection and LOD

## Goal

Restore the full runtime visibility/LOD selection narrative, including horizon-first ordering, physics residency separation, and commit/upload budgeting.

## Implementation Status

Implemented on 2026-03-21 in:

- `rust/src/runtime/pipeline/selection.rs`
- `rust/src/lib.rs`
- `scenes/main.tscn`

What shipped:

- A real runtime selector that runs in the required stage order: horizon -> frustum -> projected-error LOD -> neighbor normalization -> render/physics set diffing -> budgeted commit application.
- Conservative chunk metadata generation for bounds, min/max radius, angular radius, geometric error, and default surface class.
- Split/merge hysteresis using `8 px` split and `4 px` merge thresholds.
- Separate desired and committed render/physics active sets with per-frame deferred-work metrics.
- Commit-budget and upload-budget enforcement with starvation tracking for deferred work.
- `PlanetRoot` camera-driven runtime ticks and headless debug logging so the selector can be validated with the local Godot binary.

## Documentation Checked Before Implementation

Checked on 2026-03-21:

- Godot stable `Camera3D` docs for `get_camera_transform()` and `get_frustum()`.
- Godot stable `Viewport` docs for active-camera lookup and viewport sizing behavior.
- Godot stable performance docs for conservative culling guidance.
- godot-rust API docs for `Camera3D`, `Viewport`, `Plane`, and `Transform3D` behavior used by the selector.

Constraints carried into code:

- Frustum planes are consumed exactly as exposed by `Camera3D.get_frustum()` instead of reconstructing undocumented camera internals.
- Horizon culling stays Rust-side and runs before frustum/LOD work.
- Upload budgeting is modeled conservatively from surface-class byte counts because the real region-update path is still a later phase.

## Runtime Selection Pipeline

Runtime LOD is a cheap selector over cached metadata, and selection order is fixed:

```text
1. Start from 6 roots.
2. Horizon-cull.
3. Frustum-cull survivors.
4. For each surviving chunk:
   - compute projected error in pixels
   - if split/merge hysteresis says split and lod < max_lod: recurse
   - else keep leaf
5. Enforce max neighbor LOD delta = 1 by splitting overly coarse selected neighbors.
6. Build new desired render set.
7. Build near-camera desired physics set.
8. Diff desired vs committed sets and apply budget-limited operations.
```

Per selected or cached chunk, runtime now has:

- bounding sphere
- min/max height
- min/max radius
- conservative angular radius
- geometric error
- same-LOD neighbor data
- default surface class

## Horizon Culling

Frustum culling alone is not enough for globe-scale visibility. Horizon culling remains a Rust-side pre-frustum stage.

Current test:

- `d = |camera_pos_from_planet_center|`
- `beta = acos((planet_radius + safety_margin) / d)` when the camera is outside the occluder sphere
- `theta = angle(camera_dir_from_center, chunk_bound_center_dir)`
- keep if `theta <= beta + chunk_angular_radius`

If the camera is inside the occluder radius, horizon culling falls back to keeping the chunk.

## Frustum Culling

After horizon pass, frustum-cull survivors using the documented world-space planes returned by `Camera3D.get_frustum()`.

Current rule:

- convert chunk bound center to `Vector3`
- reject when any frustum plane distance exceeds the chunk sphere radius on the outside side of the plane

This remains sphere-only for now.

## LOD Error and Hysteresis

Projected error uses:

```text
projected_error_px = geometric_error_world * projection_scale / distance_to_camera
```

Hysteresis rules:

- split at `> 8 px`
- remain split until `< 4 px`

The runtime determines "currently split" from committed active descendants, which keeps visible set churn down while budgets catch up.

## Physics Residency

Render and physics residency are independent:

- render: horizon/frustum/error driven
- physics: near-camera subset of the desired render set using `COLLISION_LOD_RADIUS_M = 3000.0`

This phase keeps physics conservative and budget-aware without equating it to full render visibility.

## Budgeting Rules

After desired-set diffing:

- cap logical commit work per frame with `COMMIT_BUDGET_PER_FRAME = 1024`
- cap logical upload work per frame with `UPLOAD_BUDGET_BYTES_PER_FRAME = 64 MiB`
- prioritize render activation first, then physics activation, then deactivation work
- defer overflow and track starvation depth in `SelectionFrameState`

Because render/physics server object creation lands in later phases, this phase applies the budgets to active-set commitment and byte estimates rather than real `RenderingServer` uploads.

## Deviation Notes

- The original phase wording implied precomputing metadata for every chunk through `MAX_LOD = 10`. In the current implementation, metadata is built lazily on first touch and cached. This keeps the selector deterministic while avoiding a startup-time `HashMap` allocation on the order of millions of entries for unused far-future chunks.
- Physics residency currently uses the active camera as the near-player proxy. The default scene now supplies that camera through the fly controller rig.
- The maintenance pass before Phase 11 raised the default commit budget from `24` to `1024` and the upload budget from `8 MiB` to `64 MiB` so free-fly camera movement can activate chunk churn more aggressively without leaving chunk-sized holes during transition frames.
- Full in-editor orbit stress testing is still a follow-up. This phase records the shipped headless validation plus unit-test coverage for selector behavior and budgeting.

## Checklist

- [x] Implement selector in required stage order.
- [x] Add conservative horizon test before frustum/LOD.
- [x] Apply hysteresis thresholds for split/merge stability.
- [x] Keep render and physics active sets separate.
- [x] Enforce commit and upload budgets every frame.
- [x] Track deferred queue depth and starvation signals.

## Prerequisites

- [x] Phase 05 topology/stitch compatibility model completed.
- [x] Metadata fields for bounds, error, and angular radius are available for every chunk the selector touches.

## Ordered Build Steps

1. [x] Implement selector order exactly (horizon -> frustum -> LOD -> neighbor normalization -> diff).
2. [x] Implement conservative horizon culling.
3. [x] Implement frustum culling on horizon survivors.
4. [x] Implement projected-error split/merge with hysteresis.
5. [x] Build separate render/physics active sets.
6. [x] Apply diff and budgeted commit/upload scheduling.

## Validation and Test Gates

- [x] Default external camera rejects one root face through the horizon pass in headless validation.
- [x] LOD transitions are stabilized by hysteresis rules and neighbor normalization in unit tests.
- [x] Budget saturation defers lower-priority work instead of spiking frame in unit tests.
- [x] Physics active set stays near-camera and not equal to render set in unit tests.

## Definition of Done

- [x] Selector is deterministic and stage-ordered.
- [x] Budget controls are enforced every frame.
- [x] Metrics exist for queued/committed/deferred operations.

## Test Record (Fill In)

- [x] Date: 2026-03-21
- [x] Result summary: `cargo test` passed with 25/25 tests, `./scripts/build_rust.sh` built successfully, and `./scripts/run_godot.sh --headless --quit-after 5` loaded the extension and reported `desired_render=5`, `active_render=5`, `desired_physics=1`, `active_physics=1`, `horizon=5`, and `frustum=5` from the default debug camera.
- [x] Budget behavior notes: the tight-budget unit test confirms overflow work is deferred, `deferred_upload_bytes` is non-zero when the upload budget is undersized, and starvation counters increment while work remains queued.
- [x] Follow-up actions: connect these logical commit/upload budgets to the real server-side mesh and physics commit path in Phases 07 and 08, then add a scripted fly-path stress pass to exercise warm reuse repeatedly.

## References

- [Camera3D - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_camera3d.html)
- [Viewport - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_viewport.html)
- [Optimizing 3D performance - Godot docs](https://docs.godotengine.org/en/latest/tutorials/performance/optimizing_3d_performance.html)
- [Camera3D in godot-rust API docs](https://godot-rust.github.io/docs/gdext/master/godot/classes/struct.Camera3D.html)
- [Viewport in godot-rust API docs](https://godot-rust.github.io/docs/gdext/master/godot/classes/struct.Viewport.html)
- [Plane in godot-rust API docs](https://godot-rust.github.io/docs/gdext/master/godot/builtin/struct.Plane.html)
