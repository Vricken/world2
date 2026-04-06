# Phase 06 - Visibility Selection and LOD

## Goal

Restore the full runtime visibility/LOD selection narrative, including horizon-first ordering, physics residency separation, and commit/upload budgeting.

## Implementation Status

Implemented on 2026-03-21 in:

- `rust/src/runtime/pipeline/selection.rs`
- `rust/src/runtime/workers/metadata.rs`
- `rust/src/lib.rs`
- `scenes/main.tscn`

What shipped:

- A real runtime selector that runs in the required stage order: horizon -> frustum -> projected-error LOD -> neighbor normalization -> render/physics set diffing -> budgeted commit application.
- Conservative chunk metadata generation for bounds, sampled per-chunk min/max height/radius, angular radius, geometric error, and default surface class, with bounded dense startup prebuild through the configured window and async worker generation into sparse residency above that window.
- Split/merge hysteresis using `8 px` split and `4 px` merge thresholds.
- Neighbor normalization now has both a bounded per-frame pass budget and a bounded per-frame neighbor-check work budget, plus a monotonic coarse-collapse fallback, so sparse-metadata churn cannot trap `_process()` inside either a non-converging loop or an oversized normalization pass.
- Separate desired and committed render/physics active sets with near-camera physics caps and per-frame deferred-work metrics.
- Commit-budget and upload-budget enforcement with per-kind throttles and starvation tracking for deferred work.
- A no-visual-holes invariant on deactivation: render chunks now stay resident until intersecting desired replacement coverage is already active or has committed successfully in the current frame.
- More conservative physics retirement: coarse colliders can linger while replacement render coverage or replacement physics coverage is still streaming, so collision gaps are less likely than visual holes.
- `PlanetRoot` camera-driven runtime ticks and headless debug logging so the selector can be validated with the local Godot binary.

## Documentation Checked Before Implementation

Checked on 2026-03-21:

- Godot stable `Camera3D` docs for `get_camera_transform()` and `get_frustum()`.
- Godot stable `Viewport` docs for active-camera lookup and viewport sizing behavior.
- Godot stable `Node` docs for `_process()` ordering and per-frame callback behavior.
- Godot stable performance docs for conservative culling guidance.
- godot-rust API docs for `Camera3D`, `Viewport`, `Plane`, and `Transform3D` behavior used by the selector.
- godot-rust `INode3D` docs for the `process()` virtual callback mapping used by `PlanetRoot`.

Constraints carried into code:

- Frustum planes are consumed exactly as exposed by `Camera3D.get_frustum()` instead of reconstructing undocumented camera internals.
- Horizon culling stays Rust-side and runs before frustum/LOD work.
- Because `PlanetRoot` selection runs from `_process()`, neighbor normalization must always return control to the engine within a bounded frame budget instead of relying on eventual convergence or on a single pass staying cheap.
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
- sampled min/max height
- sampled min/max radius
- conservative angular radius
- geometric error
- same-LOD neighbor data
- default surface class

## Horizon Culling

Frustum culling alone is not enough for globe-scale visibility. Horizon culling remains a Rust-side pre-frustum stage.

Current test:

- `d = |camera_pos_from_planet_center|`
- `R_occ = planet_radius - height_amplitude`
- `beta_camera = acos(R_occ / d)` when the camera is outside the guaranteed-low occluder shell
- `beta_chunk = acos(R_occ / chunk_max_radius)` when sampled chunk peaks rise above that shell
- `theta = angle(camera_dir_from_center, chunk_bound_center_dir)`
- keep if `theta <= beta_camera + beta_chunk + chunk_angular_radius + angular_slack`

There is no near-surface hard disable anymore. The runtime keeps using the sampled chunk radial interval so tall chunks can emerge over the horizon without forcing every chunk visible.

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

The runtime determines "currently split" from a cached set of active split ancestors instead of scanning all active descendants for every chunk.

## Physics Residency

Render and physics residency are independent:

- render: horizon/frustum/error driven
- physics: near-camera subset of the desired render set using `physics_activation_radius = 512.0` and `physics_max_active_chunks = 12`

This phase keeps physics conservative and budget-aware without equating it to full render visibility.

## Budgeting Rules

After desired-set diffing:

- cap logical commit work per frame with `COMMIT_BUDGET_PER_FRAME = 24`
- cap logical upload work per frame with `UPLOAD_BUDGET_BYTES_PER_FRAME = 1 MiB`
- cap per-kind work with `render_activation_budget = 6`, `render_update_budget = 4`, `render_deactivation_budget = 8`, `physics_activation_budget = 2`, and `physics_deactivation_budget = 4`
- prioritize render activation first, then render update, then physics activation, then deactivation work
- block render deactivation when it would retire visible parent/ancestor coverage before desired replacement coverage is active
- block physics deactivation when either desired render coverage or desired physics coverage for the same region is not ready yet
- defer overflow and track starvation depth in `SelectionFrameState`

