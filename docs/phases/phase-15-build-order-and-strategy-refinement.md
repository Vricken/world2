# Phase 15 - One Important Refinement

## Goal

Restore the missing strategy-layer detail without changing the ownership, pooling, visibility, or FFI contracts already locked in by phases 01-14.

## Implementation Status

Implemented on 2026-03-22 in:

- `rust/src/runtime/strategy.rs`
- `rust/src/runtime/data.rs`
- `rust/src/runtime.rs`
- `rust/src/runtime/pipeline/selection.rs`
- `rust/src/runtime/pipeline/commit.rs`
- `rust/src/runtime/workers/payloads.rs`
- `rust/src/runtime/assets.rs`
- `rust/src/runtime/tests.rs`
- `rust/src/lib.rs`
- `docs/phases/README.md`
- `docs/phases/phase-15-build-order-and-strategy-refinement.md`
- `README.md`

What shipped:

- A dedicated `runtime/strategy.rs` module now formalizes four explicit strategy seams: projection, visibility, render backend, and packed staging policy.
- `RuntimeConfig` now carries the default strategy stack directly via `cube_projection`, `visibility_strategy`, `render_backend`, and `staging_policy`.
- The shipped projection strategy is still spherified cube by default, but metadata sampling, threaded payload generation, and deterministic asset placement now all read that strategy from config instead of hardcoding it in each path.
- Visibility math remains unchanged behaviorally, but it is now routed through `VisibilityStrategyKind::HorizonFrustumLod`, making the horizon/frustum/screen-error policy explicit and independently testable.
- Render commit ownership remains server-driven and pool-based, but it is now surfaced as `RenderBackendKind::ServerPool` so cold/warm behavior is a named backend seam instead of only implicit runtime internals.
- Packed staging reuse remains Godot-owned and copy-possible, but it is now surfaced as `PackedStagingPolicyKind::GodotOwnedReuse` so staging acquisition and in-place fills are a documented policy layer.
- `PlanetRuntime::strategy_summary()` and `PlanetRoot` logs now report the active strategy stack at startup and during headless validation.
- Regression tests now cover the default strategy summary, projection swappability, visibility-wrapper equivalence, and direct backend exercise without changing the live runtime behavior already established by earlier phases.

## Documentation Checked Before Implementation

Checked on 2026-03-22:

- Godot stable `Optimization using Servers` docs for the low-level server ownership model and explicit RID-based resource management.
- Godot stable `RenderingServer` docs for instance creation, scenario/base binding, transform updates, and manual RID cleanup.
- Godot stable dedicated-server/headless docs for the `--headless` validation path used in this repository.
- godot-rust built-in types docs for packed-array copy-on-write semantics, `resize()`, and slice-based mutation used by the staging policy.

Constraints carried into code:

- Phase 15 must not replace the already-shipped runtime architecture. It should only make the strategy boundaries explicit.
- Projection, visibility, backend, and staging defaults must preserve the same shipped behavior as phases 01-14.
- Packed staging still assumes copy-possible transfer semantics and must not imply undocumented zero-copy adoption of arbitrary Rust allocations.
- Strategy metadata must stay lightweight enough for config cloning, worker payload requests, runtime summaries, and tests.

## Continuity From Phases 01-14

Phase 15 does not replace the systems built in phases 01-14. It formalizes strategy seams so those systems remain testable and swappable without rewriting runtime ownership contracts.

The resulting architecture still keeps:

- chunk identity and lifecycle in Rust data,
- render state in `RenderingServer` RIDs,
- collision state in `PhysicsServer3D` RIDs,
- horizon before frustum and LOD,
- warm-path reuse separate from cold creation,
- Godot-owned reusable staging buffers on the hot path,
- worker-local scratch reuse for generation and packing.

## Strategy Layers Shipped

### Projection Strategy

```rust
pub trait ProjectionStrategy {
    fn label(&self) -> &'static str;
    fn project(&self, cube_point: DVec3) -> DVec3;
}

impl ProjectionStrategy for CubeProjection {
    // `Normalized` and `Spherified` both remain available.
}
```

The default runtime config sets `cube_projection = CubeProjection::Spherified`.

### Visibility Strategy

```rust
pub trait ChunkVisibilityStrategy {
    fn label(&self) -> &'static str;
    fn horizon_visible(&self, config: &RuntimeConfig, camera: &CameraState, meta: &ChunkMeta) -> bool;
    fn frustum_visible(&self, camera: &CameraState, meta: &ChunkMeta) -> bool;
    fn screen_error_px(&self, camera: &CameraState, meta: &ChunkMeta) -> f32;
}

pub enum VisibilityStrategyKind {
    HorizonFrustumLod,
}
```

The shipped implementation preserves the existing horizon-first selector math and exposes it through a named policy.

### Backend Strategy

```rust
pub trait ChunkRenderBackend {
    fn label(&self) -> &'static str;
    fn commit_render_payload(
        &self,
        runtime: &mut PlanetRuntime,
        key: ChunkKey,
        frame_state: &mut SelectionFrameState,
    ) -> bool;
    fn deactivate_render(&self, runtime: &mut PlanetRuntime, key: ChunkKey);
}

pub enum RenderBackendKind {
    ServerPool,
}
```

The shipped backend still performs the same cold-create and warm-pool behavior from Phase 08, but the policy is now explicit instead of implicit.

