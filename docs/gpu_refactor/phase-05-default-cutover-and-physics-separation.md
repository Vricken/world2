# Phase 05 - Default Cutover and Physics Separation

## Goal

Make the GPU tile path the default terrain render backend and leave CPU-built geometry responsible only for collision and any explicitly approved fallback cases.

Phase 05 ends with render and physics clearly separated in runtime ownership, payload generation, and budgeting.

## Prerequisites

- [ ] Phase 04 definition of done complete.

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

- [ ] GPU tile rendering is the default runtime path.
- [ ] CPU mesh generation is no longer part of the normal render residency hot path.
- [ ] Collision generation remains correct for the physics set.
- [ ] Render, tile, and collision residency metrics are reported separately.
- [ ] Profiling harness reflects the new default backend.
- [ ] README and phase notes describe the shipped default behavior accurately.

## Ordered Build Steps

1. [ ] Flip the default render backend to the GPU tile path.
2. [ ] Remove normal-path dependencies on CPU render mesh payloads.
3. [ ] Confirm collision generation still works from its own CPU-owned data path.
4. [ ] Update diagnostics, profiling scripts, and README language.

## Validation and Test Gates

- [ ] Default scene passes full traversal checks on the default backend.
- [ ] `300 km` scene passes full traversal checks on the default backend.
- [ ] No terrain holes appear during fast movement or orbit-to-surface descent.
- [ ] Collision behavior remains correct after the default render cutover.
- [ ] Deferred selected chunks remain bounded and drain quickly in steady traversal.
- [ ] Near-player detail remains visually close to the pre-refactor runtime.

## Definition of Done

- [ ] The shipped render backend is the GPU tile path.
- [ ] CPU-built geometry remains only where still justified, primarily collision.
- [ ] Runtime ownership and docs clearly separate render and physics responsibilities.

## Test Record

- [ ] Date:
- [ ] Result summary:
- [ ] Collision observations:
- [ ] Performance observations:
- [ ] Deviations:
- [ ] Follow-up actions:
