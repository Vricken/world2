# PhysX + Building Port Plan

## Purpose

Define the implementation plan to merge `world2`, `building`, and the custom `godot_physx` module into one runtime that:

- keeps `world2`'s streamed planet renderer and chunk-selection pipeline,
- uses the custom PhysX stack as the shared gameplay physics world,
- lets `building` structures, build queries, and player movement operate against the streamed planet terrain,
- avoids introducing per-chunk scene-tree collision nodes for planet terrain.

This document is intended to be the execution plan, not just a brainstorming note.

## Why This Port Exists

`building` already assumes a custom PhysX-centered gameplay stack:

- `PhysXWorld3D` owns the active simulation world.
- `PhysXCharacter3D` / `ProtoController` own player movement.
- build queries go through the PhysX query path first.
- structures are authored around PhysX bodies and joints.

Relevant local references:

- `building` controller: `/Users/rp/Documents/Coding/Space Explorers/engines/building/addons/proto_controller/proto_controller.gd`
- `building` query bridge: `/Users/rp/Documents/Coding/Space Explorers/engines/building/scripts/physics/physics_query_bridge.gd`
- `building` overview: `/Users/rp/Documents/Coding/Space Explorers/engines/building/docs/overview.md`
- `godot_physx` runtime overview: `/Users/rp/Documents/Coding/Space Explorers/engines/godot/modules/godot_physx/docs/README.md`

`world2` currently assumes Godot's built-in `World3D` and `PhysicsServer3D` for terrain collision residency:

- chunk collision is committed through `PhysicsServer3D` static body + concave shape RIDs,
- `PlanetRoot` caches the built-in physics-space RID from `World3D`,
- the sample scene still uses a built-in `CharacterBody3D` controller.

Relevant local references:

- plan constraint: `/Users/rp/Documents/Coding/Space Explorers/engines/world2/bigplan.md`
- collision commit path: `/Users/rp/Documents/Coding/Space Explorers/engines/world2/rust/src/runtime/pipeline/commit.rs`
- world RID caching and origin logic: `/Users/rp/Documents/Coding/Space Explorers/engines/world2/rust/src/lib.rs`
- current scene/controller: `/Users/rp/Documents/Coding/Space Explorers/engines/world2/scenes/main.tscn`

Because the final merged product needs one gameplay physics world, the long-term target should be PhysX, not a permanent dual-physics setup.

## Checked Constraints

### Godot docs checked

- Using servers is valid and can be the right tool when scene-tree overhead is the bottleneck.
- `PhysicsServer3D` is the low-level built-in physics server that `world2` currently targets.
- large-world/origin management remains an explicit concern and cannot be ignored when moving collision systems.

References:

- https://docs.godotengine.org/en/stable/tutorials/performance/using_servers.html
- https://docs.godotengine.org/en/stable/classes/class_physicsserver3d.html
- https://docs.godotengine.org/en/stable/tutorials/physics/large_world_coordinates.html

### godot-rust constraints checked

- The current project uses upstream `gdext` from `master`.
- Typed bindings for custom engine classes should not be assumed to exist automatically.
- Runtime access to custom engine classes can still be staged through dynamic `ClassDb` / `Object.call(...)` usage before any binding-regeneration work.

Local references:

- dependency declaration: `/Users/rp/Documents/Coding/Space Explorers/engines/world2/rust/Cargo.toml`
- local gdext `ClassDb` test usage: `/Users/rp/.cargo/git/checkouts/gdext-067f4b88e7bd088f/2417d4a/itest/rust/src/engine_tests/classdb_test.rs`

### Local architecture constraints checked

- `world2` explicitly forbids per-chunk terrain or collision nodes for the planet runtime.
- `building` already has adapters and backend seams around movement, queries, and physics actors.
- `godot_physx` already supports static triangle mesh and static heightfield shapes, but its current public workflow is node/resource-oriented.

Local references:

