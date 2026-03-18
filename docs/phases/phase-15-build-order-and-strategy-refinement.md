# Phase 15 - One Important Refinement

## Goal

Restore full strategy-layer refinement detail, including projection/visibility/backend/staging abstractions and final architecture shape.

## Projection Strategy (Keep Swappable)

```rust
pub trait CubeProjection {
    fn project(&self, cube_point: DVec3) -> DVec3;
}

pub struct SpherifiedCubeProjection;

impl CubeProjection for SpherifiedCubeProjection {
    fn project(&self, cube_point: DVec3) -> DVec3 {
        let x = cube_point.x;
        let y = cube_point.y;
        let z = cube_point.z;

        let x2 = x * x;
        let y2 = y * y;
        let z2 = z * z;

        let sx = x * (1.0 - y2 * 0.5 - z2 * 0.5 + (y2 * z2) / 3.0).sqrt();
        let sy = y * (1.0 - z2 * 0.5 - x2 * 0.5 + (z2 * x2) / 3.0).sqrt();
        let sz = z * (1.0 - x2 * 0.5 - y2 * 0.5 + (x2 * y2) / 3.0).sqrt();

        DVec3::new(sx, sy, sz).normalize()
    }
}
```

## Visibility Strategy Layer

```rust
pub trait ChunkVisibilityStrategy {
    fn horizon_visible(&self, camera: &CameraState, meta: &ChunkMeta) -> bool;
    fn frustum_visible(&self, camera: &CameraState, meta: &ChunkMeta) -> bool;
    fn screen_error_px(&self, camera: &CameraState, meta: &ChunkMeta) -> f32;
}
```

## Backend Strategy Layer

```rust
pub trait ChunkRenderBackend {
    fn cold_create(&mut self, class: &SurfaceClassKey, payload: &ChunkPayload, xform: Transform3D);
    fn warm_reuse_update(&mut self, class: &SurfaceClassKey, payload: &ChunkPayload, xform: Transform3D);
    fn deactivate(&mut self, key: ChunkKey);
}
```

## Packed Staging Policy Layer

```rust
pub trait PackedStagingPolicy {
    fn acquire_staging(&mut self, class: &SurfaceClassKey) -> &mut GdPackedStaging;
    fn fill_staging_from_payload(&mut self, class: &SurfaceClassKey, payload: &ChunkPayload);
}
```

This gives similar flexibility on buffer architecture as projection strategy gives for geometry:

- start conservative with arrays-based cold creation
- optimize region-update packing and class ownership later
- keep runtime testable and data-oriented

## Final Architecture Shape

- quadsphere with modified/spherified cube projection default
- 6 face quadtrees
- fixed-grid chunks
- 3D noise in planet-space
- horizon before frustum and LOD
- edge stitching with neighbor delta limited to 1
- global-space normals
- Rust workers for generation
- server-managed render chunks via `RenderingServer`
- per-class chunk render pools
- cold creation + warm byte-region update paths
- reusable Godot-owned `PackedByteArray` staging buffers
- no runtime dependency on undocumented zero-copy Rust buffer adoption
- reusable Rust-side worker scratch buffers
- server-managed near-player physics chunks via `PhysicsServer3D`
- chunk-group multimesh assets where beneficial
- render-origin-relative transforms
- bounded RID churn, upload budgets, and pool watermarks

## Final Caveat

Server-driven chunks + pooling + tighter FFI boundary are the right architecture for large planet rendering, but visibility discipline remains the dominant performance lever. Horizon culling, active-set control, collision residency, bounded commit churn, and bounded upload churn still matter more than any single low-level API trick.

## Checklist

- [ ] Keep projection strategy swappable with spherified default.
- [ ] Keep visibility strategy explicit and testable.
- [ ] Keep backend cold/warm behavior explicit and measurable.
- [ ] Keep staging policy explicit and class-compatible.
- [ ] Keep no-undocumented-zero-copy rule explicit in architecture docs.
- [ ] Validate final architecture checklist against real implementation.

## References

- [RenderingServer - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_renderingserver.html)
- [Optimization using Servers - Godot docs](https://docs.godotengine.org/en/stable/tutorials/performance/using_servers.html)
- [godot-rust built-in types (packed arrays)](https://godot-rust.github.io/book/godot-api/builtins.html)
