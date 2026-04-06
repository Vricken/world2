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
- Treat `target_render_chunks = 160` as the steady-state residency target and `hard_render_chunk_cap = 224` as the absolute cache ceiling. The residency table may temporarily sit above the soft target when the selected set itself exceeds `160` or when hole-safe transitions still need both old and new coverage, but the residency system now trims unselected inactive entries back toward the target explicitly.

## Checklist

- [x] Stable render residency entry type exists and is keyed by chunk.
- [x] Target and hard caps are enforced by the residency system, not only by selection.
- [x] Eviction ordering is implemented and documented.
- [x] Selected-chunk service outranks speculative admits.
- [x] Deferred work tracking includes starvation age for selected chunks.
- [x] Coverage-safe deactivation still prevents holes.
- [x] Runtime diagnostics expose residency counts, evictions, and starvation behavior.
- [x] README and phase notes describe the new bounded-residency model.

## Ordered Build Steps

1. [x] Introduce stable render residency entries and cache ownership.
2. [x] Route existing commit ops through the stable residency state.
3. [x] Implement admission and eviction ordering.
4. [x] Enforce starvation accounting and selected-first servicing.
5. [x] Extend runtime logs and probe output with residency metrics.

## Validation and Test Gates

- [x] Unit coverage proves stable entry reuse across repeated selection frames.
- [x] Unit coverage proves eviction ordering.
- [x] Unit coverage proves selected chunks are serviced before speculative work.
- [x] Unit coverage proves deferred selected chunks do not starve past the steady-state budget without a recorded failure signal.
- [ ] Fast camera motion does not open terrain holes.
- [x] Deferred commit backlog remains bounded and drains predictably.

## Definition of Done

- [x] Render residency has explicit steady-state bounds independent of per-frame upload opportunities.
- [x] Commit behavior is predictable enough to support the later GPU tile cutover.
- [ ] The runtime remains correct on the current CPU render backend.

## Test Record

- [x] Date: 2026-04-06
- [x] Result summary: `cargo test` passed `77/77`; `./scripts/build_rust.sh` built successfully; `./scripts/run_godot.sh --headless --quit-after 5` loaded the updated extension and on the first headless tick logged `render_residency=124`, `render_residency_evictions=0`, `selected_render_starved=124`, `selected_render_starvation_failures=0`, and `selected_render_starvation_frames=1` while the initial render set was still waiting on prepared payloads.
- [x] Residency and starvation observations: `./scripts/profile_window_modes.sh` reported `small_window` at `1728 x 1116` with `avg_render_residency=166.0208`, `avg_render_residency_evictions=0.9043`, `avg_selected_render_starved=2.1540`, `avg_selected_render_starvation_failures=0.0000`, `avg_selected_render_starvation_frames=0.3842`, and `avg_deferred_commits=0.8044`. `fullscreen_native` at `3456 x 2168` reported `avg_render_residency=160.1433`, `avg_render_residency_evictions=0.6328`, `avg_selected_render_starved=1.4381`, `avg_selected_render_starvation_failures=0.0000`, `avg_selected_render_starvation_frames=0.2670`, and `avg_deferred_commits=0.4618`. In both cases the backlog stayed bounded and no selected chunk crossed the `30`-frame starvation limit.
- [x] Deviations: the residency table is explicitly soft-bounded at `160` and hard-bounded at `224`, so it can legitimately average above `160` when the selected render set itself is larger than the target or when hole-safe replacement keeps both generations resident briefly. Phase 02 was also implemented before a separately recorded interactive visual sign-off for Phase 01's near-player-detail check, so the prior visual follow-up remains open.
- [x] Follow-up actions: do an interactive fast-traversal visual pass to confirm hole-free behavior and CPU-backend correctness under camera motion, then proceed to Phase 03 with the Phase 02 residency counters as the new baseline.
