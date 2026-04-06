# Phase 14 - Build Order

## Goal

Restore the complete implementation order with explicit separation between correctness stages and optimization stages.

## Implementation Status

Implemented on 2026-03-22 in:

- `rust/src/runtime.rs`
- `rust/src/runtime/tests.rs`
- `rust/src/lib.rs`
- `docs/phases/README.md`
- `docs/phases/phase-14-default-numbers-and-tuning.md`
- `README.md`

What shipped:

- The full 23-step build sequence from the phase brief is now encoded in `BUILD_ORDER_STAGES` so the intended dependency order is explicit in code instead of living only in markdown.
- The cross-phase handoff is now encoded in `BUILD_ORDER_HANDOFFS`, including the documented `phases01-10 -> steps 1-20`, `phase11 -> seam hardening over steps 5/8/19`, `phase12 -> step 21`, `phase09 -> step 22`, and `phase13 -> step 23` continuity contract.
- `PlanetRuntime` now exposes build-order helpers and a compact runtime summary string, and `PlanetRoot` logs/reporting now identify the project as Phase 14 with the next planned phase called out explicitly.
- Regression tests now fail if the build-order step list becomes non-contiguous or if the documented handoff between completed phases drifts away from code reality.

## Documentation Checked Before Implementation

Checked on 2026-03-22:

- Godot stable `Optimization using Servers` docs for the server-driven ownership model and the warning that querying server state can stall asynchronous server work.
- Godot stable `RenderingServer` docs for the render-server ownership model this repository continues to use across the already-implemented stages.
- Godot stable `PhysicsServer3D` docs for the server-side collision ownership model preserved by the build-order continuity metadata.
- godot-rust built-in types docs for packed-array copy-on-write and mutable-slice semantics that remain part of the documented step 17/18 staging path.

Constraints carried into code:

- Phase 14 should not change the runtime ownership model established by phases 01-13; it should make that order explicit and testable.
- The build-order representation must stay outside the render/physics hot path and remain lightweight enough for debug and documentation use only.
- The documented warm-path and packed-array stages must remain described conservatively, without implying undocumented zero-copy behavior.

## Build Sequence

```text
1. face basis + chunk key + neighbor mapping
2. default modified / spherified cube projection
3. cube-face sample coordinates
4. 3D noise displacement on sphere
5. border ring + normal generation
6. base chunk mesh generation
7. same-LOD neighbor validation across face edges
8. stitch index buffers
9. metadata tree + bounds + angular radius + surface class
10. horizon culling
11. frustum culling
12. projected-error LOD selection
13. render/physics active-set separation
14. cold server-side render commit path
15. warm pooled render path
16. Rust byte-region packing helpers
17. reusable Godot packed staging buffers
18. in-place staging fills via resize() + as_mut_slice()
19. byte-region vertex / attribute / index updates
20. server-side physics commit path
21. chunk-group asset multimesh path
22. worker scratch reuse
23. commit budgeting / upload budgeting / pool watermarks / hysteresis / caching polish
```

This order validates hard geometry/math early, then backend correctness, then optimization layers. It intentionally separates:

- cold creation path
- warm pooled reuse path
- Rust packing path
- Godot staging-buffer path
- bounded-churn runtime controls

## Natural Handoff From Phases 01-10 Into 11-15

- Phases 01-10 cover build-order steps 1-20.
- Phase 11 hardens seam correctness over steps 5, 8, and 19.
- Phase 12 introduces chunk-group asset residency for step 21.
- Phase 09 threading plus worker-scratch policy handles step 22.
- Phase 13 default numbers and back-pressure controls harden step 23.
- Phase 14 records this dependency order directly in code and runtime diagnostics.
- Phase 15 adds strategy-layer refinement across all steps without changing ownership contracts.

This is the intended continuity path between the two author groups, and it is now represented in both docs and tests.

## Checklist

- [x] Follow the sequence unless a hard dependency blocks progress.
- [x] Document any ordering deviations and reasons.
- [x] Verify each stage before moving to next.
- [x] Keep cold path working before warm-path optimization.
- [x] Keep budget controls as explicit final hardening stage.

## Prerequisites

- [x] Phase 13 default-number policy completed.

## Validation and Test Gates

- [x] Sequencing dependencies are respected in implementation plans.
- [x] Any out-of-order work includes a documented dependency reason.
- [x] Handoff from completed to pending steps is explicit in tracking notes.

## Definition of Done

- [x] Build order and dependency policy are explicit and enforceable.
- [x] The handoff from phases 01-10 to phases 11-15 is documented and unambiguous.

## Deviations

- [x] No build-order deviation was needed. The repository already contained the runtime systems through step 23; Phase 14 only encoded and tested that continuity.

## Test Record

- [x] Date: 2026-03-22
- [x] Result summary: `cargo test` passed `51/51`, `./scripts/build_rust.sh` built successfully, and `./scripts/run_godot.sh --headless --quit-after 5` loaded cleanly. The startup log reported `build_order_summary=phase=14 steps=1-23 handoff=phases01-10=1-20,phase11=doc+5/8/19,phase12=21,phase09=22,phase13=23`, and the first headless tick reported `desired_render=5`, `active_render=5`, `deferred_ops=0`, `deferred_upload_bytes=0`, `build_order_steps=23`, and `next_phase=Phase 15 - One Important Refinement`.
- [x] Profiles and scenarios tested: unit tests for sequence contiguity and handoff invariants; default headless startup camera through the repository Godot binary in `../godot/bin`.
- [x] Follow-up actions: Phase 15 can now layer swappable strategy abstractions on top of this locked ordering without re-litigating which systems are already canonical.

## References

- [Optimization using Servers - Godot docs (stable)](https://docs.godotengine.org/en/stable/tutorials/performance/using_servers.html)
- [RenderingServer - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_renderingserver.html)
- [PhysicsServer3D - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_physicsserver3d.html)
- [godot-rust built-in types (packed arrays)](https://godot-rust.github.io/book/godot-api/builtins.html)
- [All prior phase docs](./README.md)
