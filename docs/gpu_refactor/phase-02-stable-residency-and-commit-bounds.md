# Phase 02 - Stable Residency and Commit Bounds

## Goal

Redesign render residency and commit scheduling around stable per-key entries and fixed caps before the GPU render path is introduced.

Phase 02 should make the runtime behave like a bounded cache with predictable service guarantees, even while it still commits the current CPU-built render payloads.

## Prerequisites

- [ ] Phase 01 definition of done complete.

## In Scope

- Introduce a fixed-cap render residency cache with target `160` and hard cap `224`.
- Store render residency in stable chunk-key-indexed entries instead of treating each frame as a fresh upload opportunity.
- Prioritize already-selected chunk updates and activations ahead of speculative new admissions.
- Add explicit eviction ordering: lowest refinement benefit, then farthest distance, then oldest unused.
- Track starvation against the `30` rendered-frame service target.
- Move deactivation and activation logic onto the stable residency model while preserving hole-safe behavior.
- Extend diagnostics with render residency, evictions, and starvation counters.

## Out of Scope

- New GPU tile payload types.
- Shader-driven displacement.
- Shared canonical render meshes.
- Collision backend redesign.

## Expected Touch Points

- `rust/src/runtime/data.rs`
- `rust/src/runtime/pipeline/commit.rs`
- `rust/src/runtime/pipeline/selection.rs`
- `rust/src/runtime/tests.rs`
- `rust/src/lib.rs`
- `README.md`
- `docs/gpu_refactor/phase-02-stable-residency-and-commit-bounds.md`

## Documentation To Verify Before Coding

- [Optimization using Servers - Godot docs (stable)](https://docs.godotengine.org/en/stable/tutorials/performance/using_servers.html)
- [RenderingServer - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_renderingserver.html)
- [godot-rust built-in types](https://godot-rust.github.io/book/godot-api/builtins.html)

## Implementation Notes

- Keep the payload backend unchanged in this phase so residency logic can be validated in isolation.
- Stable residency entries should own the render lifecycle state even if the payload contents still come from CPU-built meshes.
- Selected chunks must always outrank speculative admissions once the system is under pressure.
- Preserve coverage-safe deactivation behavior while moving it onto the new residency model.

## Checklist

- [ ] Stable render residency entry type exists and is keyed by chunk.
- [ ] Target and hard caps are enforced by the residency system, not only by selection.
- [ ] Eviction ordering is implemented and documented.
- [ ] Selected-chunk service outranks speculative admits.
- [ ] Deferred work tracking includes starvation age for selected chunks.
- [ ] Coverage-safe deactivation still prevents holes.
- [ ] Runtime diagnostics expose residency counts, evictions, and starvation behavior.
- [ ] README and phase notes describe the new bounded-residency model.

## Ordered Build Steps

1. [ ] Introduce stable render residency entries and cache ownership.
2. [ ] Route existing commit ops through the stable residency state.
3. [ ] Implement admission and eviction ordering.
4. [ ] Enforce starvation accounting and selected-first servicing.
5. [ ] Extend runtime logs and probe output with residency metrics.

## Validation and Test Gates

- [ ] Unit coverage proves stable entry reuse across repeated selection frames.
- [ ] Unit coverage proves eviction ordering.
- [ ] Unit coverage proves selected chunks are serviced before speculative work.
- [ ] Unit coverage proves deferred selected chunks do not starve past the steady-state budget without a recorded failure signal.
- [ ] Fast camera motion does not open terrain holes.
- [ ] Deferred commit backlog remains bounded and drains predictably.

## Definition of Done

- [ ] Render residency has explicit steady-state bounds independent of per-frame upload opportunities.
- [ ] Commit behavior is predictable enough to support the later GPU tile cutover.
- [ ] The runtime remains correct on the current CPU render backend.

## Test Record

- [ ] Date:
- [ ] Result summary:
- [ ] Residency and starvation observations:
- [ ] Deviations:
- [ ] Follow-up actions:
