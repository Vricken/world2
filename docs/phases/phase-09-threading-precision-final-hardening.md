# Phase 09 - Threading Model in godot-rust

## Goal

Ship the documented Mode A worker/commit split: pure Rust worker generation, deterministic handoff back to the commit lane, synchronized warm-path ownership, and measurable scratch/queue behavior.

## Continuity From Phases 01-08

Phases 01-08 already established:

- Rust-owned chunk identity and lifecycle
- deterministic selection and payload preparation
- cold/warm server commit paths with strict class compatibility
- reusable Godot-owned staging buffers

Phase 09 preserves those contracts while adding concurrency. Threading changes how work is scheduled, not what owns chunk truth.

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

## Implemented Threading Contract

- workers: persistent Rust threads that only sample, mesh, and byte-pack render payloads
- commit side: single-lane warm/cold commit logic, RID ownership, pooled-entry routing, and Godot server calls
- handoff: mutex/condvar worker queue plus deterministic sequence numbers on the result drain

Godot thread-safety docs allow server access from threads, but the shipping implementation intentionally keeps all server and staging mutation on the commit lane. That keeps Phase 09 independent of project thread-setting changes and avoids scene-tree or GPU-adjacent work in workers.

## Mode A

- workers generate plain Rust mesh buffers plus packed byte regions
- one commit lane performs all Godot server calls in controlled batches
- commit lane owns Godot staging buffers and fills them via `as_mut_slice()`
- no worker touches scene tree, server singletons, or the active scene tree

This is the required shipping default.

## Warm-Path Synchronization Rules

- workers may warm-update only against prevalidated compatible classes
- pool manager must remain synchronized and deterministic
- no mutable staging-buffer sharing across simultaneous commits
- warm-path class selection and pooled-entry ownership stay on the commit lane

Practical ownership models:

1. per-worker Rust scratch + single commit-lane Godot staging
2. isolated per-worker Godot staging sets by handled class

The current codebase implements option `1`.

## Worker Allocation Policy

- avoid fresh large `Vec` allocation per job when possible
- keep reusable per-worker `CpuMeshBuffers`
- keep reusable per-worker byte-packing scratch
- reset/refill instead of reconstructing large buffers
- convert to Godot staging only at final commit boundary

The current worker implementation reuses sample, mesh, pack, and slope-height scratch inside each worker thread, then clones the finished data into resident payload storage owned by the runtime.

## Implementation Notes

- `PlanetRuntime` now owns a persistent `ThreadedPayloadGenerator` sized from `RuntimeConfig::worker_thread_count`.
- Payload requests are collected in deterministic key order before entering the worker queue.
- Worker results are sorted back into request sequence before any runtime state or Godot staging is touched.
- Warm-path routing (`ReuseCurrentSurface`, `ReusePooledSurface`, `ColdCreate`) remains commit-lane only.
- Physics activation remains commit-lane only and still derives collider payloads from the committed render payload.
- Mode B is not implemented. There is no worker-side server mutation path in shipping code.

## Checklist

- [x] Implement deterministic command handoff.
- [x] Enforce no scene-tree mutation in workers.
- [x] Keep Mode A as required shipping path.
- [x] Add synchronization guardrails.
- [x] Reuse worker scratch buffers for mesh/packing jobs.
- [x] Capture queue, contention, and allocation metrics.

## Prerequisites

- [x] Phase 08 commit paths and runtime command model completed.

## Ordered Build Steps

1. [x] Implement worker->commit command handoff.
2. [x] Implement Mode A as baseline.
3. [x] Enforce no scene-tree mutation in workers.
4. [x] Implement warm-path class validation and synchronized pool access.
5. [x] Implement worker scratch reuse for mesh and packing data.

## Validation and Test Gates

- [x] Deterministic worker handoff tests pass.
- [x] Determinism remains stable for fixed startup camera/headless path.
- [x] Scratch reuse and growth metrics are emitted for tuning.
- [x] No mutable staging sharing occurs across concurrent worker jobs.

## Definition of Done

- [x] Worker/commit responsibilities are clean and enforceable.
- [x] Mode A is stable under the current headless validation path.
- [x] Threading metrics support tuning decisions.

## Deviations From Earlier Plan Text

- The earlier master-plan wording allowed a lock-free queue or a double-buffered command list. The shipped implementation uses a persistent mutex/condvar queue with deterministic sequence ordering because it is simpler to reason about and matches the current safety goals.
- The earlier master-plan wording described a more aggressive Mode B. The current phase text intentionally removes that path; shipping code is Mode A only.
- Scratch reuse currently targets worker-side generation buffers. Resident payload storage still owns its final mesh and packed buffers after handoff, so the current optimization pass focused on removing temporary worker allocations before attempting a larger ownership refactor.

## Test Record

- [x] Date: 2026-03-22
- [x] Result summary: `cargo test` passed `42/42`; `./scripts/build_rust.sh` built successfully; `./scripts/run_godot.sh --headless --quit-after 5` loaded the extension and logged `worker_threads=4`, `worker_jobs=5`, `worker_queue_peak=5`, `worker_waits=4`, `sample_scratch_reuse=1`, `mesh_scratch_reuse=1`, `pack_scratch_reuse=1`, `scratch_growth=40`, `render_cold_commits=5`, `physics_commits=0`, and `deferred_ops=0`.
- [x] Mode tested: Mode A only
- [x] Follow-up actions: if post-fix profiling shows worker generation dominant again after the visibility/physics reductions, reevaluate a larger ownership refactor or a GPU-assisted generation path from this cleaner baseline.

## References

- [Thread-safe APIs - Godot docs](https://docs.godotengine.org/en/latest/tutorials/performance/thread_safe_apis.html)
- [Optimization using Servers - Godot docs](https://docs.godotengine.org/en/stable/tutorials/performance/using_servers.html)
- [godot-rust built-in types (packed arrays)](https://godot-rust.github.io/book/godot-api/builtins.html)
