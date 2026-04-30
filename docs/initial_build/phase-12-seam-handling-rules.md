# Phase 12 - Asset Placement

## Goal

Restore deterministic chunk-local asset placement rules and streaming ownership behavior with full narrative detail.

## Implementation Status

Implemented on 2026-03-22 in:

- `rust/src/runtime/assets.rs`
- `rust/src/runtime.rs`
- `rust/src/runtime/core.rs`
- `rust/src/runtime/data.rs`
- `rust/src/runtime/pipeline/selection.rs`
- `rust/src/runtime/pipeline/commit.rs`
- `rust/src/runtime/workers/payloads.rs`
- `rust/src/runtime/tests.rs`
- `rust/src/lib.rs`

What shipped:

- Deterministic chunk-local asset placement now runs during payload preparation, using the required hash inputs `(planet_seed, chunk_key, cell_id, family_id)` to derive one candidate per placement cell and family.
- Candidate projection samples the same documented cube-sphere terrain field used by chunk generation, then applies deterministic moisture, height, slope, curvature, altitude, procedural mask, below-sea, and exclusion-radius filters before storing accepted transforms in `ChunkPayload.assets`.
- Asset residency is now bound to the active render chunk set instead of scene-tree callbacks, and accepted instances remain chunk-owned even when grouped later for rendering.
- Repeated assets now render through compact `RenderingServer` multimeshes keyed by `(face, lod, chunk_group, asset_family)` with a fixed `2 x 2` chunk batch policy, custom group AABBs, and one shared low-poly family mesh RID per asset family.
- Origin-rebind and teardown paths now include asset multimesh instances, and `PlanetRoot` logs/exported counters expose active asset group and instance totals during headless validation.

## Documentation Checked Before Implementation

Checked on 2026-03-22:

- Godot stable `MultiMesh` docs for the one-object culling trade-off and custom AABB requirements.
- Godot stable `RenderingServer` docs for `multimesh_create()`, `multimesh_allocate_data()`, `multimesh_set_mesh()`, `multimesh_instance_set_transform()`, `multimesh_set_custom_aabb()`, `multimesh_set_visible_instances()`, and instance base/scenario/transform binding.
- godot-rust generated `RenderingServer` docs for the exact gdext enum and method bindings used by the multimesh commit path.

Constraints carried into code:

- Multimesh instances are not individually culled, so grouping must stay spatially compact; this implementation batches at most `2 x 2` chunks per face/LOD/family group.
- The runtime supplies a custom AABB for each multimesh group instead of relying on undocumented engine inference.
- Asset placement stays deterministic by keeping all accept/reject inputs pure functions of runtime config, chunk identity, and terrain sampling.
- Asset multimesh pooling remains deferred until after terrain RID pooling, matching the original phase priority.

## Continuity From Phases 01-11

This phase depends on:

- deterministic chunk keys and topology from Phases 03-05
- render/physics residency selection from Phase 06
- payload assembly and commit ownership from Phases 07-08
- seam-safe terrain interpretation from Phase 11

Phase 12 adds deterministic placement and residency policy on top of those established systems.

## Deterministic Placement Pipeline

After terrain sampling per chunk:

```text
1. divide chunk into placement cells
2. hash (planet_seed, chunk_key, cell_id, family_id)
3. generate candidate point(s)
4. project candidate to terrain
5. reject by:
   - biome
   - slope
   - curvature
   - altitude
   - mask textures
   - exclusion radius
6. store accepted transforms in chunk payload
```

Keep assets attached to owning chunk identity for deterministic streaming and straightforward invalidation.

## MultiMesh Guidance

Use `MultiMesh` for repeated assets, but honor its culling trade-off: instances inside one multimesh are not culled independently.

Recommendations:

- not one multimesh for the entire planet
- one multimesh per `(chunk_group, asset_family)` or similarly compact spatial grouping
- near: optional higher-quality handling for important/interactable assets
- mid: multimesh
- far: impostor, billboard, or none

Because chunk visibility is server-driven, asset residency should follow chunk lifecycle rather than scene-node visibility callbacks.

## Pooling Priority

Asset multimesh pooling/reuse can be added later, but only after terrain chunk pooling and staging reuse path is stable.

## Deviation Notes

- The original phase text called out `mask textures` as a reject input. The shipped runtime uses a deterministic procedural mask signal derived from the sampled terrain direction instead, because no texture-backed placement mask system exists in the current codebase yet. This keeps placement deterministic without inventing an undocumented texture pipeline.
- Asset families currently use built-in low-poly server meshes created at runtime instead of imported authored assets. That keeps the Phase 12 residency and grouping rules testable while leaving higher-fidelity authored assets for a later content pass.

## Checklist

- [x] Implement deterministic placement hash inputs exactly.
- [x] Keep all reject filters deterministic and documented.
- [x] Attach placement ownership to chunk lifecycle state.
- [x] Keep multimesh groups spatially compact.
- [x] Avoid global multimesh sets for planet-wide instances.
- [x] Add deterministic replay checks for same seed/path.

## Prerequisites

- [x] Phase 11 seam-handling rules completed.

## Ordered Build Steps

1. [x] Implement deterministic placement-cell hashing.
2. [x] Implement candidate projection onto terrain and deterministic reject filters.
3. [x] Store accepted transforms in chunk payload ownership.
4. [x] Add chunk-group and asset-family multimesh grouping policy.
5. [x] Bind asset residency to chunk active-set lifecycle.
6. [x] Add deterministic replay validation for fixed seed and camera path.

## Validation and Test Gates

- [x] Placement replay is identical for fixed seed/chunk/camera path.
- [x] Asset residency diffing follows chunk lifecycle deterministically.
- [x] Spatial grouping avoids one-global-multimesh anti-pattern.

## Definition of Done

- [x] Asset placement is deterministic and chunk-owned.
- [x] Grouping strategy is culling-aware and scalable.
- [x] Placement behavior is reproducible under test.

## Runtime Validation Hooks

Headless and live inspection can now use:

- `runtime_active_asset_group_count()`
- `runtime_active_asset_instance_count()`
- `runtime_active_stitch_mask_summary()`
- `runtime_pending_seam_mismatch_count()`

The periodic runtime log now also includes:

- asset payload chunk count
- asset candidate, rejected, and accepted totals
- active asset group and instance counts
- live asset-family mesh RID count

## Test Record (Fill In)

- [x] Date: 2026-03-22
- [x] Result summary: `cargo test` passed `48/48`; `./scripts/build_rust.sh` built successfully; `./scripts/run_godot.sh --headless --quit-after 5` loaded the extension, logged `Phase 12 runtime active`, and on the first runtime tick reported `asset_payload_chunks=5`, `asset_candidates=640`, `asset_rejected=614`, `asset_accepted=26`, `active_asset_groups=6`, `active_asset_instances=26`, and `asset_family_meshes=2` with no headless shutdown errors.
- [x] Replay scenarios tested: same-seed chunk placement replay; compact group-key bucketing for `2 x 2` chunk batches; active-render lifecycle diffing for grouped asset residency; repeated fixed-camera-path selection across two runtimes with matching asset group summaries.
- [x] Follow-up actions: integrate real authored asset meshes/materials and a texture-backed placement-mask path without breaking the deterministic chunk-owned residency contract.

## References

- [MultiMesh - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_multimesh.html)
- [RenderingServer - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_renderingserver.html)
- [RenderingServer in godot-rust API docs](https://godot-rust.github.io/docs/gdext/master/godot/classes/struct.RenderingServer.html)
