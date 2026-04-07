# Phase 00 - Refactor Overview

## Goal

Deliver a terrain runtime whose steady-state render cost is controlled by explicit budgets, not by live viewport size, while preserving close-range terrain detail and keeping collision correct.

The concrete end state of this refactor is:

- fullscreen and Retina resolution do not cause the CPU selector to ask for dramatically more terrain,
- render residency is explicitly bounded and predictable,
- terrain rendering uses shared canonical chunk meshes plus compact GPU-consumed height or shading tiles,
- CPU-built geometry is no longer the normal render payload,
- collision remains on a safe CPU path until the render redesign is proven.

## Acceptance Targets

- Fullscreen or Retina size must no longer cause a multi-x jump in desired render chunks.
- Near-player terrain detail should stay visually close to the current runtime.
- The render path should move from CPU-built mesh payloads toward compact GPU-consumed tiles.
- Collision should remain on a safe CPU path until the render redesign is proven.
- The default acceptance benchmark remains the existing profiling harness and the default plus `300 km` scenes.

## Ordered Phase Map

1. Phase 01 replaces fullscreen-sensitive selection with a reference-height, best-first, capped selector while leaving the current render payload path in place.
2. Phase 02 changes residency and commit control so render work is bounded around stable per-key entries before any GPU render cutover.
3. Phase 03 introduces compact render tile payloads and explicit render-vs-collision payload separation without changing the default render backend yet.
4. Phase 04 activates shared canonical meshes plus shader-driven displacement using the tile payloads while collision stays on the CPU path.
5. Phase 05 makes the GPU tile path the default render path and removes CPU mesh generation from the render hot path.
6. Phase 06 hardens metrics, removes temporary shipped-path exceptions, and closes the acceptance benchmark.

## Why This Order

- Phase 01 comes first because the current viewport-height-driven selector is the control-plane bug that makes fullscreen ask for more terrain.
- Phase 02 comes before any GPU work because a new payload format does not help if residency and commit scheduling are still unbounded.
- Phase 03 introduces compact payloads without forcing a render cutover, so tile generation and seam correctness can be validated in isolation.
- Phase 04 switches the render backend once compact payloads already exist and are measurable.
- Phase 05 makes the new path the default only after it is already working.
- Phase 06 is for removal of transitional behavior and final acceptance, not for discovering core architecture problems.

## Advancement Rules

- Do not start a phase until the previous phase is working and passes its validation gates.
- If a later phase reveals a missing prerequisite, stop and fix the prerequisite instead of patching around it.
- If implementation diverges from the plan, record the new reality in the active phase file immediately.

## Shared Runtime Invariants

- Keep the current canonical chunk topology and stitch-mask model.
- Keep neighbor `delta <= 1` normalization in every selection/render transition.
- Keep render and physics residency explicitly separate once payload separation begins.
- Keep commit and upload budgeting explicit through the entire refactor.
- Prefer stable reuse of meshes, materials, textures, and instance RIDs over create/free churn.
- Keep the current profiling harness usable throughout the refactor, even if scenarios or counters are extended.

## Documentation To Verify Before Coding

- [Optimization using Servers - Godot docs (stable)](https://docs.godotengine.org/en/stable/tutorials/performance/using_servers.html)
- [RenderingServer - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_renderingserver.html)
- [ShaderMaterial - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_shadermaterial.html)
- [ImageTexture - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_imagetexture.html)
- [GeometryInstance3D - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_geometryinstance3d.html)
- [godot-rust built-in types](https://godot-rust.github.io/book/godot-api/builtins.html)
- [godot-rust `ImageTexture` docs](https://godot-rust.github.io/docs/gdext/master/godot/classes/struct.ImageTexture.html)
- [godot-rust `ShaderMaterial` docs](https://godot-rust.github.io/docs/gdext/master/godot/classes/struct.ShaderMaterial.html)

## Refactor Checklist

- [x] Phase 01 completed.
- [x] Phase 02 completed.
- [x] Phase 03 completed.
- [x] Phase 04 completed.
- [x] Phase 05 completed.
- [x] Phase 06 completed.
- [x] Small window vs fullscreen chunk counts settle within the target tolerance.
- [x] Fullscreen no longer causes a multi-x deferred render commit spike.
- [ ] Near-player visual detail remains acceptable.
- [x] Collision behavior remains correct through the render refactor.
- [x] Default scene validation completed.
- [x] `300 km` scene validation completed.
- [x] Profiling harness outputs updated to reflect final runtime behavior.
- [x] Runtime docs match shipped behavior.

## Reusable Test Matrix

- Small window and Retina fullscreen at the same camera.
- Default scene and `300 km` scene.
- Steady hover, fast traversal, orbit-to-surface descent.
- With normal atmosphere path and with atmosphere disabled when needed for separation.
- Rust tests, headless Godot boot, and scripted profiling probe.

## Phase 0 Deliverables

- Define the target runtime shape.
- Define the implementation order.
- Define the refactor-wide acceptance checklist.
- Keep all implementation work in later phases.
