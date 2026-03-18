# Phase 11 - Seam Handling Rules

## Goal

Restore complete seam-handling guidance, including sampling rules, stitch policy, and class-compatibility implications.

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

Seam state is part of surface compatibility classification, and index region byte size must match class assumptions.

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

## References

- [RenderingServer - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_renderingserver.html)