Because render/physics server object creation lands in later phases, this phase applies the budgets to active-set commitment and byte estimates rather than real `RenderingServer` uploads.

## Deviation Notes

- The original phase wording implied precomputing metadata for every chunk through `MAX_LOD = 10`. The current implementation now prebuilds only a bounded dense metadata tier, using dense compact slabs through that tier plus sparse high-LOD residency above it so the selector avoids whole-planet startup allocation on large planets while still supporting async metadata misses during traversal.
- Physics residency still uses the active camera as the near-player proxy. The current maintenance pass sharply narrowed that bubble and added a hard active-chunk cap so close-to-surface traversal no longer activates most selected chunks for collision.
- The 2026-03-22 maintenance pass reversed the earlier permissive defaults, restoring explicit back-pressure so visibility spikes turn into bounded streaming instead of `100 ms+` single-frame stalls.
- Freeze investigations on 2026-04-03 and 2026-04-04 found two failure modes in the selector after high-speed sparse-metadata traversal: repeated-state normalization churn across passes and an oversized first normalization pass dominated by ancestor hash lookups. The current implementation now caps both normalization passes and normalization work items per frame and, if either guard trips, force-collapses the finer side until the `max neighbor LOD delta = 1` contract is satisfied.
- The current streaming path now prefers temporary overlap over holes: old render or physics coverage may persist for extra frames while replacements are still in flight.
- Full in-editor orbit stress testing is still a follow-up. This phase records the shipped headless validation plus unit-test coverage for selector behavior and budgeting.

## Checklist

- [x] Implement selector in required stage order.
- [x] Add conservative horizon test before frustum/LOD.
- [x] Apply hysteresis thresholds for split/merge stability.
- [x] Keep render and physics active sets separate.
- [x] Enforce commit and upload budgets every frame.
- [x] Track deferred queue depth and starvation signals.
- [x] Prevent parent/ancestor retirement from opening visible holes while replacement chunks are still loading.

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
- [x] Neighbor-normalization fallback still returns a `LOD delta <= 1` set when the iterative pass is forced to bail out.
- [x] Neighbor-normalization fallback still returns a `LOD delta <= 1` set when the per-frame normalization work budget is exhausted mid-pass.
- [x] Budget saturation defers lower-priority work instead of spiking frame in unit tests.
- [x] Physics active set stays near-camera and not equal to render set in unit tests.
- [x] Coverage-retirement guard keeps parent chunks alive until replacement coverage is ready in unit tests.

## Definition of Done

- [x] Selector is deterministic and stage-ordered.
- [x] Budget controls are enforced every frame.
- [x] Metrics exist for queued/committed/deferred operations.
- [x] Visible chunk retirement does not outrun replacement readiness.
- [x] Full startup metadata availability keeps the selector off the runtime metadata-generation path during normal traversal.

## Test Record

- [x] Date: 2026-04-03
- [x] Result summary: `cargo test` passed with `70/70` tests after the sparse-metadata maintenance pass. Neighbor normalization still splits the coarse side when metadata is ready, and now falls back to a bounded monotonic coarse collapse when either the iterative split/collapse pass repeats, the pass cap is exhausted, or the per-frame normalization work budget is exhausted, which prevents the selector from freezing `_process()` in one frame during high-speed traversal.
- [x] Budget behavior notes: the tight-budget and per-kind budget unit tests confirm overflow work is deferred, render/physics activation spikes are capped independently, and starvation counters increment while work remains queued.
- [x] Follow-up actions: re-profile a real fly-through with camera translation near the surface and verify that the new normalization fallback only appears as a transient coarsening guardrail, not as a visible long-lived LOD regression, while also checking whether the current work-budget threshold should be tuned upward or downward.

## References

- [Camera3D - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_camera3d.html)
- [Viewport - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_viewport.html)
- [Optimizing 3D performance - Godot docs](https://docs.godotengine.org/en/latest/tutorials/performance/optimizing_3d_performance.html)
- [Camera3D in godot-rust API docs](https://godot-rust.github.io/docs/gdext/master/godot/classes/struct.Camera3D.html)
- [Viewport in godot-rust API docs](https://godot-rust.github.io/docs/gdext/master/godot/classes/struct.Viewport.html)
- [Plane in godot-rust API docs](https://godot-rust.github.io/docs/gdext/master/godot/builtin/struct.Plane.html)
