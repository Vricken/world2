# Phase 06 - Hardening, Cleanup, and Acceptance

## Goal

Close the refactor by removing temporary shipped-path exceptions, hardening diagnostics and documentation, and recording the final acceptance results.

Phase 06 should leave the repo describing current reality, not transition state.

## Prerequisites

- [ ] Phase 05 definition of done complete.

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
- [godot-rust built-in types](https://godot-rust.github.io/book/godot-api/builtins.html)

## Implementation Notes

- This phase is where temporary toggles and transition-only docs should disappear from the shipped path.
- If any debug-only fallback stays, document it explicitly as debug-only.
- Acceptance is not complete until code, metrics, and docs all match the same shipped architecture.

## Checklist

- [ ] Shipped runtime path no longer depends on the viewport-height override.
- [ ] Final diagnostics reflect the new selector and GPU tile backend.
- [ ] Superseded render-hot-path code is removed or clearly quarantined.
- [ ] Phase docs are updated with actual implementation status and test outcomes.
- [ ] README and profiling instructions describe the final runtime accurately.
- [ ] Remaining future-work items are recorded outside the shipped-plan checklist.

## Ordered Build Steps

1. [ ] Remove or quarantine transition-only runtime toggles.
2. [ ] Finalize diagnostics and logging.
3. [ ] Clean up dead code and stale docs.
4. [ ] Run the full acceptance matrix and record results.

## Validation and Test Gates

- [ ] Small window vs fullscreen desired chunk counts settle within about `15%`.
- [ ] Fullscreen no longer causes a multi-x deferred render commit spike.
- [ ] Fullscreen steady traversal reaches the target `120 FPS` class on the current machine and display.
- [ ] Upload bytes per frame no longer scale with fullscreen in proportion to chunk count.
- [ ] No seam regressions are visible in default scene or `300 km` scene.
- [ ] Collision remains correct on the separated CPU path.
- [ ] All relevant Rust tests, headless Godot validation, and scripted profiling probes are recorded.

## Definition of Done

- [ ] The redesign is shipped, bounded, measurable, and documented.
- [ ] The repo no longer describes transition architecture as if it were final.
- [ ] The acceptance benchmark is recorded with real outcomes.

## Test Record

- [ ] Date:
- [ ] Result summary:
- [ ] Acceptance benchmark observations:
- [ ] Deviations:
- [ ] Follow-up actions:
