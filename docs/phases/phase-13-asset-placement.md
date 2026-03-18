# Phase 13 - Default Numbers I Would Start With

## Goal

Restore initial tuning defaults and runtime back-pressure controls with complete context.

## Starting Defaults

```text
MAX_LOD                         = 10
PAYLOAD_PRECOMPUTE_MAX_LOD      = 5
QUADS_PER_EDGE                  = 32
SAMPLED_EDGE                    = 35   // 33 visible + 2 border
SPLIT_THRESHOLD_PX              = 8
MERGE_THRESHOLD_PX              = 4
HORIZON_SAFETY_MARGIN_M         = max(100.0, 0.00005 * PLANET_RADIUS_M)
COLLISION_LOD_RADIUS_M          = 3000.0
ASSET_CELL_GRID                 = 8x8 per chunk
COMMIT_BUDGET_PER_FRAME         = 24   // max RID lifecycle ops per frame
UPLOAD_BUDGET_BYTES_PER_FRAME   = 8388608   // 8 MiB total staging upload budget
POOL_WATERMARK_PER_CLASS        = 8
PHYSICS_POOL_WATERMARK          = 32
WORKER_SCRATCH_COUNT            = WORKER_THREAD_COUNT
```

Why these are good starting values:

- `32` quads keeps index buffers small and reusable
- `33` visible vertices support stable normals/materials
- border ring resolves most seam/shading issues early
- 16 stitch index variants stay operationally manageable

New explicit controls:

- `PAYLOAD_PRECOMPUTE_MAX_LOD`
- `UPLOAD_BUDGET_BYTES_PER_FRAME`
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

## References

- [Project-local phase docs and runtime metrics policy](./README.md)
