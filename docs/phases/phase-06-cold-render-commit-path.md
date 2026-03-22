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
- Conservative chunk metadata generation for bounds, sampled per-chunk min/max height/radius, angular radius, geometric error, and default surface class.
- Split/merge hysteresis using `8 px` split and `4 px` merge thresholds.
- Separate desired and committed render/physics active sets with near-camera physics caps and per-frame deferred-work metrics.
- Commit-budget and upload-budget enforcement with per-kind throttles and starvation tracking for deferred work.
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
- defer overflow and track starvation depth in `SelectionFrameState`

Because render/physics server object creation lands in later phases, this phase applies the budgets to active-set commitment and byte estimates rather than real `RenderingServer` uploads.

## Deviation Notes

- The original phase wording implied precomputing metadata for every chunk through `MAX_LOD = 10`. In the current implementation, metadata is built lazily on first touch and cached. This keeps the selector deterministic while avoiding a startup-time `HashMap` allocation on the order of millions of entries for unused far-future chunks.
- Physics residency still uses the active camera as the near-player proxy. The current maintenance pass sharply narrowed that bubble and added a hard active-chunk cap so close-to-surface traversal no longer activates most selected chunks for collision.
- The 2026-03-22 maintenance pass reversed the earlier permissive defaults, restoring explicit back-pressure so visibility spikes turn into bounded streaming instead of `100 ms+` single-frame stalls.
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

## Test Record

- [x] Date: 2026-03-22
- [x] Result summary: `cargo test` passed with `42/42` tests, `./scripts/build_rust.sh` built successfully, `./scripts/run_godot.sh --headless --quit-after 5` loaded the extension and reported `desired_render=5`, `active_render=5`, `desired_physics=0`, `active_physics=0`, `horizon=5`, and `frustum=5` from the default debug camera, and `./scripts/run_godot.sh --headless -s /tmp/world2_deep_profile.gd` held the close-surface radius-`80` repro to `avg_frame_ms=6.84`, `p95_frame_ms=7.34`, and `max_frame_ms=8.47`.
- [x] Budget behavior notes: the tight-budget and per-kind budget unit tests confirm overflow work is deferred, render/physics activation spikes are capped independently, and starvation counters increment while work remains queued.
- [x] Follow-up actions: re-profile a real fly-through with camera translation near the surface, then decide whether a dedicated coarse-physics LOD path is still worth the extra complexity.

## References

- [Camera3D - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_camera3d.html)
- [Viewport - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_viewport.html)
- [Optimizing 3D performance - Godot docs](https://docs.godotengine.org/en/latest/tutorials/performance/optimizing_3d_performance.html)
- [Camera3D in godot-rust API docs](https://godot-rust.github.io/docs/gdext/master/godot/classes/struct.Camera3D.html)
- [Viewport in godot-rust API docs](https://godot-rust.github.io/docs/gdext/master/godot/classes/struct.Viewport.html)
- [Plane in godot-rust API docs](https://godot-rust.github.io/docs/gdext/master/godot/builtin/struct.Plane.html)
