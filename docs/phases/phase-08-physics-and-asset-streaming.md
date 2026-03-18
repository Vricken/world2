# Phase 08 - Server-Side Render and Collision Commit Pattern

## Goal

Restore the full cold/warm commit architecture, explicit FFI boundary rules, pool policy, and conservative collision guidance.

## Commit Model

Use Godot procedural mesh APIs and attach to `RenderingServer` instances rather than per-chunk `MeshInstance3D` nodes.

```rust
enum RenderCommitMode {
    ColdCreate,
    WarmReuseRegionUpdate,
}

fn create_render_chunk_cold(...) -> ChunkRenderState {
    // 1. Build mesh resource.
    //    Option A: ArrayMesh.add_surface_from_arrays()
    //    Option B: RenderingServer.mesh_add_surface_from_arrays()
    // 2. Create render instance RID.
    // 3. instance_set_base(instance_rid, mesh_rid)
    // 4. instance_set_scenario(instance_rid, scenario_rid)
    // 5. instance_set_transform(instance_rid, transform)
    // 6. return both RIDs
}

fn update_render_chunk_warm(...) {
    // Preconditions:
    // - pooled entry class matches
    // - capacities match expected class
    // - material/shader contract matches
    //
    // 1. mesh_surface_update_vertex_region(...)
    // 2. mesh_surface_update_attribute_region(...)
    // 3. mesh_surface_update_index_region(...) if needed
    // 4. instance_set_base(...) if rebinding required
    // 5. instance_set_scenario(...)
    // 6. instance_set_transform(...)
}
```

## Cold Path

Use when no compatible pool entry exists, class changed incompatibly, startup warming is in progress, or deliberate surface rebuild is needed. Keep cold path simple and correctness-first.

## Warm Path

Use only when pooled mesh/instance pair is compatible:

- matching surface format
- matching vertex count
- compatible index/stitch class
- matching material contract

Warm path should prefer byte-region updates plus reusable Godot staging buffers filled in place.

## Explicit FFI Boundary Rule

Treat Rust <-> Godot boundary as copy-possible unless docs explicitly guarantee otherwise.

- do not architect around undocumented zero-copy ownership transfer
- keep hot-path packed buffers Godot-owned and reusable
- fill buffers in place

## Pool Policy

Minimum key dimensions:

- `format_mask`
- `vertex_count`
- `index_count`
- `stitch_mask` (or reduced stitch/index class)
- `material_class`

Pool entries should also carry:

- reusable `PackedByteArray` staging buffers
- byte-count expectations per region
- optional metadata for partial updates

## Transform and Scenario Rebinding

Reused instances must refresh scenario and transform each activation (`instance_set_scenario()`, `instance_set_transform()`).

## Collision Path

Collision remains conservative and separate:

```rust
fn create_or_update_physics_chunk(...) {
    // 1. Create or fetch pooled body/shape if policy allows.
    // 2. Fill or replace shape data.
    // 3. body_add_shape(...)
    // 4. body_set_state(... transform ...)
    // 5. body_set_space(...)
}
```

Prefer near-player, coarse correctness-first collision residency. Render pooling is usually a clearer win than aggressive physics pooling.

## Checklist

- [ ] Implement explicit cold and warm render commit modes.
- [ ] Validate compatibility before any warm update calls.
- [ ] Keep staging buffers reusable and Godot-owned.
- [ ] Rebind transform/scenario on pooled activation.
- [ ] Keep collision conservative and near-player scoped.
- [ ] Track warm hits/misses and fallback reasons.

## Prerequisites

- [ ] Phase 07 pipeline outputs available for cold and warm commit modes.

## Ordered Build Steps

1. [ ] Implement cold render commit path (mesh+instance+scenario+transform).
2. [ ] Implement warm reuse path with strict compatibility preconditions.
3. [ ] Implement in-place region update calls.
4. [ ] Implement transform/scenario rebind rules for pooled instances.
5. [ ] Implement conservative collision commit path.
6. [ ] Implement render/physics pool watermark behavior.

## Validation and Test Gates

- [ ] Cold-only mode renders correctly.
- [ ] Warm reuse mode updates without corruption.
- [ ] Rebound pooled instances appear in correct scenario and position.
- [ ] Collision activation/deactivation works for near-player chunks.

## Definition of Done

- [ ] Cold and warm commit paths are both production-usable.
- [ ] Incompatible warm updates are blocked and counted.
- [ ] Collision path is bounded and gameplay-correct.

## Test Record (Fill In)

- [ ] Date:
- [ ] Result summary:
- [ ] Fallback causes observed:
- [ ] Follow-up actions:

## References

- [ArrayMesh - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_arraymesh.html)
- [RenderingServer - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_renderingserver.html)
- [ConcavePolygonShape3D - Godot docs](https://docs.godotengine.org/en/stable/classes/class_concavepolygonshape3d.html)
