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

- Explicit Phase 13 default constants now anchor the runtime config for radius-derived LOD, payload precompute scope, split/merge hysteresis, horizon slack, physics activation radius, commit/upload budgets, and per-kind render/physics commit caps.
- Runtime `max_lod` now respects a Project Settings-backed cap at `world2/runtime/max_lod_cap`, which defaults to `10` in-editor, is now seeded directly in `project.godot` for visibility, and still leaves topology support through LOD `16` for larger planets.
- Metadata residency now defaults to prebuilding through the effective runtime `max_lod`, and the resident metadata set is stored in dense compact slabs with cached same-LOD neighbors rather than a `HashMap<ChunkKey, ChunkMeta>`.
- Budgeted diff application already defers render and physics work when total commit count, upload bytes, or per-kind caps are exceeded, now precomputes deactivation coverage blockers instead of rescanning the desired sets per op, and keeps starvation counters visible in `SelectionFrameState` and `PlanetRoot` logs.
- Render and physics pool reuse remain bounded, with the default physics pool watermark tightened to `4` so collision pooling stays more conservative than the render per-class watermark of `8`.
- Worker-thread startup remains clamped to a small bounded count, and Phase 13 regression coverage now checks that the documented starting values and worker-count alignment stay explicit in code.
- `PlanetRoot` now exposes `planet_radius`, `frustum_culling_enabled`, and `keep_coarse_lod_chunks_rendered` in the inspector and draws a simple tool-time preview sphere in the editor so radius changes are visible before running the scene.
- The default `MainCamera` far clip now ships at `20000.0`, which is `5x` the Godot `Camera3D.far` default of `4000.0`.

## Documentation Checked Before Implementation

Checked on 2026-03-22:

- Godot stable `RenderingServer` docs for the server-driven render ownership model used by the budgeted commit path.
- Godot stable `PhysicsServer3D` docs for the conservative body/shape residency model that Phase 13 budgets and pool limits constrain.
- Godot stable `Thread-safe APIs` docs for the server-threading constraints that still shape worker/commit ownership.
- Godot stable `ProjectSettings` docs for custom project-setting registration and editor exposure.
- Godot stable `SphereMesh` docs for the editor preview primitive.
- Godot stable `Camera3D` docs for `far` as the camera's far culling boundary and `get_frustum()` as the source of the six runtime frustum planes.
- godot-rust built-in types docs for `PackedByteArray` copy-on-write semantics and reusable packed staging assumptions carried forward by the budgeted runtime path.
- godot-rust `GodotClass` docs for `#[class(tool)]` editor execution and exported inspector properties.

Constraints carried into code:

- Phase 13 changes runtime limits and pool defaults, not the documented server ownership model from Phases 08-12.
- Pool reuse must stay bounded and measurable; defaults should enforce conservative free behavior rather than allow quiet RID growth.
- Worker scratch ownership remains one reusable scratch set per worker thread rather than a shared mutable pool.
- Disabling frustum culling only bypasses the runtime frustum-plane rejection; horizon culling, projected-error LOD selection, and budgeted commit behavior remain unchanged.
- Coarse fallback coverage must stay selector-driven and server-managed; the runtime still does not create per-chunk scene-tree nodes.

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
MAX_LOD                         = radius-derived from planet_radius
MAX_LOD_CAP_PROJECT_SETTING     = 10
MIN_AVG_CHUNK_SURFACE_SPAN      = 32.0 m
TOPOLOGY_SUPPORTED_MAX_LOD      = 16
PAYLOAD_PRECOMPUTE_MAX_LOD      = 5
METADATA_PRECOMPUTE_MAX_LOD     = runtime MAX_LOD by default
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
FRUSTUM_CULLING_ENABLED         = true
KEEP_COARSE_LOD_FALLBACK        = false
RENDER_ACTIVATIONS_PER_FRAME    = 6
RENDER_UPDATES_PER_FRAME        = 4
RENDER_DEACTIVATIONS_PER_FRAME  = 8
PHYSICS_ACTIVATIONS_PER_FRAME   = 2
PHYSICS_DEACTIVATIONS_PER_FRAME = 4
POOL_WATERMARK_PER_CLASS        = 8 per surface class
PHYSICS_POOL_WATERMARK          = 4
WORKER_SCRATCH_COUNT            = one reusable scratch set per worker
MAIN_CAMERA_FAR_CLIP            = 20000.0
```

Why these are good starting values:

- radius-derived `max_lod` avoids tiny finest chunks on small planets and sharply reduces active-chunk churn near the surface
- the editor-facing cap stays explicit and easy to tune per project without recompiling Rust, while still leaving headroom above the shipped default for larger planets
- `planet_radius = 1000` now resolves to `max_lod = 5`, which keeps average finest-chunk surface span around `45.2 m`
- the default metadata prebuild window now follows that effective `max_lod`, so a `planet_radius = 1000` world still prebuilds only through LOD `5`, while larger planets no longer depend on runtime metadata misses during traversal
- even if `world2/runtime/max_lod_cap` is raised above `10`, a `planet_radius = 1000` world still stays at `max_lod = 5` because the radius-derived target is reached before the cap
- `32` quads keeps index buffers small and reusable
- `33` visible vertices support stable normals/materials
- border ring resolves most seam/shading issues early
- 16 stitch index variants stay operationally manageable

New explicit controls:

- `PAYLOAD_PRECOMPUTE_MAX_LOD`
- `MIN_AVG_CHUNK_SURFACE_SPAN`
- `MAX_LOD_CAP_PROJECT_SETTING`
- `UPLOAD_BUDGET_PER_FRAME`
- `PHYSICS_MAX_ACTIVE_CHUNKS`
- `FRUSTUM_CULLING_ENABLED`
- `KEEP_COARSE_LOD_FALLBACK`
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
- [x] Expose the project-wide `max_lod` cap in the Godot editor instead of hardcoding it in Rust only.
- [x] Expose the planet radius on `PlanetRoot` and show a matching editor preview sphere.
- [x] Expose `PlanetRoot` toggles for frustum culling and coarse fallback chunk coverage.

## Prerequisites

- [x] Phase 12 asset-placement ownership rules completed.

## Ordered Build Steps

1. [x] Apply default numbers to runtime config with explicit constants.
2. [x] Derive default runtime `max_lod` from `planet_radius` and minimum average chunk span.
3. [x] Enforce commit and upload budgets in active diff/commit scheduling.
4. [x] Enforce render and physics pool watermarks with bounded free behavior.
5. [x] Keep worker scratch count tied to worker-thread count.
6. [x] Run baseline profiling before changing `QUADS_PER_EDGE` or major thresholds.

## Validation and Test Gates

- [x] Budget saturation defers work instead of frame spikes.
- [x] Pool sizes remain bounded under camera churn.
- [x] Runtime remains stable with default values under representative traversal.
- [x] Tool-mode `PlanetRoot` can initialize in editor context without panicking while registering the custom project setting.

## Definition of Done

- [x] Defaults are encoded and documented.
- [x] Back-pressure controls are active and measurable.
- [x] Any deviations from defaults are justified by profiling notes.
- [x] Default runtime `max_lod` no longer produces sub-32-meter average finest chunks on small planets.
- [x] Planet size, coarse-coverage behavior, frustum-culling behavior, and the global LOD cap are visible/editable from the Godot editor.

## Test Record

- [x] Date: 2026-03-22
- [x] Result summary: `cargo test` passed with the Phase 13 default-number coverage updated for the new `enable_frustum_culling` and `keep_coarse_lod_chunks_rendered` controls, plus the raised `MainCamera.far = 20000.0` default. The default `planet_radius = 1000` profile still resolves to `max_lod = 5`, keeps frustum culling on by default, leaves coarse fallback off by default, and now allows a fully culled face to keep its root chunk selected when coarse fallback is enabled.
- [x] Profiles and scenarios tested: unit tests covering the documented default numbers, frustum-bypass behavior, coarse-root fallback behavior, cap behavior, budget saturation, physics-set limits, and pool watermark enforcement; `./scripts/build_rust.sh` completed successfully; `./scripts/run_godot.sh --headless --quit-after 2` loaded the extension and main scene, reporting `strategy_summary=projection=spherified_cube visibility=horizon_frustum_lod frustum_culling=true coarse_lod_fallback=false render_backend=server_pool_render_backend staging=godot_owned_packed_byte_array` on startup.
- [x] Follow-up actions: if coarse fallback is enabled in production, capture a moving-camera trace to measure the extra overlapping residency against the reduced risk of transient empty coverage during rapid camera motion.

## References

- [RenderingServer - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_renderingserver.html)
- [PhysicsServer3D - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_physicsserver3d.html)
- [Camera3D - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_camera3d.html)
- [Thread-safe APIs - Godot docs (stable)](https://docs.godotengine.org/en/stable/tutorials/performance/thread_safe_apis.html)
- [godot-rust built-in types (packed arrays)](https://godot-rust.github.io/book/godot-api/builtins.html)
- [godot-rust register/export docs](https://godot-rust.github.io/docs/gdext/master/godot/register/index.html)
- [Project-local phase docs and runtime metrics policy](./README.md)