- no per-chunk nodes: `/Users/rp/Documents/Coding/Space Explorers/engines/world2/bigplan.md`
- backend seam in structures: `/Users/rp/Documents/Coding/Space Explorers/engines/building/scripts/structure/common/structure.gd`
- PhysX shape resources: `/Users/rp/Documents/Coding/Space Explorers/engines/godot/modules/godot_physx/shapes/physx_shape_resource_3d.h`

## Recommended Target Architecture

### Core decision

Do not port planet collision by instantiating one `PhysXStaticBody3D` + `PhysXShape3D` scene subtree per active terrain chunk.

Instead:

1. Keep `world2` as the authority for terrain selection, payload generation, residency, and origin-aware transforms.
2. Add a runtime chunk-terrain API inside `godot_physx`.
3. Make `world2`'s collision backend target either:
   - the current built-in `PhysicsServer3D` backend, or
   - a new PhysX terrain backend.
4. Merge scenes so gameplay lives under one `PhysXWorld3D`.

### End-state runtime model

- `PhysXWorld3D` is the shared gameplay physics world.
- `PlanetRoot` remains the streamed terrain/render owner.
- `PlanetRoot` no longer commits planet collision into Godot Physics in the final path.
- `PlanetRoot` commits planet collision into the PhysX terrain backend owned by `PhysXWorld3D`.
- `building` structures continue to use PhysX bodies/joints.
- the player uses `PhysXCharacter3D` / `ProtoController` or a direct descendant of that stack.
- build raycasts and overlap queries see planet terrain through the PhysX query path.

### Transition rule

Temporary dual-physics support is allowed only as a migration aid. It is not the target architecture.

That means:

- temporary mirrored planet collision in both built-in physics and PhysX is acceptable for short validation windows,
- long-term gameplay must not depend on built-in physics for terrain interaction.

## Port Strategy

Implement the port in phases that preserve forward progress and keep each phase testable.

### Phase 0 - Freeze the integration direction

Goal:

- align all three codebases on one explicit direction before implementation churn starts.

Tasks:

- confirm that the merged game root will be `PhysXWorld3D`-based.
- confirm that `PlanetRoot` will become a child or descendant of `PhysXWorld3D`.
- confirm that the player/controller path will ultimately move onto `PhysXCharacter3D`.
- confirm that the initial terrain collision backend will use triangle meshes, not heightfields.

Deliverables:

- this plan document,
- any follow-up architecture note if scene ownership changes from current assumptions.

Exit criteria:

- no one is still assuming the final merged product keeps gameplay on built-in Godot physics.

### Phase 1 - Introduce a collision backend seam inside `world2`

Goal:

- isolate the planet collision commit path so PhysX can be added without destabilizing render streaming.

Tasks:

- define an internal collision backend abstraction in `world2`.
- move direct `PhysicsServer3D` calls behind that abstraction.
- keep the existing built-in backend as the first implementation.
- ensure origin-rebind and shutdown paths also route through the abstraction.

Suggested touchpoints:

- `/Users/rp/Documents/Coding/Space Explorers/engines/world2/rust/src/runtime/pipeline/commit.rs`
- `/Users/rp/Documents/Coding/Space Explorers/engines/world2/rust/src/runtime/data.rs`
- `/Users/rp/Documents/Coding/Space Explorers/engines/world2/rust/src/lib.rs`

Notes:

- this phase should not change terrain selection logic,
- this phase should not change render residency logic,
- this phase should not yet require `PhysXWorld3D`.

Exit criteria:

- `world2` can swap collision commit implementations without touching selection/payload generation logic.

### Phase 2 - Add a runtime terrain API to `godot_physx`

Goal:

- give `world2` a no-scene-node way to stream chunk colliders into the PhysX scene.

Tasks:

- add a terrain/chunk manager owned by `PhysXWorld3D`, or add equivalent methods directly on `PhysXWorld3D`.
- support stable chunk IDs so `world2` can update, detach, recycle, and remove chunk actors deterministically.
- support triangle-mesh chunk submission from raw arrays or a minimal resource-free payload form.
- support transform updates independent from mesh recooking when possible.
- support query-visible and simulation-visible flags, layer/mask configuration, and debug visualization.
- support bounded pooling or actor reuse for chunk churn.

