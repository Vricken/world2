# Phase 01 - Recommended Project Shape

## Goal

Re-establish the full architectural intent for the runtime shell and scene shape so this phase is not checklist-only and can serve as a complete engineering reference.

## Recommended Scene Layout

Use this scene layout:

```text
PlanetRoot (Node3D / Rust GodotClass)
|- DebugRoot (Node3D, optional)
|- PlayerController (CharacterBody3D fly rig)
|  `- MainCamera / gameplay nodes
`- No per-chunk terrain or collision nodes
```

This remains the core structural decision. `PlanetRoot` is a thin shell used to own the Rust planet runtime, fetch `World3D`, cache the rendering scenario RID and physics space RID, drive the update loop, and expose debug/editor controls.

The current default gameplay node is a fly controller rig under `PlanetRoot`, not a free-standing `CameraAnchor`. That still fits the phase contract because gameplay/debug nodes remain shell-only and chunk state still lives in server RIDs rather than scene children.

Terrain chunks do not exist as `MeshInstance3D` or `StaticBody3D` nodes. Their visible state lives in `RenderingServer` resources and instance RIDs attached to a scenario, while their collision state lives in `PhysicsServer3D` body and shape RIDs attached to a physics space.

Godot docs explicitly support bypassing the scene tree this way, and also explicitly note this only helps when the scene system is actually the bottleneck.

Use a cube-sphere with 6 face quadtrees. This keeps square chunks, deterministic per-face addressing, and practical chunked LOD without pole singularities. The production default projection should be modified/spherified cube mapping, not naive normalized-cube, while still keeping projection as a swappable strategy.

The top-level Godot scene should stay intentionally small. Chunk creation, destruction, visibility, transform updates, pooling, region updates, packed staging reuse, and collision residency are all runtime responsibilities in Rust data and server RIDs.

## Why This Phase Matters

This is the architecture contract every later phase depends on:

1. Scene tree is shell-only.
2. Chunk identity is data-oriented, not node-oriented.
3. Rendering is server-managed.
4. Physics is server-managed.
5. Runtime scaling depends on visibility, active-set discipline, and bounded churn rather than scene-node count growth.

## Checklist

- [x] Keep `PlanetRoot` as orchestration shell only.
- [x] Cache scenario RID and physics space RID from `World3D`.
- [x] Keep chunk lifecycle out of scene children.
- [x] Keep projection swappable; default to spherified cube.
- [x] Keep scene tree small for gameplay/debug/editor needs only.
- [x] Record any architecture deviation in this file.

## Prerequisites

- [x] Godot and rust-godot toolchain verified.
- [x] Runtime root scene exists and loads.

## Ordered Build Steps

1. [x] Implement shell scene shape exactly.
2. [x] Wire `PlanetRoot` ownership and RID caching path.
3. [x] Enforce no per-chunk terrain/collision node ownership.
4. [x] Verify runtime loop owns chunk lifecycle orchestration.

## Validation and Test Gates

- [x] Running scene shows shell nodes only.
- [x] Scenario and physics space RIDs are valid and cached.
- [x] Server-driven test object creation works without chunk nodes.

## Definition of Done

- [x] Architecture contract is implemented, not only documented.
- [x] Phase checklist, validation gates, and deviations are all recorded.

## Test Record (Fill In)

- [x] Date: 2026-03-18
- [x] Result summary: Phase 01 scaffolding implemented with a Rust `PlanetRoot` GodotClass (`Node3D`) that caches `World3D` scenario and space RIDs, runs a shell-only tick loop, and keeps chunk identity out of scene children.
- [x] Deviations from plan: Added tracked `.godot/extension_list.cfg` to ensure the GDExtension loads consistently so the `PlanetRoot` custom node type can be instantiated from the main scene.
- [x] Follow-up actions: Begin Phase 02 data model implementation in Rust (`ChunkKey`, metadata maps, RID state, pools).

## References

- [RenderingServer - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_renderingserver.html)
- [Optimization using Servers - Godot docs](https://docs.godotengine.org/en/stable/tutorials/performance/using_servers.html)
