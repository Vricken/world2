# Phase 03 - Compact Render Tile Payloads

## Goal

Introduce compact GPU-ready terrain payloads and explicit render-vs-collision payload separation without changing the default render backend yet.

Phase 03 ends with the worker path able to produce compact render tiles for selected chunks while the runtime can still fall back to the existing CPU render mesh path if needed.

## Prerequisites

- [ ] Phase 02 definition of done complete.

## In Scope

- Replace render-side mesh bytes in the design with a compact `ChunkRenderTilePayload`.
- Define render payload contents around a `35 x 35` height tile and optional normal and material tiles.
- Split render payload ownership from collision payload ownership.
- Keep `QUADS_PER_EDGE = 32` and the current stitch-mask model.
- Extend worker generation so render residency can be populated from scalar-field tiles instead of full mesh buffers.
- Add tile-pool or tile-atlas bookkeeping structures and metrics, even if full render consumption is not enabled by default yet.
- Document the shader input contract needed by the next phase.

## Out of Scope

- Default render cutover to GPU displacement.
- Removal of the CPU render path.
- Collision redesign beyond explicit separation of ownership and generation.

## Expected Touch Points

- `rust/src/runtime/data.rs`
- `rust/src/runtime/workers/payloads.rs`
- `rust/src/runtime/core.rs`
- `rust/src/runtime/tests.rs`
- `rust/src/lib.rs`
- `README.md`
- `docs/gpu_refactor/phase-03-compact-render-tile-payloads.md`

## Documentation To Verify Before Coding

- [ImageTexture - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_imagetexture.html)
- [ShaderMaterial - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_shadermaterial.html)
- [godot-rust built-in types](https://godot-rust.github.io/book/godot-api/builtins.html)
- [godot-rust `ImageTexture` docs](https://godot-rust.github.io/docs/gdext/master/godot/classes/struct.ImageTexture.html)

## Implementation Notes

- Keep this phase testable on its own by allowing the runtime to keep using the current CPU render backend while tile payload generation stabilizes.
- Collision should continue to rely on CPU-built data and should not consume the render tile payload.
- Tile payload generation must preserve seam-safe borders and match the current chunk-local sampling contract.
- If normals or material masks are deferred, the shader contract and fallback rules still need to be documented in this phase.

## Checklist

- [ ] `ChunkRenderTilePayload` or equivalent render-tile type exists.
- [ ] Collision payload ownership is separated from render payload ownership.
- [ ] Worker output can produce compact render tiles for selected chunks.
- [ ] Tile bookkeeping exists with stable handles or stable atlas slots.
- [ ] Runtime metrics report tile bytes, pool usage, and eviction-ready accounting.
- [ ] Shader input contract is documented for Phase 04.
- [ ] README and phase notes describe the dual-path state accurately.

## Ordered Build Steps

1. [ ] Define the new render tile payload and render-state ownership types.
2. [ ] Split collision payload ownership from render payload ownership.
3. [ ] Teach worker generation to emit seam-safe tiles.
4. [ ] Add tile pool or atlas bookkeeping with stable handles.
5. [ ] Extend tests and diagnostics before enabling live render consumption.

## Validation and Test Gates

- [ ] Unit coverage proves render tile borders match across seams and stitched edges.
- [ ] Unit coverage proves tile payloads match current scalar-field samples for selected chunks.
- [ ] Unit coverage proves tile-pool reuse and eviction bookkeeping behave deterministically.
- [ ] The runtime still renders correctly on the existing CPU render path after the payload split.
- [ ] Collision behavior remains correct after render-vs-collision separation.

## Definition of Done

- [ ] Compact GPU-ready render payloads exist and are testable.
- [ ] Render and collision payload ownership are clearly separated.
- [ ] The repo is ready to switch render consumption to shared meshes plus shader displacement in the next phase.

## Test Record

- [ ] Date:
- [ ] Result summary:
- [ ] Tile payload observations:
- [ ] Deviations:
- [ ] Follow-up actions:
