# Phase 01 - Resolution-Invariant Selection

## Goal

Replace fullscreen-sensitive LOD selection with a reference-height, budget-aware selector while keeping the current render payload backend in place.

Phase 01 ends with the runtime still using the current CPU-built render payload path, but the control plane for render selection must no longer scale directly with live viewport height.

## Prerequisites

- [x] Phase 00 exit gate complete.

## In Scope

- Add `render_lod_reference_height_px`.
- Add `target_render_chunks`.
- Add `hard_render_chunk_cap`.
- Normalize screen-space error against the reference height instead of raw live viewport height.
- Replace recursive split-until-stop behavior with best-first refinement from the six root chunks.
- Keep current split and merge thresholds as the initial refinement thresholds, now evaluated in the reference-height metric.
- Preserve neighbor `delta <= 1` normalization after capped refinement.
- Preserve current close-range density target unless a bug fix makes a different behavior unavoidable.
- Extend diagnostics with `selected_candidates`, `refinement_iterations`, `selection_cap_hits`, and `fullscreen_lod_bias = none`.

## Out of Scope

- GPU render tiles.
- Shared canonical render meshes.
- Material or tile pooling.
- Collision payload redesign.
- Render residency eviction policy changes beyond what is strictly required to support the new selector.

## Expected Touch Points

- `rust/src/runtime/pipeline/selection.rs`
- `rust/src/runtime/data.rs`
- `rust/src/runtime/core.rs`
- `rust/src/runtime/tests.rs`
- `rust/src/lib.rs`
- `scripts/profile_window_modes.sh`
- `scripts/profiling/perf_probe.gd`
- `README.md`
- `docs/gpu_refactor/phase-01-resolution-invariant-selection.md`

## Documentation To Verify Before Coding

- [Viewport - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_viewport.html)
- [Optimization using Servers - Godot docs (stable)](https://docs.godotengine.org/en/stable/tutorials/performance/using_servers.html)
- [godot-rust built-in types](https://godot-rust.github.io/book/godot-api/builtins.html)

## Implementation Notes

- Start from the visible root set, not from a fullscreen-sized target depth.
- Key the refinement queue by split benefit after hysteresis, not by raw geometric error alone.
- Stop refinement when the error target is satisfied or the target/cap is reached.
- Treat cap hits as an explicit runtime condition, not a silent failure mode.
- Keep the current seam normalization logic after selection so the selector rewrite does not change the seam contract.
- Treat `target_render_chunks` as the best-first refinement budget and `hard_render_chunk_cap` as the final seam-safe normalization ceiling, so normalized results may exceed the soft target while still remaining under the hard cap.

## Checklist

- [x] Config fields added and normalized with safe defaults.
- [x] Reference-height screen-error computation is in place.
- [x] Best-first refinement queue replaces recursive raw split traversal.
- [x] `target_render_chunks = 160` and `hard_render_chunk_cap = 224` are enforced.
- [x] Existing split and merge hysteresis are preserved in the new scoring path.
- [x] Neighbor normalization still guarantees `delta <= 1`.
- [x] New diagnostics fields are emitted.
- [x] Debug viewport-height override is no longer required to keep fullscreen chunk demand stable.
- [x] README and phase notes updated to match the shipped selector behavior.

## Ordered Build Steps

1. [x] Introduce the new config knobs and selector diagnostics.
2. [x] Isolate the current screen-error computation behind a reference-height helper.
3. [x] Add the best-first refinement data structure and split-benefit scoring.
4. [x] Apply target and hard-cap stop conditions.
5. [x] Re-run neighbor normalization on the capped selection result.
6. [x] Update profiling output so small-window vs fullscreen comparisons report the new metrics.

## Validation and Test Gates

- [x] Unit coverage proves selection invariance when only viewport size changes.
- [x] Unit coverage proves cap enforcement and best-first ordering.
- [x] Unit coverage proves neighbor normalization still converges after capped refinement.
- [x] Profiling probe shows small-window and fullscreen desired chunk counts settle within about `15%`.
- [x] Fullscreen no longer creates a multi-x increase in deferred render commits on the current CPU render path.
- [ ] Near-player detail remains visually close to the current runtime.

## Definition of Done

- [x] Fullscreen size no longer acts as the far-field detail driver.
- [x] The selector has explicit steady-state chunk bounds.
- [x] Diagnostics clearly report when selection is budget-limited.
- [ ] The runtime remains visually and functionally correct on the current render backend.

## Test Record

- [x] Date: 2026-04-06
- [x] Result summary: `cargo test` passed `73/73`; `./scripts/build_rust.sh` built successfully; `./scripts/run_godot.sh --headless --quit-after 5` loaded the updated extension and logged `selection_reference_height_px=1080`, `target_render_chunks=160`, `hard_render_chunk_cap=224`, `selected_candidates=85`, `refinement_iterations=85`, `selection_cap_hits=0`, and `fullscreen_lod_bias=none` on the first headless tick.
- [x] Small window vs fullscreen observations: `./scripts/profile_window_modes.sh` reported `small_window` at `1728 x 1116` with `avg_desired_render=155.6694`, `avg_deferred_commits=1.1028`, `avg_selected_candidates=68.7722`, and `avg_selection_cap_hits=1.1042`; `fullscreen_native` at `3456 x 2168` reported `avg_desired_render=150.9181`, `avg_deferred_commits=1.0000`, `avg_selected_candidates=64.0250`, and `avg_selection_cap_hits=0.7125`. Desired render demand differed by about `3.1%`, so fullscreen no longer drives a multi-x chunk jump.
- [x] Deviations: the final normalized render set is allowed to exceed `target_render_chunks` when seam-safe `delta <= 1` normalization needs extra splits, but it remains bounded by `hard_render_chunk_cap`. Early headless/probe ticks showed this with transient `desired_render=172` while `selection_cap_hits` still reported the soft-budget pressure explicitly.
- [x] Follow-up actions: visually confirm near-player terrain detail in an interactive camera pass before marking the final runtime-correctness box complete, then proceed to Phase 02 once the current selector metrics are accepted as the new baseline.
