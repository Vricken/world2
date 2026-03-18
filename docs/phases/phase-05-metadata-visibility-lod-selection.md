# Phase 05 - Visible Grid, Border Ring, and Stitch Variants

## Goal

Restore full topology and stitch-class detail so this phase captures the full compatibility and upload implications from the master plan.

## Canonical Topology

Use one canonical chunk topology:

- `QUADS_PER_EDGE = 32`
- visible vertices per edge: `33`
- sampled vertices per edge with border ring: `35`

Each chunk samples a `35 x 35` grid but renders only the inner `33 x 33` vertices. The extra ring exists so normals, tangents, and biome gradients are computed from consistent data across chunk boundaries.

## Stitch Variants

Precompute once:

- `base_indices`
- 16 stitch variants for visible mesh:
  - bit 0 = stitch `NegU`
  - bit 1 = stitch `PosU`
  - bit 2 = stitch `NegV`
  - bit 3 = stitch `PosV`

Only finer chunks stitch upward to coarser neighbors. With `max LOD delta = 1`, coarser chunks need no special handling.

## Compatibility and Region-Update Implications

This topology now has explicit class implications:

1. `vertex_count` is constant for all visible chunks within a topology class.
2. `index_count` is constant within each stitch class.
3. `vertex_bytes`, `attribute_bytes`, and `index_bytes` are constant within an exact `SurfaceClassKey`.

Render pools and staging pools must be keyed by compatible class. Even if visible grid size matches, differing stitch masks, material contracts, or format flags may require different resources and staging byte sizes.

Godot region update APIs operate on existing surfaces and byte payloads must match the surface layout they were created with.

## Runtime Role

Because topology is canonical and reused, runtime never rebuilds topology definitions. It switches stitch/index class, updates data against compatible surfaces, and fills already-sized staging buffers.

## Checklist

- [ ] Lock canonical chunk topology constants.
- [ ] Precompute base and all 16 stitch index buffers.
- [ ] Enforce fine-to-coarse stitch policy only.
- [ ] Encode stitch/index/material/format compatibility in class keys.
- [ ] Validate byte-length expectations per class before region updates.
- [ ] Route incompatibilities to compatible pool slot or cold path fallback.

## Prerequisites

- [ ] Phase 04 neighbor graph and seam-direction logic completed.

## Ordered Build Steps

1. [ ] Lock canonical topology constants (`32/33/35`).
2. [ ] Generate base index buffer and all stitch variants.
3. [ ] Enforce fine-to-coarse stitch rule.
4. [ ] Bind stitch/index/material/format into surface class compatibility.
5. [ ] Define byte-size expectations per class for upload safety.

## Validation and Test Gates

- [ ] All index buffers validate in-range.
- [ ] Visual seam test passes across all stitch masks.
- [ ] Forced incompatible warm reuse triggers fallback.

## Definition of Done

- [ ] Canonical topology can be reused without per-frame regeneration.
- [ ] Warm-path compatibility checks block invalid updates before upload calls.

## Test Record (Fill In)

- [ ] Date:
- [ ] Result summary:
- [ ] Stitch compatibility notes:
- [ ] Follow-up actions:

## References

- [RenderingServer - region update and stride APIs](https://docs.godotengine.org/en/stable/classes/class_renderingserver.html)
