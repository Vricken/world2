# Phase 06 - Hardening, Cleanup, and Acceptance

## Goal

Close the refactor by removing temporary shipped-path exceptions, hardening diagnostics and documentation, and recording the final acceptance results.

Phase 06 should leave the repo describing current reality, not transition state.

## Prerequisites

- [x] Phase 05 definition of done complete enough for final hardening and acceptance recording.

## In Scope

- Remove the debug viewport-height override from the shipped runtime path.
- Finalize diagnostics including `fullscreen_lod_bias = none`.
- Clean up dead or superseded render-hot-path code that no longer matches the shipped architecture.
- Re-check eviction, residency, and starvation metrics against the final path.
- Update project docs and phase checklists with what actually shipped and what was tested.
- Record any remaining deferred opportunities separately from the shipped plan.

## Out of Scope

- Fresh architecture pivots.
- Replacing the chunk model with clipmaps.
- Hidden last-minute scope creep.

## Expected Touch Points

- `rust/src/runtime/pipeline/selection.rs`
- `rust/src/runtime/pipeline/commit.rs`
- `rust/src/runtime/data.rs`
- `rust/src/runtime/tests.rs`
- `rust/src/lib.rs`
- `README.md`
- `docs/gpu_refactor/README.md`
- all completed phase files in `docs/gpu_refactor/`

## Documentation To Verify Before Coding

- [Optimization using Servers - Godot docs (stable)](https://docs.godotengine.org/en/stable/tutorials/performance/using_servers.html)
- [RenderingServer - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_renderingserver.html)
- [ProjectSettings - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_projectsettings.html)
- [godot-rust built-in types](https://godot-rust.github.io/book/godot-api/builtins.html)

## Implementation Notes

- This phase is where temporary toggles and transition-only docs should disappear from the shipped path.
- If any debug-only fallback stays, document it explicitly as debug-only.
- Acceptance is not complete until code, metrics, and docs all match the same shipped architecture.

## Checklist

- [x] Shipped runtime path no longer depends on the viewport-height override.
- [x] Final diagnostics reflect the new selector and GPU tile backend.
- [x] Superseded render-hot-path code is removed or clearly quarantined.
- [x] Phase docs are updated with actual implementation status and test outcomes.
- [x] README and profiling instructions describe the final runtime accurately.
- [x] Remaining future-work items are recorded outside the shipped-plan checklist.

## Ordered Build Steps

1. [x] Remove or quarantine transition-only runtime toggles.
2. [x] Finalize diagnostics and logging.
3. [x] Clean up dead code and stale docs.
4. [x] Run the full acceptance matrix and record results.

## Validation and Test Gates

- [x] Small window vs fullscreen desired chunk counts settle within about `15%`.
- [x] Fullscreen no longer causes a multi-x deferred render commit spike.
- [x] Fullscreen steady traversal reaches the target `120 FPS` class on the current machine and display.
- [x] Upload bytes per frame no longer scale with fullscreen in proportion to chunk count.
- [ ] No seam regressions are visible in default scene or `300 km` scene.
- [x] Collision remains correct on the separated CPU path.
- [x] All relevant Rust tests, headless Godot validation, and scripted profiling probes are recorded.

## Definition of Done

- [x] The redesign is shipped, bounded, measurable, and documented.
- [x] The repo no longer describes transition architecture as if it were final.
- [x] The acceptance benchmark is recorded with real outcomes.

## Test Record

- [x] Date: 2026-04-07
- [x] Result summary: Phase 06 hardening is implemented. The shipped selector no longer carries the viewport-height override plumbing, `fullscreen_lod_bias=none` remains the final selector diagnostic, and the legacy CPU backend is quarantined behind the explicit debug-only `PlanetRoot.debug_force_server_pool_render_backend` property instead of a transition-era shipped toggle.
- [x] Acceptance benchmark observations: `cargo test` passed `88/88`. `./scripts/build_rust.sh` succeeded. `./scripts/run_godot.sh --headless --quit-after 5` and `./scripts/run_godot.sh --headless res://scenes/main_300km.tscn --quit-after 8` both loaded cleanly with `render_backend=gpu_displaced_canonical_render_backend` and `fullscreen_lod_bias=none` logged on tick `1`. `./scripts/profile_window_modes.sh` completed and recorded `small_window` at `avg_desired_render=187.2250`, `avg_deferred_upload_mib=0.000714`, and `avg_fps=119.9289`; `fullscreen_native` at `avg_desired_render=172.0000`, `avg_deferred_upload_mib=0.000000`, and `avg_fps=120.0745`; and `fullscreen_native_no_atmosphere` at `avg_desired_render=172.0000`, `avg_deferred_upload_mib=0.000000`, and `avg_fps=145.0151`. Small-window versus fullscreen desired render demand differed by about `8.9%`, which stays within the Phase 06 acceptance tolerance, and fullscreen no longer showed a deferred upload spike relative to the small-window pass.
- [x] Deviations: The final acceptance record still depends on automated seam and collision coverage plus headless scene boots rather than a fresh manual near-surface visual / collision-contact pass. That leaves the explicit "visible seam regression" gate unclaimed even though the seam-oriented Rust tests remain green.
- [x] Follow-up actions: Run one interactive near-surface traversal and collision-contact pass on both the default and `300 km` scenes, then close the remaining unchecked visual acceptance gate if no art or seam regressions appear.

## Deferred Opportunities

- Remove the legacy `RenderBackendKind::ServerPool` comparison path entirely once direct A/B debugging is no longer useful.
- Add an automated near-surface traversal and collision-contact scenario to the profiling harness so the remaining manual acceptance check becomes scriptable.
