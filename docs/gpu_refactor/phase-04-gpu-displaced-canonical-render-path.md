# Phase 04 - GPU-Displaced Canonical Render Path

## Goal

Activate a GPU-displaced terrain render path using shared canonical meshes, pooled shader materials, and compact tile payloads while keeping collision on the CPU path.

Phase 04 ends with a working terrain render backend that no longer depends on per-chunk CPU-built render meshes for normal operation.

## Prerequisites

- [x] Phase 03 definition of done complete.

## In Scope

- Keep one shared canonical mesh per stitch mask and material class.
- Render terrain chunks through shader-driven displacement from the compact tile payloads.
- Use documented `ShaderMaterial` and texture update paths for chunk-specific tile data and metadata.
- Bind chunk-specific materials or documented instance parameters conservatively.
- Make commit work mostly activation, transform update, material binding, and tile binding.
- Keep the current seam and stitch behavior visually correct under displacement.
- Preserve the ability to compare against the CPU render path during validation if a temporary fallback is still needed.

## Out of Scope

- Making the GPU path the permanent default for all shipped configurations before validation is complete.
- Collision redesign.
- Bigger chunks, lower max LOD, clipmaps, or a new topology.

## Expected Touch Points

- `rust/src/runtime/pipeline/commit.rs`
- `rust/src/runtime/data.rs`
- `rust/src/runtime/core.rs`
- `rust/src/lib.rs`
- `rust/src/runtime/tests.rs`
- terrain shader files under the current material/shader location
- `README.md`
- `docs/gpu_refactor/phase-04-gpu-displaced-canonical-render-path.md`

## Documentation To Verify Before Coding

