# Phase 04 - GPU-Displaced Canonical Render Path

## Goal

Activate a GPU-displaced terrain render path using shared canonical meshes, pooled shader materials, and compact tile payloads while keeping collision on the CPU path.

Phase 04 ends with a working terrain render backend that no longer depends on per-chunk CPU-built render meshes for normal operation.

## Prerequisites

- [ ] Phase 03 definition of done complete.

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

- [ ] Shared canonical render meshes exist for stitch and material classes.
- [ ] Terrain shader displaces from compact tile data.
- [ ] Chunk commit path binds transforms, shared mesh, material, and tile state.
- [ ] Texture updates use a documented fixed-size update path.
- [ ] Visual seams remain correct across face edges and stitched edges.
- [ ] Runtime diagnostics expose tile uploads, material binds, and active GPU-tile render chunks.
- [ ] README and phase notes describe the live GPU render path accurately.

## Ordered Build Steps

1. [ ] Add shared canonical mesh ownership and class lookup.
2. [ ] Implement the terrain shader contract for displacement and shading inputs.
3. [ ] Bind per-chunk tile and metadata state through documented material or instance APIs.
4. [ ] Route the render commit path through shared meshes plus tile-backed materials.
5. [ ] Compare the GPU path against the CPU path for seams, transforms, and near-ground detail.

## Validation and Test Gates

- [ ] Default scene renders correctly on the GPU path.
- [ ] `300 km` scene renders correctly on the GPU path.
- [ ] Manual or scripted seam checks show no new cracks or holes.
- [ ] Near-ground detail remains visually close to the CPU path.
- [ ] Upload bytes per frame drop materially relative to the CPU mesh render path.
- [ ] Fullscreen no longer scales upload pressure in proportion to chunk count.

## Definition of Done

- [ ] Terrain rendering works through shared canonical meshes and GPU displacement.
- [ ] The runtime no longer depends on per-chunk CPU render meshes for normal render activation.
- [ ] Collision remains correct and independent on the CPU path.

## Test Record

- [ ] Date:
- [ ] Result summary:
- [ ] Visual parity observations:
- [ ] Upload and commit observations:
- [ ] Deviations:
- [ ] Follow-up actions:
