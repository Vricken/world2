# Phase 11 - Seam Handling Rules

## Goal

Restore complete seam-handling guidance, including sampling rules, stitch policy, class-compatibility implications, and runtime validation steps.

## Implementation Status

Implemented on 2026-03-22 in:

- `rust/src/runtime/data.rs`
- `rust/src/runtime/core.rs`
- `rust/src/runtime/tests.rs`
- `rust/src/lib.rs`

What shipped:

- Shared-border sampling remains globally consistent through the existing face-basis and cube-surface mapping rules, and rendered seam tests now cover all 24 directed cross-face seams.
- Border-ring sampling remains mandatory for mesh derivation, so normals and tangents continue to come from shared global-field samples instead of per-chunk triangle winding.
- Fine-to-coarse stitch selection remains the only allowed mixed-LOD seam mode, and delta-1 stitched-edge tests now verify even boundary vertices match the coarse cover while odd boundary vertices are excluded from stitched edge use.
- Warm-path compatibility continues to reject incompatible stitch/index classes, and the seam-sensitive fallback path is now covered explicitly by runtime tests.
- `SeamDebugSnapshot` now exposes active and pooled stitch-mask summaries, stitched-edge counts, pending seam-mismatch counts, and missing-surface-class counts for headless validation and `PlanetRoot` inspection hooks.

## Documentation Checked Before Implementation

Checked on 2026-03-22:

- Godot `RenderingServer` stable docs for mesh-surface lifecycle rules used by the warm/cold commit path that seam-class compatibility gates.
- godot-rust built-in types docs for packed-array copy-on-write and mutable-slice access that back the reusable staging buffers carried through seam-compatible warm updates.

Constraints carried into code:

- Surface refresh and reuse must stay inside the documented mesh-surface contract; seam/index changes are only valid when the reused surface class matches exactly.
- Reused packed staging buffers remain Godot-owned packed arrays mutated through documented slice access, not undocumented ownership transfer of Rust allocations.

## Continuity From Phases 01-10

This phase is a direct continuation of:

- Phase 03 face-space coordinate rules
- Phase 05 canonical topology and stitch-mask model
- Phase 07 border-ring sampling and normal derivation
- Phase 08 compatibility-gated warm/cold commit behavior
- Phase 10 precision/origin conversion policy

Phase 11 does not introduce new topology primitives. It hardens seam correctness and compatibility enforcement using systems that already exist.

## Three Required Rules (Used Together)

1. Global border sampling: every shared border vertex must come from the same face-space coordinate rule.
2. Border ring for shading: sample one hidden ring outside visible chunk.
3. Fine-to-coarse stitch indices: only finer chunks stitch.

Combined, these remove:

- geometric cracks
- face-edge mismatches
- normal seams

Skirts should not be baseline behavior.

## Compatibility Implications

Seam state is part of surface compatibility classification, and index-region byte size must match class assumptions.

If a pooled warm slot expects one stitch/index class and incoming chunk requires another incompatible class:

1. switch to pool keyed for required class, or
2. fall back to cold creation/rebuild

Do not force incompatible stitch topology into reused surfaces because vertex count alone matches.

## Checklist

- [x] Enforce shared border coordinate mapping globally.
- [x] Keep hidden border ring for derivative-quality shading.
- [x] Enforce fine-to-coarse stitch only.
- [x] Keep seam/index state in surface compatibility keys.
- [x] Block warm reuse on stitch/index incompatibility.
- [x] Keep seam debug tooling to inspect active stitch masks.

## Prerequisites

- [x] Phase 10 precision strategy completed.

## Ordered Build Steps

1. [x] Confirm shared-border coordinate rules remain identical across face boundaries.
2. [x] Keep border-ring sampling mandatory for normal/tangent derivation.
3. [x] Enforce fine-to-coarse-only stitch selection in runtime seam masks.
4. [x] Include stitch/index class in warm-path compatibility checks.
5. [x] Route incompatible warm updates to compatible pooled class or cold path.
6. [x] Add seam-debug metrics/inspection for active stitch masks.

## Validation and Test Gates

- [x] Cross-face seam tests pass for all stitch masks.
- [x] No geometric cracks appear under mixed LOD neighbors with delta = 1.
- [x] Warm path rejects incompatible seam/index classes deterministically.

## Definition of Done

- [x] Seam correctness is maintained without skirts as default behavior.
- [x] Warm reuse never applies incompatible stitch/index topology.
- [x] Seam diagnostics are available for live validation.

## Runtime Validation Hooks

Headless and live inspection now use these seam-specific outputs on `PlanetRoot`:

- `runtime_active_stitch_mask_summary()`
- `runtime_pooled_stitch_mask_summary()`
- `runtime_active_stitched_edge_summary()`
- `runtime_pending_seam_mismatch_count()`

The periodic runtime log now also includes:

- active stitched chunk count
- active stitch-mask summary
- stitched-edge summary
- pooled stitch-mask summary
- pending seam mismatches
- missing active surface-class counts

## Test Record (Fill In)

- [x] Date: 2026-03-22
- [x] Result summary: `cargo test` passed `44/44`; `./scripts/build_rust.sh` built successfully; `./scripts/run_godot.sh --headless --quit-after 5` loaded the extension and logged `Phase 11 runtime active`, `active_stitch_masks=0:5`, `stitched_edges=neg_u:0|pos_u:0|neg_v:0|pos_v:0`, `pooled_stitch_masks=none`, and `pending_seam_mismatches=0` on the default startup view.
- [x] Seam scenarios tested: shared-border rendered seams across all directed face-edge pairs at LOD 1; mixed-LOD fine-to-coarse stitched edges for all four edge directions; incompatible current-surface seam/index classes with pooled reuse and cold fallback routing.
- [x] Follow-up actions: Phase 12 can build asset residency and placement rules on top of the now-explicit seam diagnostics and stitched-edge guarantees.

## Deviations From Earlier Plan Text

- The original validation text described rendered seam coverage as “all stitch masks.” The shipped runtime now validates that in two layers: cross-face rendered border matching remains topology-agnostic because border vertex positions come from the shared sampling rule, while stitch-mask-specific coverage is exercised through delta-1 stitched-edge tests and seam-class compatibility checks.

## References

- [RenderingServer - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_renderingserver.html)
- [godot-rust built-in types](https://godot-rust.github.io/book/godot-api/builtins.html)