Recommended API shape:

- `begin_terrain_batch()`
- `update_terrain_chunk_triangle_mesh(chunk_id, faces_or_vertices_indices, transform, layer, mask, metadata)`
- `remove_terrain_chunk(chunk_id)`
- `end_terrain_batch()`
- `clear_all_terrain_chunks()`

Possible implementation homes:

- extend `PhysXWorld3D`, or
- add a dedicated terrain helper owned by `PhysXWorld3D`.

Do not:

- require one `PhysXStaticBody3D` node per chunk,
- require one `PhysXShape3D` child per chunk,
- force `world2` to construct scene-tree authoring nodes to drive runtime terrain.

Suggested touchpoints:

- `/Users/rp/Documents/Coding/Space Explorers/engines/godot/modules/godot_physx/world/physx_world_3d.h`
- `/Users/rp/Documents/Coding/Space Explorers/engines/godot/modules/godot_physx/world/physx_world_3d.cpp`
- new terrain-specific files under `godot_physx` if the logic becomes large

Exit criteria:

- a test scene can stream a handful of static triangle-mesh chunks into `PhysXWorld3D` without scene-node chunk bodies.

### Phase 3 - Wire `world2` to the PhysX terrain backend

Goal:

- make the planet runtime drive the new PhysX terrain path using its current payloads.

Tasks:

- resolve a `PhysXWorld3D` handle from `PlanetRoot` instead of only caching built-in `World3D` RIDs.
- add a PhysX collision backend implementation to `world2`.
- feed current `collider_faces` payloads into the PhysX terrain API.
- mirror chunk activation/deactivation, pooling, and transform rebind logic into the PhysX backend.
- keep render logic unchanged.

Important design choice:

- the initial backend should consume the already-prepared triangle payloads.
- do not block this phase on inventing a new heightfield payload format.

Suggested touchpoints:

- `/Users/rp/Documents/Coding/Space Explorers/engines/world2/rust/src/runtime/workers/payloads.rs`
- `/Users/rp/Documents/Coding/Space Explorers/engines/world2/rust/src/runtime/pipeline/commit.rs`
- `/Users/rp/Documents/Coding/Space Explorers/engines/world2/rust/src/lib.rs`

godot-rust integration note:

- first pass can use dynamic class lookup / dynamic method calls if that is the fastest path,
- a later cleanup can add better typed wrappers or generated bindings if the custom engine API stabilizes.

Exit criteria:

- streamed planet chunks appear in PhysX queries and contact against PhysX actors in a focused validation scene.

### Phase 4 - Merge the scene root and player stack

Goal:

- get the actual merged gameplay scene onto one world model.

Tasks:

- make the merged root scene own a `PhysXWorld3D`.
- place `PlanetRoot`, the player, and `building` runtime systems under that world.
- replace the `world2` sample `CharacterBody3D` path with the PhysX controller stack used by `building`.
- migrate any assumptions in `PlanetRoot` that currently inspect `CharacterBody3D` collision state for origin-shift deferral.

Important concern:

`world2` currently defers origin shifts when the camera's owning `CharacterBody3D` is colliding. That logic must be redefined for the PhysX controller path rather than silently removed.

Suggested touchpoints:

- `/Users/rp/Documents/Coding/Space Explorers/engines/world2/scenes/main.tscn`
- `/Users/rp/Documents/Coding/Space Explorers/engines/world2/scripts/player/fly_controller.gd`
- `/Users/rp/Documents/Coding/Space Explorers/engines/world2/rust/src/lib.rs`
- `/Users/rp/Documents/Coding/Space Explorers/engines/building/addons/proto_controller/proto_controller.gd`

Exit criteria:

- the merged player can walk/fly on planet terrain through PhysX-only collision.

### Phase 5 - Unify gameplay queries against the planet

Goal:

- ensure all gameplay query systems can see the streamed planet.

Tasks:

- validate `PhysicsQueryBridge` raycasts against planet chunks in `PhysXWorld3D`.
- validate `PhysicsQueryBridge.intersect_box()` against planet chunks.
- ensure build placement, overlap checks, and grounding checks behave correctly on curved terrain.
- decide whether the temporary Godot direct-space-state fallback remains enabled during migration.

Suggested touchpoints:

- `/Users/rp/Documents/Coding/Space Explorers/engines/building/scripts/physics/physics_query_bridge.gd`
- `/Users/rp/Documents/Coding/Space Explorers/engines/building/scripts/structure/common/structure_overlap_queries.gd`
- `/Users/rp/Documents/Coding/Space Explorers/engines/building/scripts/build/build_controller_utils.gd`

Exit criteria:

- building placement and structure overlap logic can operate on the planet surface using the merged runtime.

### Phase 6 - Handle origin shifting and precision explicitly

Goal:

- make the merged PhysX path safe under large-world movement.

Tasks:

- define whether the PhysX terrain world follows `PlanetRoot` rebases or receives equivalent actor/controller transform updates.
- ensure terrain chunk transforms, structure transforms, and controller state remain coherent across rebases.
- validate that controller grounding and structure joints do not explode on rebase events.
- add instrumentation for rebase count, actor rebind count, and terrain chunk transform refresh count.

Suggested touchpoints:

- `/Users/rp/Documents/Coding/Space Explorers/engines/world2/rust/src/runtime/pipeline/commit.rs`
- `/Users/rp/Documents/Coding/Space Explorers/engines/world2/rust/src/lib.rs`
- `godot_physx` terrain backend files from Phase 2

Exit criteria:

- repeated long-distance traversal does not cause terrain/query/controller drift or contact instability.

### Phase 7 - Performance pass on triangle-mesh terrain

Goal:

- make the merged path shippable before attempting a heightfield backend.

Tasks:

- measure triangle-mesh cooking cost for chunk activation churn.
- measure query cost against active terrain chunks.
- measure controller behavior near chunk boundaries.
- add or tune chunk pooling/reuse in the PhysX terrain backend.
- tune activation radius / active chunk caps to keep near-player collision conservative.

Do not:

- switch to heightfields before the triangle backend has real profiling numbers,
- over-pool if profiling shows mesh cooking is not the dominant cost,
- assume PhysX triangle meshes are automatically cheap enough for over-eager residency.

Exit criteria:

- the merged game can run a representative traversal/build session without obvious collision hitches.

### Phase 8 - Optional heightfield backend evaluation

Goal:

- decide whether planet terrain should stay on triangle meshes or gain a heightfield path.

Why this is optional:

- `world2` already prepares triangle collision payloads,
- the custom PhysX module already supports heightfields,
- but `world2` does not currently persist a heightfield-ready payload and its chunk representation is not yet committed to a PhysX heightfield contract.

Tasks:

- define the per-chunk local frame required for a PhysX heightfield representation.
- expose height samples, row/column scale, and height scale from `world2` payload generation.
- test seam correctness and query/controller behavior across neighboring chunk heightfields.
- compare memory, cook time, and runtime query/contact cost against the triangle backend.

Decision rule:

- keep triangle meshes if they are good enough,
- only switch if heightfields produce a clear measured win without causing seam or controller regressions.

Exit criteria:

- there is a written go/no-go decision backed by profiling and correctness checks.

## Detailed Workstreams

### Workstream A - `world2` backend abstraction

Owner focus:

- make terrain collision a pluggable backend instead of a hardcoded built-in physics commitment.

Required outcomes:

- backend trait or equivalent dispatch,
- unified activation/deactivation/update/rebind/shutdown flow,
- backend-specific stats exposed in logs.

### Workstream B - `godot_physx` runtime terrain backend

Owner focus:

- ingest streamed chunk data efficiently inside PhysX.

Required outcomes:

- chunk-ID keyed actor lifetime management,
- mesh cooking/update path,
- query visibility,
- debug instrumentation,
- explicit cleanup.

### Workstream C - merged world ownership

Owner focus:

- ensure there is exactly one gameplay physics authority in the merged scene.

Required outcomes:

- `PhysXWorld3D`-rooted scene,
- `PlanetRoot` integration,
- player/controller migration,
- removal of assumptions that gameplay terrain lives in built-in physics.

### Workstream D - query/build integration

Owner focus:

- make existing build tooling work unchanged or nearly unchanged on the merged planet runtime.

Required outcomes:

- raycasts hit terrain,
- overlap checks see terrain and structures consistently,
- build placement respects curved world space.

### Workstream E - precision/origin handling

Owner focus:

- prevent large-world correctness regressions.

Required outcomes:

- safe rebase policy for terrain + controller + structures,
- deterministic transform refresh points,
- validation scenes and logs for long traversal.

## Explicit Non-Goals

- Do not port planet terrain by creating one authored scene-tree physics body per chunk.
- Do not maintain a permanent dual-physics gameplay setup where `building` uses PhysX and the planet uses built-in Godot physics.
- Do not block the first merged milestone on a heightfield backend.
- Do not rewrite `world2`'s render streaming just because collision is moving to PhysX.

## Risks

### Risk 1 - Hidden dual-world assumptions

Symptoms:

- some systems query PhysX while others still query built-in `World3D`,
- objects appear to collide in one subsystem and ghost through in another.

Mitigation:

- explicitly choose the physics authority per phase,
- add logging around query source and active backend,
- make the transition window short.

### Risk 2 - Origin-rebase instability

Symptoms:

- controller loses floor state after rebase,
- structures jitter against planet terrain,
- chunk transforms drift from render.

Mitigation:

- treat rebase support as a first-class workstream, not post-polish,
- instrument rebind counts and controller state transitions.

### Risk 3 - Triangle-mesh cook churn

Symptoms:

- camera movement spikes when entering new collision chunks,
- build queries stutter near terrain updates.

Mitigation:

- start with conservative active chunk counts,
- add bounded pooling,
- only pursue heightfields if profiling proves triangle meshes insufficient.

### Risk 4 - Binding friction from custom engine classes

Symptoms:

- Rust side cannot cleanly call custom PhysX APIs with typed wrappers.

Mitigation:

- use dynamic `ClassDb` / `Object.call(...)` first,
- defer typed binding cleanup until the engine-side API settles.

## Validation Matrix

Each major phase should be considered incomplete until the relevant checks are run and recorded.

### Functional checks

- player/controller collides with streamed planet terrain,
- PhysX rigid bodies rest on planet terrain,
- build raycasts hit planet terrain,
- overlap checks near terrain/build structures behave as expected,
- terrain chunk activation/deactivation causes no collision gaps in the active bubble.

### Precision checks

- long traversal across many chunk activations,
- origin-shift stress case with active contacts,
- structure placement and movement after multiple rebases.

### Performance checks

- terrain activation hitch timing,
- PhysX query timing near dense active terrain,
- controller traversal over chunk boundaries,
- build interaction while moving over active terrain.

### Shutdown / lifetime checks

- no leaked terrain actors,
- no leaked cooked mesh resources,
- clean world teardown when reloading scenes or quitting headless.

## Documentation Updates Required During the Port

When implementation starts, update docs in the same change sets as code:

- `world2` phase docs that currently describe built-in `PhysicsServer3D` terrain collision as the shipped path,
- any `bigplan.md` sections whose wording changes from built-in server physics to backend-agnostic or PhysX-targeted collision,
- `building` docs that describe the merged scene/runtime assumptions,
- `godot_physx` docs for the new terrain runtime API.

## Suggested Execution Order

1. Phase 0
2. Phase 1
3. Phase 2
4. Phase 3
5. Focused terrain-query validation
6. Phase 4
7. Phase 5
8. Phase 6
9. Phase 7
10. Optional Phase 8

## Recommended First Milestone

The first milestone should not be "full merged gameplay."

It should be:

"A PhysXWorld3D-rooted test scene where `PlanetRoot` streams visible terrain, a PhysX controller can stand on it, and PhysX raycasts hit it."

That milestone proves the critical integration seam before spending time on building tools, structure systems, or optional heightfield work.