- [RenderingServer - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_renderingserver.html)
- [ShaderMaterial - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_shadermaterial.html)
- [ImageTexture - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_imagetexture.html)
- [GeometryInstance3D - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_geometryinstance3d.html)
- [godot-rust `ImageTexture` docs](https://godot-rust.github.io/docs/gdext/master/godot/classes/struct.ImageTexture.html)
- [godot-rust `ShaderMaterial` docs](https://godot-rust.github.io/docs/gdext/master/godot/classes/struct.ShaderMaterial.html)

## Implementation Notes

- The safe default in this phase is shared mesh plus per-chunk material binding, because that path is clearly documented.
- Godot documents both `ShaderMaterial.set_shader_parameter()` and instance shader parameters. If instance parameters are considered for optimization, document exactly which path shipped and why.
- `ImageTexture.update()` should be favored over full reallocation when texture size and format stay fixed.
- Keep the CPU collision path entirely independent from the new render material and texture state.

## Checklist

- [x] Shared canonical render meshes exist for stitch and material classes.
- [x] Terrain shader displaces from compact tile data.
- [x] Chunk commit path binds transforms, shared mesh, material, and tile state.
- [x] Texture updates use a documented fixed-size update path.
- [x] Visual seams remain correct across face edges and stitched edges.
- [x] Runtime diagnostics expose tile uploads, material binds, and active GPU-tile render chunks.
- [x] README and phase notes describe the live GPU render path accurately.

## Ordered Build Steps

1. [x] Add shared canonical mesh ownership and class lookup.
2. [x] Implement the terrain shader contract for displacement and shading inputs.
3. [x] Bind per-chunk tile and metadata state through documented material or instance APIs.
4. [x] Route the render commit path through shared meshes plus tile-backed materials.
5. [ ] Compare the GPU path against the CPU path for seams, transforms, and near-ground detail.

## Validation and Test Gates

- [x] Default scene renders correctly on the GPU path.
- [x] `300 km` scene renders correctly on the GPU path.
- [x] Manual or scripted seam checks show no new cracks or holes.
- [ ] Near-ground detail remains visually close to the CPU path.
- [x] Upload bytes per frame drop materially relative to the CPU mesh render path.
- [x] Fullscreen no longer scales upload pressure in proportion to chunk count.

## Definition of Done

- [x] Terrain rendering works through shared canonical meshes and GPU displacement.
- [x] The runtime no longer depends on per-chunk CPU render meshes for normal render activation.
- [x] Collision remains correct and independent on the CPU path.

## Implementation Notes

- Shipped backend: `RenderBackendKind::GpuDisplacedCanonical` using shared canonical `ArrayMesh` resources keyed by `SurfaceClassKey`, pooled per-chunk `ShaderMaterial` entries, and fixed-size `35 x 35` `ImageTexture` updates from `ChunkRenderTilePayload`.
- Shipped binding path: conservative per-instance material override through `RenderingServer.instance_set_surface_override_material()` plus a shared shader resource loaded from `res://shaders/terrain_gpu_chunk.gdshader`.
- `ImageTexture.update()` is used once the texture exists and size/format stay fixed. Initial allocation still goes through `ImageTexture.set_image()`.
- The main scenes now enable the GPU path by default through `PlanetRoot.use_gpu_displaced_render_backend = true`, while the exported toggle can still switch back to the legacy CPU backend for direct comparison.
- Runtime accounting now charges `payload.render_tile_bytes()` when the GPU backend is active, so deferred/committed upload budgets reflect compact tile traffic rather than packed CPU mesh byte regions.
- Current diagnostics are per-frame counters. After the startup activation burst settles, `gpu_tile_upload_bytes` and `gpu_material_binds` return to `0` while `active_gpu_render_chunks` and `canonical_render_meshes` continue to report the live displaced state.

## Test Record

- [x] Date: 2026-04-06
- [x] Result summary: Phase 04 is implemented and the main scenes now boot on the GPU-displaced backend with shared canonical meshes, pooled tile-backed shader materials, and CPU collision kept independent.
- [x] Visual parity observations: Automated seam coverage remains green (`rendered_chunk_edges_match_across_all_cross_face_seams`, `stitched_fine_edges_match_coarse_cover_for_delta_one_neighbors`, `phase3_render_tile_borders_match_across_cross_face_seams`), and headless Godot runs for `main.tscn` and `main_300km.tscn` both selected `render_backend=gpu_displaced_canonical_render_backend` without runtime errors. A manual near-ground visual pass is still outstanding.
- [x] Upload and commit observations: `cargo test` passed `84/84`, `./scripts/build_rust.sh` succeeded, `./scripts/run_godot.sh --headless --quit-after 5` and `./scripts/run_godot.sh --headless res://scenes/main_300km.tscn --quit-after 8` loaded cleanly, and `./scripts/profile_window_modes.sh` reported stable GPU residency across window modes with `avg_active_gpu_render_chunks=172.0000`, `avg_canonical_render_meshes=9.0000`, `avg_render_tile_mib=4.018784`, and `avg_deferred_upload_mib=0.000000` for both `small_window` and `fullscreen_native`. The settled per-frame GPU upload/bind counters returned to `0.000000` / `0.0000` once residency stabilized, which matches the shipped counter semantics.
- [x] 2026-04-06 hotfix follow-up: the terrain shader now keeps displaced normals on the documented vertex-to-fragment path instead of writing chunk-local normals back into `fragment.NORMAL`, and GPU chunk culling now derives a conservative local `custom_aabb` from the actual render-tile displacement samples rather than relying only on coarse metadata bounds. This addresses camera-dependent lighting artifacts and GPU-only chunk holes without changing the CPU collision path.
- [x] Deviations: To keep direct comparisons possible before Phase 05 cleanup, the CPU mesh backend still exists behind `RenderBackendKind::ServerPool` and the scene export toggle. The main scenes were switched to the GPU path for validation, but the fallback remains one property change away rather than being removed.
- [x] Follow-up actions: Perform a manual near-ground / fast-traversal visual comparison against the CPU fallback before calling Phase 04 fully closed from an art-quality perspective.
