# Phase 11 - Seam Handling Rules

## Goal

Restore complete seam-handling guidance, including sampling rules, stitch policy, class-compatibility implications, and runtime validation steps.

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

- [ ] Enforce shared border coordinate mapping globally.
- [ ] Keep hidden border ring for derivative-quality shading.
- [ ] Enforce fine-to-coarse stitch only.
- [ ] Keep seam/index state in surface compatibility keys.
- [ ] Block warm reuse on stitch/index incompatibility.
- [ ] Keep seam debug tooling to inspect active stitch masks.

## Prerequisites

- [x] Phase 10 precision strategy completed.

## Ordered Build Steps

1. [ ] Confirm shared-border coordinate rules remain identical across face boundaries.
2. [ ] Keep border-ring sampling mandatory for normal/tangent derivation.
3. [ ] Enforce fine-to-coarse-only stitch selection in runtime seam masks.
4. [ ] Include stitch/index class in warm-path compatibility checks.
5. [ ] Route incompatible warm updates to compatible pooled class or cold path.
6. [ ] Add seam-debug metrics/inspection for active stitch masks.

## Validation and Test Gates

- [ ] Cross-face seam tests pass for all stitch masks.
- [ ] No geometric cracks appear under mixed LOD neighbors with delta = 1.
- [ ] Warm path rejects incompatible seam/index classes deterministically.

## Definition of Done

- [ ] Seam correctness is maintained without skirts as default behavior.
- [ ] Warm reuse never applies incompatible stitch/index topology.
- [ ] Seam diagnostics are available for live validation.

## Test Record (Fill In)

- [ ] Date:
- [ ] Result summary:
- [ ] Seam scenarios tested:
- [ ] Follow-up actions:

## References

- [RenderingServer - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_renderingserver.html)
