# Phase 03 - Compact Render Tile Payloads

## Goal

Introduce compact GPU-ready terrain payloads and explicit render-vs-collision payload separation without changing the default render backend yet.

Phase 03 ends with the worker path able to produce compact render tiles for selected chunks while the runtime can still fall back to the existing CPU render mesh path if needed.

## Prerequisites

- [ ] Phase 02 definition of done complete.

Phase 03 was implemented with the documented Phase 02 manual fast-traversal visual pass still open. The shipped code keeps that deviation explicit instead of pretending the prerequisite was fully closed first.

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

## Shipped Implementation

- `ChunkRenderTilePayload` now ships in `rust/src/runtime/data.rs` with a fixed `35 x 35` seam-safe sample layout, a required `height_tile`, an optional `material_tile`, and a deferred `normal_tile`.
- Worker generation in `rust/src/runtime/workers/payloads.rs` now emits the compact render tile directly from scalar-field samples before mesh packing, so the render path has a GPU-ready payload even while the CPU mesh backend remains active.
- `ChunkPayload` now owns explicit `ChunkCollisionPayload` state separately from render tile state. Collision faces are still materialized lazily from CPU mesh buffers when physics residency actually needs them.
- `PlanetRuntime` now maintains a stable reusable tile-slot pool keyed by chunk residency. Slots persist while a chunk payload is resident, freed slots are reused deterministically, and the runtime reports tile bytes plus slot/free/eviction-ready counts every frame.
- The current render backend remains unchanged on purpose in this phase: CPU mesh buffers plus packed regions are still committed to `RenderingServer`, while the compact tile payload and slot bookkeeping prepare the Phase 04 shader-driven cutover.

## Phase 04 Shader Input Contract

- Tile resolution is fixed at `35 x 35`, matching `SAMPLED_VERTICES_PER_EDGE` and preserving the one-quad seam-safe border ring around the visible `33 x 33` vertex grid.
- `height_tile` stores terrain height offsets in meters relative to the configured base planet radius. Consumers reconstruct displaced positions by sampling this tile in chunk-local sample space.
- `material_tile`, when present, stores `[biome0, biome1, slope_hint, 1.0]` per sample. This matches the current CPU color payload and gives Phase 04 shader code a stable material-mask contract.
- `normal_tile` is not populated in Phase 03. Any shader path consuming the tile payload before Phase 04 normal support lands must derive normals from neighboring height samples or fall back to the existing CPU mesh normals.
- Tile-slot ids are stable only while a chunk payload remains resident. Once a payload is evicted, its slot may be recycled for a different chunk, so Phase 04 resource binding must treat slot reuse as normal behavior.

## Checklist

- [x] `ChunkRenderTilePayload` or equivalent render-tile type exists.
- [x] Collision payload ownership is separated from render payload ownership.
- [x] Worker output can produce compact render tiles for selected chunks.
- [x] Tile bookkeeping exists with stable handles or stable atlas slots.
- [x] Runtime metrics report tile bytes, pool usage, and eviction-ready accounting.
- [x] Shader input contract is documented for Phase 04.
- [x] README and phase notes describe the dual-path state accurately.

## Ordered Build Steps

1. [x] Define the new render tile payload and render-state ownership types.
2. [x] Split collision payload ownership from render payload ownership.
3. [x] Teach worker generation to emit seam-safe tiles.
4. [x] Add tile pool or atlas bookkeeping with stable handles.
5. [x] Extend tests and diagnostics before enabling live render consumption.

## Validation and Test Gates

- [x] Unit coverage proves render tile borders match across seams and stitched edges.
- [x] Unit coverage proves tile payloads match current scalar-field samples for selected chunks.
- [x] Unit coverage proves tile-pool reuse and eviction bookkeeping behave deterministically.
- [x] The runtime still renders correctly on the existing CPU render path after the payload split.
- [x] Collision behavior remains correct after render-vs-collision separation.

## Definition of Done

- [x] Compact GPU-ready render payloads exist and are testable.
- [x] Render and collision payload ownership are clearly separated.
- [x] The repo is ready to switch render consumption to shared meshes plus shader displacement in the next phase.

## Test Record

- [x] Date: 2026-04-06
- [x] Result summary: `cargo fmt` passed, `cargo test` passed `81/81`, `./scripts/build_rust.sh` succeeded, `./scripts/run_godot.sh --headless --quit-after 5` loaded cleanly, and `./scripts/profile_window_modes.sh` completed with the new Phase 03 tile metrics exposed in every scenario.
- [x] Tile payload observations: `small_window` averaged `avg_render_tile_mib=4.115160`, `avg_render_tile_pool_slots=199.6533`, `avg_render_tile_pool_active_slots=176.1248`, `avg_render_tile_pool_free_slots=23.5284`, and `avg_render_tile_eviction_ready_slots=0.0444`; `fullscreen_native` averaged `avg_render_tile_mib=3.727029`, `avg_render_tile_pool_slots=210.0000`, `avg_render_tile_pool_active_slots=159.5132`, `avg_render_tile_pool_free_slots=50.4868`, and `avg_render_tile_eviction_ready_slots=22.9931`. Fullscreen remained on the reference-height selector with `fullscreen_lod_bias=none`.
- [x] Deviations: the default render backend still carries CPU mesh buffers and packed regions by design, and `normal_tile` is still deferred (`None`) in this phase. Phase 03 also proceeded while the earlier Phase 02 manual fast-traversal visual pass remained open.
- [x] Follow-up actions: do the interactive traversal visual pass that was already pending from Phase 02, then use the stable `35 x 35` tile contract plus slot bookkeeping to switch the render backend over to shared canonical meshes and shader displacement in Phase 04.
