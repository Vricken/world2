# Phase 06 - Visibility Selection and LOD

## Goal

Restore the full runtime visibility/LOD selection narrative, including horizon-first ordering, physics residency separation, and commit/upload budgeting.

## Runtime Selection Pipeline

Runtime LOD is a cheap selector over precomputed metadata, and selection order is fixed:

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

Per chunk, precompute:

- bounding sphere
- min/max height
- min/max radius
- angular extent / conservative angular radius
- geometric error
- prebuilt seam state
- optional asset density stats
- `surface_class`

## Horizon Culling

Frustum culling alone is not enough for globe-scale visibility. Horizon culling remains a Rust-side pre-frustum stage.

Conservative test:

- `d = |camera_pos_from_planet_center|`
- `beta = acos(R_occ / d)` where `R_occ = planet_radius + safety_margin`
- `theta = angle(camera_dir_from_center, chunk_bound_center_dir)`
- keep if `theta <= beta + chunk_angular_radius`

Start conservative and fast. Tighten later only with profiling evidence.

## Frustum Culling

After horizon pass, frustum-cull survivors. Start with bounding spheres. Add optional higher-cost bounds only if profiling shows persistent false positives.

## LOD Error and Hysteresis

Use projected error:

```text
projected_error_px = geometric_error_world * projection_scale / distance_to_camera
```

Use hysteresis:

- split at `> 8 px`
- merge at `< 4 px`

This stabilizes active sets.

## Physics Residency

Render and physics residency are independent:

- render: horizon/frustum/error driven
- physics: player proximity + gameplay relevance + collision budget

Do not keep physics active for every visible render chunk.

## Budgeting Rules

After active-set diffing:

- cap RID lifecycle churn per frame (`COMMIT_BUDGET_PER_FRAME`)
- cap staging fills/region uploads per frame (`UPLOAD_BUDGET_BYTES_PER_FRAME`)
- prioritize by proximity and screen impact
- defer lower-priority operations when budget exceeded

This avoids frame spikes even with pooling and warm-path reuse.

## Checklist

- [ ] Implement selector in required stage order.
- [ ] Add conservative horizon test before frustum/LOD.
- [ ] Apply hysteresis thresholds for split/merge stability.
- [ ] Keep render and physics active sets separate.
- [ ] Enforce commit and upload budgets every frame.
- [ ] Track deferred queue depth and starvation signals.

## Prerequisites

- [ ] Phase 05 topology/stitch compatibility model completed.
- [ ] Metadata fields for bounds, error, angular radius available for all chunks.

## Ordered Build Steps

1. [ ] Implement selector order exactly (horizon -> frustum -> LOD -> neighbor normalization -> diff).
2. [ ] Implement conservative horizon culling.
3. [ ] Implement frustum culling on horizon survivors.
4. [ ] Implement projected-error split/merge with hysteresis.
5. [ ] Build separate render/physics active sets.
6. [ ] Apply diff and budgeted commit/upload scheduling.

## Validation and Test Gates

- [ ] Back-hemisphere culling works while orbiting planet.
- [ ] LOD transitions are stable near thresholds.
- [ ] Budget saturation defers lower-priority work instead of spiking frame.
- [ ] Physics active set stays near-player and not equal to render set.

## Definition of Done

- [ ] Selector is deterministic and stage-ordered.
- [ ] Budget controls are enforced every frame.
- [ ] Metrics exist for queued/committed/deferred operations.

## Test Record (Fill In)

- [ ] Date:
- [ ] Result summary:
- [ ] Budget behavior notes:
- [ ] Follow-up actions:

## References

- [Optimizing 3D performance - Godot docs](https://docs.godotengine.org/en/latest/tutorials/performance/optimizing_3d_performance.html)
- [ConcavePolygonShape3D - Godot docs](https://docs.godotengine.org/en/stable/classes/class_concavepolygonshape3d.html)
