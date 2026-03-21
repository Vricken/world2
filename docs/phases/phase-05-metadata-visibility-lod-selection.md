# Phase 05 - Visible Grid, Border Ring, and Stitch Variants

## Goal

Restore full topology and stitch-class detail so this phase captures the full compatibility and upload implications from the master plan.

## Implementation Status

Implemented on 2026-03-21 in:

- `rust/src/mesh_topology.rs`
- `rust/src/runtime.rs`
- `rust/src/lib.rs`

What shipped:

- Canonical `32/33/35` chunk topology constants are locked in code.
- Base visible-mesh indices plus all 16 stitch variants are precomputed once and reused from a global cache.
- Fine-to-coarse-only stitch-mask derivation rejects neighbor LOD deltas greater than 1.
- `SurfaceClassKey` now keys compatibility by topology/stitch/index/material/format plus explicit strides and byte counts.
- Packed upload regions validate both stride and byte-size expectations before any future region-update call sites use them.
- Warm-path routing explicitly rejects incompatible current surfaces and routes to a compatible pool class or cold-path fallback.

## Documentation Checked Before Implementation

Checked on 2026-03-21:

- Godot `RenderingServer` stable docs for mesh region-update APIs and surface-format stride helpers.
- godot-rust built-in types docs for `PackedByteArray` slice access and packed-array copy-on-write behavior.

Constraints carried into code:

- Surface-region updates must target already-created surfaces whose byte layout matches the original surface format.
- Reusable staging buffers should be Godot-owned packed arrays mutated in place, not transient Rust-owned allocations passed across FFI under undocumented ownership assumptions.

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

- [x] Lock canonical chunk topology constants.
- [x] Precompute base and all 16 stitch index buffers.
- [x] Enforce fine-to-coarse stitch policy only.
- [x] Encode stitch/index/material/format compatibility in class keys.
- [x] Validate byte-length expectations per class before region updates.
- [x] Route incompatibilities to compatible pool slot or cold path fallback.

## Prerequisites

- [x] Phase 04 neighbor graph and seam-direction logic completed.

## Ordered Build Steps

1. [x] Lock canonical topology constants (`32/33/35`).
2. [x] Generate base index buffer and all stitch variants.
3. [x] Enforce fine-to-coarse stitch rule.
4. [x] Bind stitch/index/material/format into surface class compatibility.
5. [x] Define byte-size expectations per class for upload safety.

## Validation and Test Gates

- [x] All index buffers validate in-range.
- [ ] Visual seam test passes across all stitch masks.
- [x] Forced incompatible warm reuse triggers fallback.

Current note:

- The visual seam gate is intentionally still pending because the runtime does not yet have the later-phase mesh commit/upload path needed to render stitch variants in-engine. This phase now validates seam topology invariants in Rust tests instead.

## Definition of Done

- [x] Canonical topology can be reused without per-frame regeneration.
- [x] Warm-path compatibility checks block invalid updates before upload calls.

## Test Record (Fill In)

- [x] Date: 2026-03-21
- [x] Result summary: `cargo test` passed with 21/21 tests, `./scripts/build_rust.sh` built successfully, and `./scripts/run_godot.sh --headless --quit-after 1` loaded the extension and reported `visible_edge_verts=33`, `sampled_edge_verts=35`, `stitch_variants=16`, and `base_index_count=6144`.
- [x] Stitch compatibility notes: odd boundary vertices are excluded on stitched edges, all stitch masks stay in-range, and incompatible warm reuse falls back to a compatible pool class or cold path.
- [x] Follow-up actions: exercise the still-pending visual seam gate once phases 07 and 08 wire stitch variants into actual `RenderingServer` mesh creation and region updates.

## Deviation Notes

- The original validation list assumed a rendered seam check in this phase. In the current implementation order that check is only possible once later mesh commit phases exist, so this phase now records topology-level seam validation in tests and leaves the rendered seam pass as an explicit follow-up.

## References

- [RenderingServer - region update and stride APIs](https://docs.godotengine.org/en/stable/classes/class_renderingserver.html)
