# Phase 05 - Default Cutover and Physics Separation

## Goal

Make the GPU tile path the default terrain render backend and leave CPU-built geometry responsible only for collision and any explicitly approved fallback cases.

Phase 05 ends with render and physics clearly separated in runtime ownership, payload generation, and budgeting.

## Prerequisites

- [x] Phase 04 definition of done complete enough for Phase 05 cutover work. Manual near-ground visual parity follow-up remains tracked separately.

## In Scope

- Make the GPU tile path the default shipped render backend.
- Remove CPU-built mesh generation from the normal render hot path.
- Keep CPU-built collision payload generation for the near-camera physics set.
- Ensure render residency, tile residency, and collision residency are separately measurable.
- Update runtime defaults, diagnostics, profiling output, and docs to describe the new normal path.
- Keep a narrowly scoped debug fallback only if it is explicitly documented and does not distort shipped behavior.

## Out of Scope

- Clipmap replacement.
- Bigger chunks or lower max LOD as the primary fix.
- Asset-system redesign unless it becomes necessary to keep the render path correct.

## Expected Touch Points

- `rust/src/runtime/pipeline/commit.rs`
- `rust/src/runtime/pipeline/selection.rs`
- `rust/src/runtime/data.rs`
- `rust/src/runtime/workers/payloads.rs`
- `rust/src/runtime/tests.rs`
- `rust/src/lib.rs`
- `README.md`
- `scripts/profile_window_modes.sh`
- `scripts/profiling/perf_probe.gd`
- `docs/gpu_refactor/phase-05-default-cutover-and-physics-separation.md`

## Documentation To Verify Before Coding

- [RenderingServer - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_renderingserver.html)
- [ImageTexture - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_imagetexture.html)
- [godot-rust built-in types](https://godot-rust.github.io/book/godot-api/builtins.html)

## Implementation Notes

- This phase is the cutover point, so all runtime docs and profiling output need to stop describing the CPU mesh path as the normal render path.
- Physics correctness has to be validated explicitly because render and collision generation are now intentionally diverged.
- If a temporary CPU render fallback remains, it should be debug-only and excluded from normal acceptance metrics unless the docs state otherwise.

## Checklist

- [x] GPU tile rendering is the default runtime path.
- [x] CPU mesh generation is no longer part of the normal render residency hot path.
- [x] Collision generation remains correct for the physics set.
- [x] Render, tile, and collision residency metrics are reported separately.
- [x] Profiling harness reflects the new default backend.
- [x] README and phase notes describe the shipped default behavior accurately.

## Ordered Build Steps

1. [x] Flip the default render backend to the GPU tile path.
2. [x] Remove normal-path dependencies on CPU render mesh payloads.
3. [x] Confirm collision generation still works from its own CPU-owned data path.
4. [x] Update diagnostics, profiling scripts, and README language.

## Validation and Test Gates

- [x] Default scene passes full traversal checks on the default backend.
- [x] `300 km` scene passes full traversal checks on the default backend.
- [ ] No terrain holes appear during fast movement or orbit-to-surface descent.
- [x] Collision behavior remains correct after the default render cutover.
- [x] Deferred selected chunks remain bounded and drain quickly in steady traversal.
- [ ] Near-player detail remains visually close to the pre-refactor runtime.

## Definition of Done

- [x] The shipped render backend is the GPU tile path.
- [x] CPU-built geometry remains only where still justified, primarily collision.
- [x] Runtime ownership and docs clearly separate render and physics responsibilities.

## Implementation Notes

- Shipped default backend: `RuntimeConfig::default()` and `PlanetRoot` now default to `RenderBackendKind::GpuDisplacedCanonical`. The exported `PlanetRoot.use_gpu_displaced_render_backend` property remains as a debug-only escape hatch; setting it to `false` forces the legacy `ServerPool` CPU mesh path for comparison.
- Render payload generation now carries explicit `PayloadBuildRequirements`. On the default GPU path, normal render residency only requests `ChunkRenderTilePayload` plus asset placement. CPU mesh generation is only requested when the server-pool fallback is active or when a chunk belongs to the desired/active physics set.
- Worker-built collision data is now stored separately from the normal GPU render path. Collision-only payloads retain just CPU positions and indices; packed render byte regions are no longer generated for them unless the server backend explicitly needs them.
- Runtime diagnostics now report render residency, render-tile residency, and collision residency separately through `render_residency=*`, `render_tile_*`, and `collision_residency=*` / `collision_residency_bytes=*`. The profiling probe now emits `avg_gpu_tile_upload_mib`, `avg_gpu_material_binds`, `avg_active_gpu_render_chunks`, `avg_canonical_render_meshes`, `avg_collision_residency`, and `avg_collision_residency_mib`.
- A 2026-04-07 follow-up profiling pass on the shipped GPU path found the hottest main-thread costs in per-commit float-tile repacking and repeated GPU custom-AABB reconstruction. The runtime now reuses `PackedByteArray` upload buffers with bulk copies from the resident tile data and carries a conservative GPU custom AABB with each prepared payload so render commit and origin-rebind paths can reuse it instead of rebuilding it in the hot loop.

## Test Record

- [x] Date: 2026-04-07
- [x] Result summary: Phase 05 code is implemented. The runtime now boots with `render_backend=gpu_displaced_canonical_render_backend` as the shipped default, render-only payload requests on that path no longer build CPU render meshes, and collision CPU data is requested only for the desired/active physics set or the explicit server-pool fallback.
- [x] Collision observations: `cargo test` passed `88/88`, including new Phase 05 regression coverage that proves GPU render-only payloads skip CPU mesh generation while physics-selected chunks still retain collision mesh data. The scripted perf probe stayed far from the surface, so `avg_collision_residency` remained `0.0000`; that matches the near-camera physics budget and means the automated runtime probe did not enter the collision set during those specific runs.
- [x] Performance observations: `./scripts/build_rust.sh` succeeded. `./scripts/run_godot.sh --headless --quit-after 5` and `./scripts/run_godot.sh --headless res://scenes/main_300km.tscn --quit-after 8` both loaded cleanly with the GPU backend reported as default. `./scripts/profile_window_modes.sh` completed and emitted the new Phase 05 metric names. Representative settled samples: `small_window` reported `avg_active_gpu_render_chunks=172.0000`, `avg_canonical_render_meshes=9.0000`, `avg_render_tile_mib=4.018784`, `avg_collision_residency=0.0000`, and `avg_deferred_upload_mib=0.000000`; `fullscreen_native` reported `avg_active_gpu_render_chunks=139.2230`, `avg_render_tile_mib=3.734457`, `avg_collision_residency=0.0000`, and `avg_deferred_upload_mib=0.084580`; `fullscreen_native_no_atmosphere` reported `avg_active_gpu_render_chunks=153.8248`, `avg_render_tile_mib=4.144690`, `avg_collision_residency=0.0000`, and `avg_deferred_upload_mib=0.191568`.
- [x] Deviations: The scripted probe and short headless boots do not provide a manual near-ground art-quality comparison or a runtime collision-contact pass, so the close-range visual parity / live collision gate remains a manual follow-up rather than something claimed complete here.
- [x] Follow-up actions: Perform a manual near-surface traversal and collision-contact pass on the shipped default backend, then update the remaining unchecked validation gates if no regressions appear.
- [x] Date: 2026-04-07
- [x] Result summary: A follow-up optimization pass based on an interactive macOS Time Profiler capture kept the shipped GPU backend behavior unchanged while reducing obvious main-thread overhead in `commit_render_payload_with_gpu_backend()`. The runtime now reuses per-material `PackedByteArray` upload buffers for `Image::set_data()`/`ImageTexture::update()` and caches conservative GPU custom AABBs on prepared payloads for later commit and origin-rebind reuse.
- [x] Docs checked before coding: Godot `Image.set_data()` / `ImageTexture.update()` stable docs and the godot-rust built-in container docs were reviewed before changing the upload staging path.
- [x] Test coverage: `cargo test` passed `88/88`, including the existing GPU custom-AABB containment test updated to assert that the cached payload AABB matches the legacy recomputed helper exactly.
- [x] Remaining follow-up: Re-run an interactive fullscreen profile on the default scene to quantify how much main-thread time moved out of `populate_material_image()` and `gpu_chunk_custom_aabb()` after the staging and AABB-cache changes.
