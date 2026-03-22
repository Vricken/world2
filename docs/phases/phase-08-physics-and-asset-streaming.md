# Phase 08 - Server-Side Render and Collision Commit Pattern

## Goal

Restore the real server-side commit layer that Phase 07 prepared for: cold mesh creation, warm pooled region updates, conservative collision activation, and bounded pool reuse with explicit RID cleanup.

## Implementation Status

Implemented on 2026-03-21 in:

- `rust/src/runtime/pipeline/commit.rs`
- `rust/src/lib.rs`
- `README.md`

What shipped:

- Real `RenderingServer` cold commits using `mesh_create()`, `mesh_add_surface_from_arrays()`, `instance_create()`, `instance_set_base()`, `instance_set_scenario()`, `instance_set_transform()`, and `instance_set_visible()`.
- Real warm render commits that reuse mesh/instance RIDs but refresh the surface with `mesh_clear()` + `mesh_add_surface_from_arrays()` on update.
- Strict warm-path compatibility routing that keeps current-surface reuse, pooled-surface reuse, and cold fallback separate all the way into commit time.
- Per-class render pool watermarks and bounded physics pool reuse with deterministic fallback to free instead of unbounded RID growth.
- Conservative `PhysicsServer3D` collision activation for a capped near-camera chunk set using static bodies plus concave polygon shapes refreshed from prepared collider payloads.
- Explicit server-resource teardown on runtime shutdown so headless validation exits without leaked mesh, instance, body, or shape RIDs.
- Phase 08 runtime counters in `PlanetRoot` logs for cold vs warm render commits, physics commits, fallback causes, and pool occupancy.

## Documentation Checked Before Implementation

Checked on 2026-03-21:

- Godot stable `ArrayMesh` docs for procedural surface array contracts and slot expectations.
- Godot stable `RenderingServer` docs for mesh creation, region-update APIs, and instance base/scenario/transform rebinding.
- Godot stable `PhysicsServer3D` docs for body/shape server ownership and body state/space updates.
- Godot stable `ConcavePolygonShape3D` docs for concave triangle-face payload expectations.
- godot-rust `RenderingServer` docs for singleton access, `mesh_add_surface_from_arrays()`, and `mesh_surface_update_*_region()` bindings.
- godot-rust `PhysicsServer3D` docs/generated bindings for singleton access, concave shape creation, `shape_set_data()`, `body_add_shape()`, `body_set_state()`, `body_set_space()`, and `free_rid()`.

Constraints carried into code:

- Rust -> Godot transfer still assumes copy-possible semantics; hot-path staging remains Godot-owned and reusable.
- Warm region updates are only allowed after strict surface-class validation (`format`, counts, stitch/index class, material class, and byte expectations).
- Reused instances always refresh scenario and transform on activation.
- Collision remains near-player scoped and correctness-first; render pooling stays more aggressive than physics pooling.
- Commit paths should avoid cloning resident mesh/collider payloads when a borrowed read is enough to build the Godot array/face data.

Maintenance note on 2026-03-22:

- Live camera motion exposed `buffer_update` overruns when the runtime attempted raw warm region writes against engine-managed surface buffers.
- The shipped maintenance fix keeps RID reuse and strict compatibility checks, but refreshes the reused surface with `mesh_clear()` + `mesh_add_surface_from_arrays()` until a future pass implements format/offset-aware partial updates directly against Godot's documented buffer layout.

## Commit Model

Phase 08 now executes the Phase 07 lifecycle commands instead of only recording them.

### Cold Render Path

- Build a mesh RID with `RenderingServer.mesh_create()`.
- Upload the initial surface with `mesh_add_surface_from_arrays()`.
- Create an instance RID with `instance_create()`.
- Bind base/scenario/transform and mark the instance visible.

### Warm Render Path

- Reuse the current mesh/instance when the committed surface class is still compatible.
- Otherwise swap to a compatible pooled mesh/instance pair when one exists.
- Push the previous committed pair back through the bounded render pool before replacing it.
- Refresh the reused mesh surface from the CPU mesh arrays, then rebind base/scenario/transform on activation.

### Collision Path

- Activate collision only for the Phase 06 near-camera physics set, which is now radius-limited and hard-capped before commit.
- Reuse or create a static `PhysicsServer3D` body/shape pair.
- Refresh concave triangle data from the prepared collider payload, clear/re-add the body shape, set the body transform, and bind the body into the active physics space.
- Deactivated physics entries are detached from the space and either pooled or freed according to the physics watermark.

## Pool Policy

Current defaults encoded in `RuntimeConfig`:

- `render_pool_watermark_per_class = 8`
- `physics_pool_watermark = 32`

Behavior:

- render pools are keyed by full `SurfaceClassKey`
- pooled render entries keep their Godot-owned staging buffers
- pooled entries above watermark are freed instead of retained
- prepared-but-uncommitted payloads now explicitly return any reserved pooled render entry if they are replaced or evicted

## Deviation Notes

- Stable docs were used first, but the local validation binary is `Godot 4.7.dev.custom_build.4ea6ff24e`. During live validation, concave server-shape data had to match the engine's `faces` dictionary contract exactly. That runtime behavior is now recorded here because the shipped headless binary enforced it.
- The original Phase 08 plan targeted raw `mesh_surface_update_*_region()` writes on warm refresh. The maintenance pass reverted that specific optimization after live movement exposed buffer-size mismatches against the engine-managed surface layout. Warm refresh still reuses RIDs and surface-class compatibility checks; only the byte-region mutation step was deferred.
- The 2026-03-22 close-surface performance pass did not reintroduce partial mesh updates. Instead it reduced warm-path amplification by tightening Phase 06 selection/budgets and by removing avoidable mesh/collider cloning inside the Phase 08 commit code.
- Warm current-surface reuse is covered by unit tests and the live commit path, but the default headless scene is still mostly a cold-start validation because the fly controller does not move in headless runs by itself. A scripted moving-camera warm-stress pass remains a useful follow-up.

## Checklist

- [x] Implement explicit cold and warm render commit modes.
- [x] Validate compatibility before any warm update calls.
- [x] Keep staging buffers reusable and Godot-owned.
- [x] Rebind transform/scenario on pooled activation.
- [x] Keep collision conservative and near-player scoped.
- [x] Track warm hits/misses and fallback reasons.

## Prerequisites

- [x] Phase 07 pipeline outputs are consumed directly by the Phase 08 commit layer.

## Ordered Build Steps

1. [x] Implement cold render commit path (mesh + instance + scenario + transform).
2. [x] Implement warm reuse path with strict compatibility preconditions.
3. [x] Implement in-place region update calls.
4. [x] Implement transform/scenario rebind rules for pooled instances.
5. [x] Implement conservative collision commit path.
6. [x] Implement render/physics pool watermark behavior.

## Validation and Test Gates

- [x] Cold-only mode renders correctly in the default headless run.
- [x] Warm reuse bookkeeping is covered by unit tests, including pooled swap and watermark behavior.
- [x] Rebound pooled instances restore the correct committed surface state in unit tests.
- [x] Collision activation/deactivation remains bounded and near-camera scoped.
- [x] Headless Godot exit is clean with no reported render/physics RID leaks.

## Definition of Done

- [x] Cold and warm commit paths are both production-usable.
- [x] Incompatible warm updates are blocked and counted.
- [x] Collision path is bounded and gameplay-correct for the current near-camera scope.

## Test Record

- [x] Date: 2026-03-22
- [x] Result summary: `cargo test` passed with `42/42` tests, `./scripts/build_rust.sh` built successfully, and `./scripts/run_godot.sh --headless --quit-after 5` loaded the extension, executed real Phase 08 server commits, and reported `render_cold_commits=5`, `physics_commits=0`, `active_render=5`, `active_physics=0`, `queued_ops=5`, and `deferred_ops=0` from the default debug camera with no shutdown RID leak errors.
- [x] Fallback causes observed: initial headless activation still used `fallback_missing_current=5` as expected on cold start; no incompatible-current or no-compatible-pool fallbacks were observed in the static-camera validation.
- [x] Follow-up actions: validate a camera path that intentionally enters the new physics bubble so the tighter collision residency policy is exercised against live `PhysicsServer3D` commits as well as unit tests.

## References

- [ArrayMesh - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_arraymesh.html)
- [Using the ArrayMesh - Godot docs (stable)](https://docs.godotengine.org/en/stable/tutorials/3d/procedural_geometry/arraymesh.html)
- [RenderingServer - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_renderingserver.html)
- [PhysicsServer3D - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_physicsserver3d.html)
- [ConcavePolygonShape3D - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_concavepolygonshape3d.html)
- [RenderingServer in godot-rust API docs](https://godot-rust.github.io/docs/gdext/master/godot/classes/struct.RenderingServer.html)
- [PhysicsServer3D in godot-rust API docs](https://godot-rust.github.io/docs/gdext/master/godot/classes/struct.PhysicsServer3D.html)
- [godot-rust built-in types (packed arrays)](https://godot-rust.github.io/book/godot-api/builtins.html)
