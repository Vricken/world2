# Phase 13 - Default Numbers I Would Start With

## Goal

Restore initial tuning defaults and runtime back-pressure controls with complete context.

## Continuity From Phases 01-12

This phase sets first-pass runtime numbers for systems already defined earlier:

- topology and stitch constraints (Phases 03-05)
- visibility selection and hysteresis (Phase 06)
- payload generation and staging (Phase 07)
- server commit and pooling behavior (Phase 08)
- threading and precision policy (Phases 09-10)
- seam and asset ownership constraints (Phases 11-12)

These defaults are not final tuning values. They are stable starting points that keep runtime behavior bounded while profiling data is gathered.

## Starting Defaults

```text
MAX_LOD                         = 9 or 10
PAYLOAD_PRECOMPUTE_MAX_LOD      = 5
QUADS_PER_EDGE                  = 32
SAMPLED_EDGE                    = 35   // 33 visible + 2 border
SPLIT_THRESHOLD_PX              = 8
MERGE_THRESHOLD_PX              = 4
HORIZON_SAFETY_MARGIN           = small positive value to avoid over-culling
COLLISION_LOD_RADIUS            = near-player only
ASSET_CELL_GRID                 = 8x8 per chunk
COMMIT_BUDGET_PER_FRAME         = cap RID churn to avoid spikes
UPLOAD_BUDGET_PER_FRAME         = cap staging fills + region uploads
POOL_WATERMARK_PER_CLASS        = small bounded free-list per surface class
PHYSICS_POOL_WATERMARK          = lower than render pool watermark
WORKER_SCRATCH_COUNT            = one reusable scratch set per worker
```

Why these are good starting values:

- `32` quads keeps index buffers small and reusable
- `33` visible vertices support stable normals/materials
- border ring resolves most seam/shading issues early
- 16 stitch index variants stay operationally manageable

New explicit controls:

- `PAYLOAD_PRECOMPUTE_MAX_LOD`
- `UPLOAD_BUDGET_PER_FRAME`
- `WORKER_SCRATCH_COUNT`

Together with pool watermarks, these establish back-pressure behavior:

- free pooled extras above watermark
- defer low-priority uploads instead of allowing one-frame upload spikes
- preserve frame stability under aggressive camera motion

## Checklist

- [ ] Encode defaults in runtime config.
- [ ] Enforce commit and upload budgets in runtime loop.
- [ ] Enforce pool watermarks with controlled free behavior.
- [ ] Keep physics watermark lower than render watermark.
- [ ] Keep worker scratch pool count aligned to worker count.
- [ ] Keep payload precompute window capped at LOD 5 unless profiling justifies change.
- [ ] Delay resolution increases until profiling evidence exists.

## Prerequisites

- [ ] Phase 12 asset-placement ownership rules completed.

## Ordered Build Steps

1. [ ] Apply default numbers to runtime config with explicit constants.
2. [ ] Enforce commit and upload budgets in active diff/commit scheduling.
3. [ ] Enforce render and physics pool watermarks with bounded free behavior.
4. [ ] Keep worker scratch count tied to worker-thread count.
5. [ ] Run baseline profiling before changing `QUADS_PER_EDGE` or major thresholds.

## Validation and Test Gates

- [ ] Budget saturation defers work instead of frame spikes.
- [ ] Pool sizes remain bounded under camera churn.
- [ ] Runtime remains stable with default values under representative traversal.

## Definition of Done

- [ ] Defaults are encoded and documented.
- [ ] Back-pressure controls are active and measurable.
- [ ] Any deviations from defaults are justified by profiling notes.

## Test Record (Fill In)

- [ ] Date:
- [ ] Result summary:
- [ ] Profiles and scenarios tested:
- [ ] Follow-up actions:

## References

- [Project-local phase docs and runtime metrics policy](./README.md)