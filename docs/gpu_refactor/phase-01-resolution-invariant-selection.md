# Phase 01 - Resolution-Invariant Selection

## Goal

Replace fullscreen-sensitive LOD selection with a reference-height, budget-aware selector while keeping the current render payload backend in place.

Phase 01 ends with the runtime still using the current CPU-built render payload path, but the control plane for render selection must no longer scale directly with live viewport height.

## Prerequisites

- [ ] Phase 00 exit gate complete.

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

## Checklist

- [ ] Config fields added and normalized with safe defaults.
- [ ] Reference-height screen-error computation is in place.
- [ ] Best-first refinement queue replaces recursive raw split traversal.
- [ ] `target_render_chunks = 160` and `hard_render_chunk_cap = 224` are enforced.
- [ ] Existing split and merge hysteresis are preserved in the new scoring path.
- [ ] Neighbor normalization still guarantees `delta <= 1`.
- [ ] New diagnostics fields are emitted.
- [ ] Debug viewport-height override is no longer required to keep fullscreen chunk demand stable.
- [ ] README and phase notes updated to match the shipped selector behavior.

## Ordered Build Steps

1. [ ] Introduce the new config knobs and selector diagnostics.
2. [ ] Isolate the current screen-error computation behind a reference-height helper.
3. [ ] Add the best-first refinement data structure and split-benefit scoring.
4. [ ] Apply target and hard-cap stop conditions.
5. [ ] Re-run neighbor normalization on the capped selection result.
6. [ ] Update profiling output so small-window vs fullscreen comparisons report the new metrics.

## Validation and Test Gates

- [ ] Unit coverage proves selection invariance when only viewport size changes.
- [ ] Unit coverage proves cap enforcement and best-first ordering.
- [ ] Unit coverage proves neighbor normalization still converges after capped refinement.
- [ ] Profiling probe shows small-window and fullscreen desired chunk counts settle within about `15%`.
- [ ] Fullscreen no longer creates a multi-x increase in deferred render commits on the current CPU render path.
- [ ] Near-player detail remains visually close to the current runtime.

## Definition of Done

- [ ] Fullscreen size no longer acts as the far-field detail driver.
- [ ] The selector has explicit steady-state chunk bounds.
- [ ] Diagnostics clearly report when selection is budget-limited.
- [ ] The runtime remains visually and functionally correct on the current render backend.

## Test Record

- [ ] Date:
- [ ] Result summary:
- [ ] Small window vs fullscreen observations:
- [ ] Deviations:
- [ ] Follow-up actions:
