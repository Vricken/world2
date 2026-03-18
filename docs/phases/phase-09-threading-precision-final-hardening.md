# Phase 09 - Threading Model in godot-rust

## Goal

Restore complete worker/commit threading guidance, including modes, synchronization rules, and reusable worker scratch policies.

## Runtime Command Shape

```rust
pub enum PlanetCommand {
    CreateOrUpdateRenderChunk {
        key: ChunkKey,
        payload: ChunkPayload,
        transform: Transform3D,
    },
    RemoveRenderChunk {
        key: ChunkKey,
    },
    CreateOrUpdatePhysicsChunk {
        key: ChunkKey,
        collider_vertices: Vec<[f32; 3]>,
        collider_indices: Vec<i32>,
        transform: Transform3D,
    },
    RemovePhysicsChunk {
        key: ChunkKey,
    },
    UpdateAssets {
        key: ChunkKey,
        instances: Vec<AssetInstance>,
    },
}
```

## Primary Threading Contract

- workers: pure Rust tasks, `f64` math, no scene-tree mutation
- commit side: server-oriented resource/object updates
- handoff: lock-free queue or double-buffered command list

Godot thread-safety docs indicate global-scope singletons are thread-safe by default, and server access from threads is supported when project settings are configured appropriately.

## Mode A: Safer Default

- workers generate plain Rust buffers and optional Rust-packed regions
- one commit lane performs all Godot server calls in controlled batches
- commit lane owns staging buffers and fills via `as_mut_slice()`
- no worker touches scene tree

This should be the baseline implementation mode.

Mode B is cancelled ignore any reference to it.

## Warm-Path Synchronization Rules

- workers may warm-update only against prevalidated compatible classes
- pool manager must remain synchronized and deterministic
- no mutable staging buffer sharing across simultaneous commits

Practical ownership models:

1. per-worker Rust scratch + single commit-lane Godot staging
2. isolated per-worker Godot staging sets by handled class

## Worker Allocation Policy

- avoid fresh large `Vec` allocation per job when possible
- keep reusable per-worker `CpuMeshBuffers`
- keep reusable per-worker byte-packing scratch
- reset/refill instead of reconstructing large buffers
- convert to Godot staging only at final commit boundary

## Checklist

- [ ] Implement deterministic command handoff.
- [ ] Enforce no scene-tree mutation in workers.
- [ ] Keep Mode A as required shipping path.
- [ ] Add synchronization guardrails and feature gating for any Mode B usage.
- [ ] Reuse worker scratch buffers for mesh/packing jobs.
- [ ] Capture queue, contention, and allocation metrics.

## Prerequisites

- [ ] Phase 08 commit paths and runtime command model completed.

## Ordered Build Steps

1. [ ] Implement worker->commit command handoff.
2. [ ] Implement Mode A as baseline.
3. [ ] Enforce no scene-tree mutation in workers.
4. [ ] Implement warm-path class validation and synchronized pool access.
5. [ ] Implement worker scratch reuse for mesh and packing data.
6. [ ] Add optional Mode B feature gate behind profiling and safety checks (default off).

## Validation and Test Gates

- [ ] Queue integrity stress test passes.
- [ ] Determinism test passes for fixed seed and camera path.
- [ ] Allocation profile improves after scratch reuse.
- [ ] No mutable staging sharing race under concurrent commits.

## Definition of Done

- [ ] Worker/commit responsibilities are clean and enforceable.
- [ ] Mode A is stable under load.
- [ ] Mode B is either disabled or validated as safe and beneficial.
- [ ] Threading metrics support tuning decisions.

## Test Record (Fill In)

- [ ] Date:
- [ ] Result summary:
- [ ] Mode tested:
- [ ] Follow-up actions:

## References

- [Thread-safe APIs - Godot docs](https://docs.godotengine.org/en/latest/tutorials/performance/thread_safe_apis.html)
- [godot-rust built-in types (packed arrays)](https://godot-rust.github.io/book/godot-api/builtins.html)
