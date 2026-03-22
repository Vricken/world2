# Phase 13 - Default Numbers I Would Start With

## Goal

Restore initial tuning defaults and runtime back-pressure controls with complete context.

## Implementation Status

Implemented on 2026-03-22 in:

- `rust/src/runtime.rs`
- `rust/src/runtime/data.rs`
- `rust/src/runtime/pipeline/commit.rs`
- `rust/src/runtime/tests.rs`
- `rust/src/lib.rs`
- `README.md`

What shipped:

- Explicit Phase 13 default constants now anchor the runtime config for LOD, payload precompute scope, split/merge hysteresis, horizon slack, physics activation radius, commit/upload budgets, and per-kind render/physics commit caps.
- Budgeted diff application already defers render and physics work when total commit count, upload bytes, or per-kind caps are exceeded, and starvation counters remain visible in `SelectionFrameState` and `PlanetRoot` logs.
- Render and physics pool reuse remain bounded, with the default physics pool watermark tightened to `4` so collision pooling stays more conservative than the render per-class watermark of `8`.
- Worker-thread startup remains clamped to a small bounded count, and Phase 13 regression coverage now checks that the documented starting values and worker-count alignment stay explicit in code.

## Documentation Checked Before Implementation

Checked on 2026-03-22:

- Godot stable `RenderingServer` docs for the server-driven render ownership model used by the budgeted commit path.
- Godot stable `PhysicsServer3D` docs for the conservative body/shape residency model that Phase 13 budgets and pool limits constrain.
- Godot stable `Thread-safe APIs` docs for the server-threading constraints that still shape worker/commit ownership.
- godot-rust built-in types docs for `PackedByteArray` copy-on-write semantics and reusable packed staging assumptions carried forward by the budgeted runtime path.

Constraints carried into code:

- Phase 13 changes runtime limits and pool defaults, not the documented server ownership model from Phases 08-12.
- Pool reuse must stay bounded and measurable; defaults should enforce conservative free behavior rather than allow quiet RID growth.
- Worker scratch ownership remains one reusable scratch set per worker thread rather than a shared mutable pool.

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
POOL_WATERMARK_PER_CLASS        = 8 per surface class
PHYSICS_POOL_WATERMARK          = 4
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
- `PHYSICS_POOL_WATERMARK`
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
- [x] Result summary: `cargo test` passed `49/49`, `./scripts/build_rust.sh` built successfully, and `./scripts/run_godot.sh --headless --quit-after 5` loaded cleanly with the Phase 13 defaults. The first headless tick reported `desired_render=5`, `active_render=5`, `desired_physics=0`, `active_physics=0`, `queued_ops=5`, `deferred_ops=0`, `deferred_upload_bytes=0`, `render_pool_entries=0`, `physics_pool_entries=0`, and `starvation_frames=0`, with no shutdown RID leak errors.
- [x] Profiles and scenarios tested: unit tests covering the documented default numbers, budget saturation, physics-set limits, and pool watermark enforcement; default headless startup camera through the repository Godot binary in `../godot/bin`.
- [x] Follow-up actions: gather a moving-camera runtime trace that intentionally exercises the tighter physics pool recycling path before changing default pool sizes or per-kind commit budgets again.

## References

- [RenderingServer - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_renderingserver.html)
- [PhysicsServer3D - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_physicsserver3d.html)
- [Thread-safe APIs - Godot docs (stable)](https://docs.godotengine.org/en/stable/tutorials/performance/thread_safe_apis.html)
- [godot-rust built-in types (packed arrays)](https://godot-rust.github.io/book/godot-api/builtins.html)
- [Project-local phase docs and runtime metrics policy](./README.md)
