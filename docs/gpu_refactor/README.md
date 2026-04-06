# GPU Refactor Phases

This folder breaks the GPU refactor into ordered, testable phases.

The sequence is intentionally conservative:

1. establish the target runtime and acceptance checks
2. stabilize selection behavior
3. stabilize residency and commit behavior
4. introduce compact GPU-ready payloads
5. activate the GPU-displaced render path
6. make the new path the default
7. harden, clean up, and confirm acceptance targets

Do not begin a later phase until the previous phase is complete, working, and has passed its validation checks.

## Files

- [Phase 00 - Refactor Overview](./phase-00-overview-and-gatekeeping.md)
- [Phase 01 - Resolution-Invariant Selection](./phase-01-resolution-invariant-selection.md)
- [Phase 02 - Stable Residency and Commit Bounds](./phase-02-stable-residency-and-commit-bounds.md)
- [Phase 03 - Compact Render Tile Payloads](./phase-03-compact-render-tile-payloads.md)
- [Phase 04 - GPU-Displaced Canonical Render Path](./phase-04-gpu-displaced-canonical-render-path.md)
- [Phase 05 - Default Cutover and Physics Separation](./phase-05-default-cutover-and-physics-separation.md)
- [Phase 06 - Hardening, Cleanup, and Acceptance](./phase-06-hardening-cleanup-and-acceptance.md)

## Shared Rules

- Use documented Godot APIs only.
- Use documented godot-rust APIs only.
- Keep collision correctness intact while render changes evolve.
- Keep the current seam model and canonical chunk topology during this refactor.
- Update the active phase doc in the same change set as code.
- Record what was actually tested, not what was intended.

## Primary References

- [Optimization using Servers - Godot docs (stable)](https://docs.godotengine.org/en/stable/tutorials/performance/using_servers.html)
- [RenderingServer - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_renderingserver.html)
- [ShaderMaterial - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_shadermaterial.html)
- [ImageTexture - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_imagetexture.html)
- [GeometryInstance3D - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_geometryinstance3d.html)
- [godot-rust built-in types](https://godot-rust.github.io/book/godot-api/builtins.html)
- [godot-rust `ImageTexture` docs](https://godot-rust.github.io/docs/gdext/master/godot/classes/struct.ImageTexture.html)
- [godot-rust `ShaderMaterial` docs](https://godot-rust.github.io/docs/gdext/master/godot/classes/struct.ShaderMaterial.html)
