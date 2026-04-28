# Phase 13 - Default Numbers I Would Start With

## Goal

Restore initial tuning defaults and runtime back-pressure controls with complete context.

## Implementation Status

Implemented on 2026-03-23 in:

- `rust/src/runtime.rs`
- `rust/src/runtime/data.rs`
- `rust/src/runtime/pipeline/commit.rs`
- `rust/src/runtime/tests.rs`
- `rust/src/lib.rs`
- `scenes/main.tscn`
- `modules/README.md`
- `README.md`

What shipped:

- Explicit Phase 13 default constants now anchor the runtime config for radius-derived LOD, payload precompute scope, split/merge hysteresis, horizon slack, physics activation radius, commit/upload budgets, and per-kind render/physics commit caps.
- Runtime `max_lod` now respects a Project Settings-backed cap at `world2/runtime/max_lod_cap`, which is now seeded directly in `project.godot` as `16` for visibility and still leaves topology support through LOD `16` for larger planets.
- Metadata residency now uses a hybrid store: dense compact slabs with cached same-LOD neighbors through a bounded dense startup tier, plus sparse `HashMap<ChunkKey, StoredChunkMeta>` residency above that tier so very large planets do not allocate whole-planet metadata up front.
- Budgeted diff application already defers render and physics work when total commit count, upload bytes, or per-kind caps are exceeded, now precomputes deactivation coverage blockers instead of rescanning the desired sets per op, and keeps starvation counters visible in `SelectionFrameState` and `PlanetRoot` logs.
- Render and physics pool reuse remain bounded, with the default physics pool watermark tightened to `4` so collision pooling stays more conservative than the render per-class watermark of `8`.
- Worker-thread startup remains clamped to a small bounded count, and Phase 13 regression coverage now checks that the documented starting values and worker-count alignment stay explicit in code.
- `PlanetRoot` now exposes `planet_radius`, `terrain_height_amplitude`, `atmosphere_height` as a planet-radius fraction, `frustum_culling_enabled`, and `keep_coarse_lod_chunks_rendered` in the inspector, keeps the `PlanetAtmosphere` child synced to the current radius and effective atmosphere thickness in both tool mode and runtime, and draws a simple tool-time preview sphere in the editor so radius changes are visible before running the scene.
- `PlanetRoot` now also derives the debug-player bootstrap distance from the configured planet scale at runtime and keeps the active camera near/far clip synced to the current camera-to-planet distance plus the atmosphere proxy cube bounds while capping far/near depth ratio, and the repository ships an inherited `scenes/main_300km.tscn` scene for headless large-planet boot validation.
- The vendored `extremely_fast_atmosphere` shader now anchors its direction-profile lookup to a planet-space sample along the view ray instead of the camera-facing outer shell entry point, so the terminator and twilight colors remain fixed on the planet during orbital fly-bys without adding raymarching or per-pixel loops.
- The default `MainCamera` far clip now ships at `100000.0`, which is `25x` the Godot `Camera3D.far` default of `4000.0` and leaves more headroom for the atmosphere shader's depth-limited sky pass on a `planet_radius = 10000` world, while larger runtime worlds now cap their bootstrap far clip to the actual initial view volume instead of multiplying the whole-planet scale by an arbitrary large constant.

## Documentation Checked Before Implementation

Checked on 2026-03-23 and 2026-04-28:

- Godot stable `RenderingServer` docs for the server-driven render ownership model used by the budgeted commit path.
- Godot stable `PhysicsServer3D` docs for the conservative body/shape residency model that Phase 13 budgets and pool limits constrain.
- Godot 4.6 `Camera3D` docs for `far`/`near` culling boundary semantics and the depth-precision tradeoff of lower near values or larger ranges.
- Godot 4.6 spatial shader docs for `VIEW`, `MODELVIEW_MATRIX`, clip/view matrices, and the recommendation to prefer `MODELVIEW_MATRIX` when floating-point issues may arise.
- Godot 4.6 `CanvasLayer` and `Label` docs for drawing runtime diagnostic UI above the 3D scene.
- godot-rust gdext `Camera3D` API docs for `get_camera_transform`, `get_far`, and `set_far`.
- godot-rust gdext `CanvasLayer` and `Label` API docs for runtime allocation, layer ordering, and label text updates.
- Godot stable `Thread-safe APIs` docs for the server-threading constraints that still shape worker/commit ownership.
- Godot stable `ProjectSettings` docs for custom project-setting registration and editor exposure.
- Godot stable `Node` docs for `owner` behavior when persisting tool-created scene children.
- Godot stable `SphereMesh` docs for the editor preview primitive.
- Godot stable `Camera3D` docs for `far` as the camera's far culling boundary and `get_frustum()` as the source of the six runtime frustum planes.
- godot-rust built-in types docs for `PackedByteArray` copy-on-write semantics and reusable packed staging assumptions carried forward by the budgeted runtime path.
- godot-rust `GodotClass` docs for `#[class(tool)]` editor execution and exported inspector properties.

Checked on 2026-04-03:

- Godot stable `Spatial shaders` docs for the view-space meaning of `VIEW`, `VERTEX`, `MODELVIEW_MATRIX`, `INV_VIEW_MATRIX`, `NODE_POSITION_WORLD`, and `CAMERA_POSITION_WORLD`.
- godot-rust `Node3D` API docs to confirm the existing transform sync path remained compatible while the atmosphere fix stayed shader-local.

Checked on 2026-04-04:

- Godot stable `Camera3D` docs for `current` camera selection and the documented `far` culling boundary semantics.
- godot-rust `Camera3D` API docs for the `get_far()` and `set_far()` binding behavior used by the runtime bootstrap.

Constraints carried into code:

- Phase 13 changes runtime limits and pool defaults, not the documented server ownership model from Phases 08-12.
- Pool reuse must stay bounded and measurable; defaults should enforce conservative free behavior rather than allow quiet RID growth.
- Worker scratch ownership remains one reusable scratch set per worker thread rather than a shared mutable pool.
- Tool-time scene helpers still need a stable scene-tree owner when they are created dynamically, and editor-facing controls should remain exported on `PlanetRoot` rather than hidden behind child-scene manual edits.
- Atmosphere thickness now scales directly from planet radius, with the shipped default set to `20%` of radius so authored shells stay proportional across scene sizes.
- Camera clip sync should only mutate a live `Viewport` current camera. The target far clip should cover the inward-faced atmosphere proxy cube from the current camera distance, but should shrink only with hysteresis because Godot's `Camera3D.far` is a culling boundary whose larger values trade away depth precision. The target near clip should rise with far clip to cap far/near ratio because Godot documents lower near values as reducing depth precision.
- Disabling frustum culling only bypasses the runtime frustum-plane rejection; horizon culling, projected-error LOD selection, and budgeted commit behavior remain unchanged.
- Coarse fallback coverage must stay selector-driven and server-managed; the runtime still does not create per-chunk scene-tree nodes.
- The atmosphere fix should stay in the existing single-pass shader path; avoid introducing raymarching, loops, or extra scene nodes for a terminator-anchoring issue.
- Direction-profile sampling should be derived from a planet-anchored point along the view ray so orbital camera motion does not shift the perceived sunrise/sunset band across the surface.

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
MAX_LOD_CAP_PROJECT_SETTING     = 16
MIN_AVG_CHUNK_SURFACE_SPAN      = 32.0 m
TOPOLOGY_SUPPORTED_MAX_LOD      = 16
METADATA_PRECOMPUTE_MAX_LOD     = caller clamp, default 8
DENSE_METADATA_PREBUILD_MAX_LOD = 8
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
MAIN_CAMERA_FAR_CLIP            = max(100000.0, runtime_player_spawn_distance + planet_radius * (1.0 + atmosphere_height))
```

Why these are good starting values:

- radius-derived `max_lod` avoids tiny finest chunks on small planets and sharply reduces active-chunk churn near the surface
- the editor-facing cap stays explicit and easy to tune per project without recompiling Rust, while the shipped `16` default no longer blocks 300 km test worlds before runtime selection can even run
- `planet_radius = 1000` now resolves to `max_lod = 5`, which keeps average finest-chunk surface span around `45.2 m`
- the default dense metadata prebuild tier now caps whole-planet startup residency at LOD `8`, so a `planet_radius = 1000` world still prebuilds only through LOD `5`, a `planet_radius = 10000` world still prebuilds through its full LOD `8`, and larger planets spill into sparse async metadata instead of blocking on whole-planet allocation
- even if `world2/runtime/max_lod_cap` is raised above `10`, a `planet_radius = 1000` world still stays at `max_lod = 5` because the radius-derived target is reached before the cap
- `32` quads keeps index buffers small and reusable
- `33` visible vertices support stable normals/materials
- border ring resolves most seam/shading issues early
- 16 stitch index variants stay operationally manageable
- a radius-relative atmosphere height keeps the shell visually proportional when planet scale changes, including the 300 km validation scene
- the runtime camera bootstrap now targets only the distance needed to see the startup shell and opposite hemisphere, instead of inflating the far plane by a large whole-planet multiplier that hurts precision on large worlds

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
- `DENSE_METADATA_PREBUILD_MAX_LOD`

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
- [x] Bound dense startup metadata residency independently from runtime `max_lod`.
- [x] Keep payload precompute window capped at LOD 5 unless profiling justifies change.
- [x] Delay resolution increases until profiling evidence exists.
- [x] Expose the project-wide `max_lod` cap in the Godot editor instead of hardcoding it in Rust only.
- [x] Expose the planet radius on `PlanetRoot` and show a matching editor preview sphere.
- [x] Expose atmosphere height on `PlanetRoot` and keep the authored atmosphere shell synced to the current planet size through a radius-relative default.
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
- [x] Large-radius startup no longer blocks on whole-planet metadata allocation before the first frame.
- [x] Tool-mode `PlanetRoot` can initialize in editor context without panicking while registering the custom project setting.

## Definition of Done

- [x] Defaults are encoded and documented.
- [x] Back-pressure controls are active and measurable.
- [x] Any deviations from defaults are justified by profiling notes.
- [x] Default runtime `max_lod` no longer produces sub-32-meter average finest chunks on small planets.
- [x] Planet size, atmosphere shell size, coarse-coverage behavior, frustum-culling behavior, and the global LOD cap are visible/editable from the Godot editor.
- [x] Startup metadata residency remains bounded for very large planets without lowering the runtime detail target.

## Test Record

- [x] Date: 2026-03-23
- [x] Result summary: `cargo test` passed with the Phase 13 default-number coverage still intact after keeping `PlanetRoot.atmosphere_height` as the single inspector property for shell sizing, now interpreted as a planet-radius fraction with a shipped default of `0.2`. The default `planet_radius = 1000` profile still resolves to `max_lod = 5`, keeps frustum culling on by default, leaves coarse fallback off by default, and now scales the atmosphere shell proportionally with planet size.
- [x] Profiles and scenarios tested: unit tests covering the documented default numbers, frustum-bypass behavior, coarse-root fallback behavior, cap behavior, budget saturation, physics-set limits, and pool watermark enforcement; `./scripts/build_rust.sh` completed successfully; `cargo test` passed (`65` tests); `./scripts/run_godot.sh --headless --quit-after 2` loaded the extension and main scene with the vendored `extremely_fast_atmosphere` shell attached, reporting `strategy_summary=projection=spherified_cube visibility=horizon_frustum_lod frustum_culling=true coarse_lod_fallback=false render_backend=server_pool_render_backend staging=godot_owned_packed_byte_array` on startup without the earlier atmosphere-parenting or `look_at()` warnings.
- [x] Follow-up actions: if coarse fallback is enabled in production, capture a moving-camera trace to measure the extra overlapping residency against the reduced risk of transient empty coverage during rapid camera motion.
- [x] Date: 2026-04-03
- [x] Result summary: startup metadata residency now stays bounded through a dense LOD-8 tier plus sparse async high-LOD metadata, so a `planet_radius = 300000` world reaches runtime startup without blocking on whole-planet metadata allocation. The sample scene bootstrap now derives player spawn distance and camera far clip from planet scale, and an inherited `main_300km.tscn` scene is available for large-planet validation.
- [x] Profiles and scenarios tested: `cargo test`; `./scripts/build_rust.sh`; `./scripts/run_godot.sh --headless --quit-after 2`; `./scripts/run_godot.sh --headless res://scenes/main_300km.tscn --quit-after 2`.
- [x] Deviations from the earlier phase note: `payload_precompute_max_lod` still exists in `RuntimeConfig`, but the shipped runtime does not eagerly precompute whole-planet payloads from it; the field is now documented as legacy/internal until a future pass repurposes or removes it.
- [x] Date: 2026-04-04
- [x] Result summary: after a live regression on the 100 km sample scene, the startup camera bootstrap was tightened so `PlanetRoot` no longer mutates camera clip distance in `ready()`, and the runtime far clip heuristic now grows only to cover the initial spawn shell and opposite hemisphere instead of scaling by a `10x` whole-planet multiplier that caused renderer instability.
- [x] Profiles and scenarios tested: `cargo test`; `./scripts/build_rust.sh`; scripted non-headless startup log check on `res://scenes/main.tscn`.
- [x] Date: 2026-04-28
- [x] Result summary: long-distance atmosphere clipping was first diagnosed as the inward-faced atmosphere cube's far side exceeding the one-shot startup camera far clip as the camera flies away. `PlanetRoot` now syncs the active camera far clip before frustum capture every runtime frame, using current camera-to-planet distance plus the `2.1x` atmosphere proxy cube bounding radius and a small margin; oversized far values shrink only after a `1.25x` hysteresis threshold to reduce depth-precision churn. Follow-up live testing showed the remaining artifact still grows and resets in cycles at distances far larger than the floating-origin recenter threshold, and the HUD showed a `0.05` near clip with an `8.4M` far/near ratio while the atmosphere shader was depth-limiting against `DEPTH_TEXTURE`. `PlanetRoot` now raises near clip with far clip to cap far/near ratio at `200000`, and the runtime debug HUD reports scene/planet coordinates, distance to current render origin, distance to planet center, origin anchor, camera near/far values, target near/far clips, render residency counters, and rebase count.
- [x] Profiles and scenarios tested: `cargo test`; `./scripts/build_rust.sh`; `./scripts/run_godot.sh --headless --quit-after 2`; `./scripts/run_godot.sh --headless res://scenes/main_300km.tscn --quit-after 2`.
- [x] Deviations from prior attempt: did not rewrite the atmosphere shader, did not use depth reconstruction as the primary ray source, did not make the proxy double-sided, and did not move the proxy around the camera. The diagnostic follow-up adds on-screen instrumentation instead of another speculative visual-path change.
- [x] Date: 2026-04-04
- [x] Result summary: a scripted Retina/window-mode probe now exists in `scenes/profiling/perf_probe.tscn` and `scripts/profile_window_modes.sh`, and isolated runs on the default scene showed the fullscreen regression is dominated by higher LOD demand and commit backlog rather than the atmosphere pass. On the repository's macOS Retina display, the probe measured `small_window` at `1728 x 1116` / `120.0235 FPS`, `fullscreen_native` at `3456 x 2168` / `116.4759 FPS` with `avg_desired_render=327.9771` and `avg_deferred_commits=83.4478`, `fullscreen_fixed_lod` at the same fullscreen pixel size but with `lod_viewport_height_override_px=1116` / `119.9981 FPS` with `avg_desired_render=110.3889` and `avg_deferred_commits=1.6569`, `fullscreen_native_no_atmosphere` at `116.7438 FPS`, and `fullscreen_fixed_lod_no_atmosphere` at `120.0614 FPS`. Because these runs were near a `120 Hz` cap, the stronger signal was the `~3x` fullscreen jump in desired render chunks and the `~50x` jump in deferred commit backlog, not the raw FPS delta alone.
- [x] Profiles and scenarios tested: `./scripts/build_rust.sh`; `./scripts/profile_window_modes.sh` on the default `res://scenes/main.tscn` content through the non-headless macOS Godot binary.
- [x] Deviations from the earlier phase note: the new `world2/debug/lod_viewport_height_override` Project Setting is intentionally debug-only and exists to hold projected-error LOD selection constant during profiling; it is not part of the shipped gameplay tuning surface.

## References

- [RenderingServer - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_renderingserver.html)
- [PhysicsServer3D - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_physicsserver3d.html)
- [Camera3D - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_camera3d.html)
- [Spatial shaders - Godot docs (stable)](https://docs.godotengine.org/en/stable/tutorials/shaders/shader_reference/spatial_shader.html)
- [Thread-safe APIs - Godot docs (stable)](https://docs.godotengine.org/en/stable/tutorials/performance/thread_safe_apis.html)
- [godot-rust built-in types (packed arrays)](https://godot-rust.github.io/book/godot-api/builtins.html)
- [godot-rust Node3D API docs](https://godot-rust.github.io/docs/gdext/master/godot/classes/struct.Node3D.html)
- [godot-rust register/export docs](https://godot-rust.github.io/docs/gdext/master/godot/register/index.html)
- [Project-local phase docs and runtime metrics policy](./README.md)