### Packed Staging Policy

```rust
pub trait PackedStagingPolicy {
    fn label(&self) -> &'static str;
    fn acquire_staging(
        &self,
        runtime: &mut PlanetRuntime,
        key: ChunkKey,
        surface_class: &SurfaceClassKey,
        warm_path: &RenderWarmPath,
    ) -> GdPackedStaging;
    fn fill_staging_from_payload(
        &self,
        staging: Option<GdPackedStaging>,
        packed_regions: Option<&PackedMeshRegions>,
        surface_class: &SurfaceClassKey,
    ) -> Option<GdPackedStaging>;
}

pub enum PackedStagingPolicyKind {
    GodotOwnedReuse,
}
```

The shipped staging policy preserves the conservative FFI boundary: reuse Godot-owned packed arrays, fill them through documented slice mutation, and treat Rust-to-Godot transfer as copy-possible.

## Runtime-Visible Strategy Summary

`PlanetRuntime::strategy_summary()` now reports the active stack as:

```text
projection=spherified_cube
visibility=horizon_frustum_lod
render_backend=server_pool_render_backend
staging=godot_owned_packed_byte_array
```

`PlanetRoot` includes that summary in both startup and periodic headless tick logs.

## Final Architecture Shape

- quadsphere with spherified cube projection default
- 6 face quadtrees
- fixed-grid chunks
- 3D noise in planet-space
- horizon before frustum and LOD
- edge stitching with neighbor delta limited to 1
- global-space normals
- Rust workers for generation
- server-managed render chunks via `RenderingServer`
- per-class chunk render pools
- cold creation plus warm pooled refresh paths
- reusable Godot-owned `PackedByteArray` staging buffers
- no runtime dependency on undocumented zero-copy Rust buffer adoption
- reusable Rust-side worker scratch buffers
- server-managed near-player physics chunks via `PhysicsServer3D`
- chunk-group multimesh assets where beneficial
- render-origin-relative transforms
- bounded RID churn, upload budgets, and pool watermarks

## Deviation Notes

- The original phase brief described the strategy layer as free-standing trait-object abstractions. The shipped implementation uses config-backed enums plus traits instead. This keeps `RuntimeConfig` clone/debug/test friendly, keeps worker requests serializable as plain data, and still preserves explicit swappable seams.
- The render backend seam is currently focused on render-chunk commit/deactivate behavior. Physics ownership remains explicit runtime logic because Phase 15 only required the missing render/backend refinement, not a full server abstraction rewrite.
- No new low-level optimization trick was introduced. The goal was explicit architecture seams with zero behavior drift, not a backend redesign.

## Final Caveat

Server-driven chunks plus pooling plus tighter FFI boundary are still the right architecture for large planet rendering, but visibility discipline remains the dominant performance lever. Horizon culling, active-set control, collision residency, bounded commit churn, and bounded upload churn still matter more than any single low-level API trick.

## Checklist

- [x] Keep projection strategy swappable with spherified default.
- [x] Keep visibility strategy explicit and testable.
- [x] Keep backend cold/warm behavior explicit and measurable.
- [x] Keep staging policy explicit and class-compatible.
- [x] Keep no-undocumented-zero-copy rule explicit in architecture docs.
- [x] Validate final architecture checklist against real implementation.

## Prerequisites

- [x] Phase 14 build-order continuity completed.

## Validation and Test Gates

- [x] Strategy seams can be exercised independently in tests.
- [x] Default strategy stack preserves current runtime behavior.
- [x] Architecture checklist matches implemented ownership/FFI rules.

## Definition of Done

- [x] Strategy-layer boundaries are documented and enforceable.
- [x] Final architecture shape is fully represented in local docs.

## Test Record

- [x] Date: 2026-03-22
- [x] Result summary: `cargo test` passed `55/55`, `./scripts/build_rust.sh` built successfully, and `./scripts/run_godot.sh --headless --quit-after 5` loaded cleanly. The startup log reported `strategy_summary=projection=spherified_cube visibility=horizon_frustum_lod render_backend=server_pool_render_backend staging=godot_owned_packed_byte_array`, `build_order_summary=phase=15 steps=1-23 handoff=phases01-10=1-20,phase11=doc+5/8/19,phase12=21,phase09=22,phase13=23 next=none`, and `next_phase=none`.
- [x] Strategies validated: unit tests exercised projection swapping, visibility policy parity with runtime wrappers, and direct backend invocation; the headless Godot pass exercised the live packed-staging path and reported `staged=5`, `render_cold_commits=5`, `active_render=5`, `active_asset_groups=6`, and `deferred_ops=0`.
- [x] Follow-up actions: if a future phase introduces alternate visibility or backend policies, extend the existing config-backed strategy enums first so the runtime summaries and regression tests continue to describe reality.

## References

- [Optimization using Servers - Godot docs (stable)](https://docs.godotengine.org/en/stable/tutorials/performance/using_servers.html)
- [RenderingServer - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_renderingserver.html)
- [Exporting for dedicated servers - Godot docs (stable)](https://docs.godotengine.org/en/stable/tutorials/export/exporting_for_dedicated_servers.html)
- [godot-rust built-in types (packed arrays)](https://godot-rust.github.io/book/godot-api/builtins.html)
- [All prior phase docs](./README.md)
