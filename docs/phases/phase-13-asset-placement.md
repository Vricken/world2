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
HORIZON_SAFETY_MARGIN           = 16.0 radial slack units
COLLISION_LOD_RADIUS            = 512.0
PHYSICS_MAX_ACTIVE_CHUNKS       = 12
ASSET_CELL_GRID                 = 8x8 per chunk
COMMIT_BUDGET_PER_FRAME         = 24
UPLOAD_BUDGET_PER_FRAME         = 1 MiB
RENDER_ACTIVATIONS_PER_FRAME    = 6
RENDER_UPDATES_PER_FRAME        = 4
RENDER_DEACTIVATIONS_PER_FRAME  = 8
PHYSICS_ACTIVATIONS_PER_FRAME   = 2
PHYSICS_DEACTIVATIONS_PER_FRAME = 4
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
- `PHYSICS_MAX_ACTIVE_CHUNKS`
- per-kind render/physics commit budgets
- `WORKER_SCRATCH_COUNT`

Together with pool watermarks, these establish back-pressure behavior:

- free pooled extras above watermark
- defer low-priority uploads instead of allowing one-frame upload spikes
- preserve frame stability under aggressive camera motion

## Checklist

- [x] Encode defaults in runtime config.
- [x] Enforce commit and upload budgets in runtime loop.
- [x] Enforce pool watermarks with controlled free behavior.
- [x] Keep physics watermark lower than render watermark.
- [x] Keep worker scratch pool count aligned to worker count.
- [x] Keep payload precompute window capped at LOD 5 unless profiling justifies change.
- [x] Delay resolution increases until profiling evidence exists.

## Prerequisites

- [x] Phase 12 asset-placement ownership rules completed.

## Ordered Build Steps

1. [x] Apply default numbers to runtime config with explicit constants.
2. [x] Enforce commit and upload budgets in active diff/commit scheduling.
3. [x] Enforce render and physics pool watermarks with bounded free behavior.
4. [x] Keep worker scratch count tied to worker-thread count.
5. [x] Run baseline profiling before changing `QUADS_PER_EDGE` or major thresholds.

## Validation and Test Gates

- [x] Budget saturation defers work instead of frame spikes.
- [x] Pool sizes remain bounded under camera churn.
- [x] Runtime remains stable with default values under representative traversal.

## Definition of Done

- [x] Defaults are encoded and documented.
- [x] Back-pressure controls are active and measurable.
- [x] Any deviations from defaults are justified by profiling notes.

## Test Record

- [x] Date: 2026-03-22
- [x] Result summary: `cargo test` passed `42/42`, `./scripts/build_rust.sh` built successfully, `./scripts/run_godot.sh --headless --quit-after 5` loaded cleanly with the tighter defaults, and the close-surface scripted repro at radius `80` held `avg_frame_ms=6.84`, `p95_frame_ms=7.34`, and `max_frame_ms=8.47`.
- [x] Profiles and scenarios tested: default headless startup camera, unit tests covering horizon/physics/budget limits, and `/tmp/world2_deep_profile.gd` for the near-core close-surface regression path.
- [x] Follow-up actions: gather a moving-camera in-engine trace with gameplay-representative motion before changing mesh density, collider LOD structure, or pursuing compute-shader generation.

## References

- [Project-local phase docs and runtime metrics policy](./README.md)
