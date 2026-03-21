# Phase 07 - Mesh Generation Pipeline

## Goal

Ship the stage-by-stage mesh generation pipeline that sits between Phase 06 selection and Phase 08 server-side render/physics commits.

## Implementation Status

Implemented on 2026-03-21 in:

- `rust/src/runtime.rs`
- `rust/src/lib.rs`

What shipped:

- Explicit Phase 07 runtime policy in `RuntimeConfig`, including `metadata_precompute_max_lod`, `payload_precompute_max_lod`, and a live-runtime `enable_godot_staging` switch used to keep plain Rust unit tests independent from Godot engine FFI.
- Startup metadata prebuild through a configurable window (`metadata_precompute_max_lod = 5` by default), with lazy metadata fallback above that window.
- Border-ring scalar sampling over the canonical `35 x 35` sample grid, including cube-surface remapping for samples that cross face edges or corners so normals remain seam-consistent.
- CPU mesh derivation for visible vertices, including positions, sampled-field normals, tangents, UVs, colors/masks, and stitch-mask-driven canonical index selection.
- Separate vertex, attribute, and index byte-region packing for the shipped surface class contract: `format_mask = 0x1B`, `vertex_stride = 12`, `attribute_stride = 24`, `index_stride = 4`.
- Reusable Godot-owned `PackedByteArray` staging on the live runtime path, filled in place with `as_mut_slice()` data copies from the packed Rust regions.
- Logical render lifecycle command assembly (`warm current`, `warm pooled`, `cold create`) and physics-ready collider payload attachment for physics-eligible chunks.
- Phase 07 counters exposed through the runtime tick logs, including sampled/meshed/packed/staged chunk counts and warm-vs-cold routing metrics.

## Documentation Checked Before Implementation

Checked on 2026-03-21:

- Godot stable `RenderingServer` docs for `mesh_surface_update_vertex_region()`, `mesh_surface_update_attribute_region()`, `mesh_surface_update_index_region()`, and the `mesh_surface_get_format_*_stride()` helpers.
- Godot stable procedural mesh docs for `ArrayMesh.add_surface_from_arrays()` array contracts and array-slot expectations.
- godot-rust docs for `RenderingServer` bindings and `PackedByteArray` slice access used by the staging path.

Constraints carried into code:

- Rust to Godot packed-array transfer is still treated as copy-possible, not zero-copy.
- The shipped Phase 07 byte packing targets the explicit `0x1B` surface class contract so the `12 / 24 / 4` region layout is internally consistent.
- The real `RenderingServer` RID mutation path is still Phase 08 work. Phase 07 now emits logical lifecycle commands and keeps warm-path state/staging ready for that commit layer.

## Stage Summary

### Stage A - Metadata Tree Build

- `PlanetRuntime::new()` prebuilds metadata through `metadata_precompute_max_lod`.
- `PlanetRuntime::build_metadata_tree_through_lod()` exists as the explicit Stage A builder.
- `PlanetRuntime::ensure_chunk_meta()` still lazily fills metadata above the configured prebuild window.

### Stage B - Sample Scalar Fields

- `PlanetRuntime::sample_chunk_scalar_field()` builds the border-ring sample grid.
- Samples store unit direction, height, two biome channels, and a derived slope hint.
- Out-of-face samples are remapped onto the cube surface before spherified projection so neighboring chunk borders remain consistent.

### Stage C - Derive Mesh Buffers

- `PlanetRuntime::derive_cpu_mesh_buffers()` turns the sample grid into `CpuMeshBuffers`.
- Normals are derived from the sampled global field via central differences over the border ring.
- Stitch indices come from `mesh_topology::canonical_chunk_topology()` using the selection-driven stitch mask.

### Stage D - Pack Rust Byte Regions

- `PlanetRuntime::pack_mesh_regions()` packs positions into the vertex region and normals/UVs/colors into the attribute region.
- The packer validates counts and stride expectations before writing byte data.

### Stage E - Fill Reusable Godot Staging Buffers

- `PlanetRuntime::stage_payload_bytes()` chooses reusable staging based on the warm-path decision.
- On the live runtime path, `GdPackedStaging::copy_from_regions()` mutates Godot-owned `PackedByteArray` buffers in place via `as_mut_slice()`.

### Stage F - Build Commit Payloads

- `PlanetRuntime::ensure_render_payload_for_selection()` assembles the final chunk payload, including surface class, stitch mask, packed regions, logical lifecycle command, transform, and optional collision mesh copies.

### Stage G - Prepare Commit Transitions

- Phase 07 now chooses and records the logical lifecycle command for each render payload (`WarmReuseCurrent`, `WarmReusePooled`, or `ColdCreate`).
- Actual mesh/resource/instance RID creation and region-update calls remain Phase 08, but the runtime now carries the exact data and warm-path state that Phase 08 will consume.

## Deviation Notes

- The original phase text implied metadata for every LOD at load. The shipped implementation makes that window explicit and bounded by config (`metadata_precompute_max_lod = 5` by default) to avoid allocating metadata for millions of chunks up front.
- Payload precompute policy is now explicit in config, but payloads are still generated on demand for selected chunks rather than by a background worker.
- Unit tests disable live Godot staging (`enable_godot_staging = false`) because `PackedByteArray` allocation requires the Godot engine runtime. The headless Godot validation path exercises the real staging behavior.
- Phase 07 stops at payload generation and logical lifecycle preparation. The actual server-side RID commit path is still tracked in Phase 08.

## Checklist

- [x] Implement all seven stages with explicit boundaries.
- [x] Keep packing aligned to the shipped class format/stride rules.
- [x] Use in-place staging fills for the live warm path.
- [x] Keep workers/node ownership out of the payload generation path.
- [x] Route incompatibilities to fallback with metrics.
- [x] Verify stage counts and byte-size integrity in tests.
- [x] Keep metadata and payload window policy explicit in runtime config.

## Prerequisites

- [x] Phase 06 active-set selection and metadata prerequisites completed.

## Ordered Build Steps

1. [x] Implement Stage A metadata-tree build with an explicit prebuild window and lazy fallback.
2. [x] Implement Stage B scalar-field sampling with a border ring.
3. [x] Implement Stage C CPU mesh derivation.
4. [x] Implement Stage D byte-region packing.
5. [x] Implement Stage E reusable Godot staging fill path for the live runtime.
6. [x] Implement Stage F commit-payload assembly.
7. [x] Implement Stage G logical lifecycle transition preparation for Phase 08.

## Validation and Test Gates

- [x] Stage-by-stage counters increment for newly prepared payloads and are visible in runtime logs.
- [x] Packed byte lengths match class stride expectations in unit tests.
- [x] Warm-path compatibility routing is exercised in unit tests.
- [x] Headless Godot validation exercises the real `PackedByteArray` staging path.

## Definition of Done

- [x] The A-G mesh-generation pipeline is deterministic for a fixed selected set.
- [x] The live runtime path uses reusable Godot-owned staging instead of transient `Vec<u8>` conversion objects.
- [x] Worker output remains data and lifecycle-command based, ready for Phase 08 commits.

## Test Record

- [x] Date: 2026-03-21
- [x] Result summary: `cargo test` passed with `28/28` tests, `./scripts/build_rust.sh` built successfully, and `./scripts/run_godot.sh --headless --quit-after 5` loaded the extension and reported `sampled=5`, `meshed=5`, `packed=5`, `staged=5`, `commit_payloads=5`, `cold=5`, `desired_render=5`, and `active_render=5` from the default debug camera.
- [x] Staging reuse notes: the first headless activation exercised the real in-place Godot staging fill path; warm-path compatibility routing is unit-tested, while repeated live warm-update stress remains a Phase 08 follow-up once server-side region updates are active.
- [x] Follow-up actions: connect `RenderLifecycleCommand` payloads to real `RenderingServer` mesh/instance updates in Phase 08 and add a moving-camera validation pass that observes live warm reuse after initial cold activation.

## References

- [ArrayMesh - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_arraymesh.html)
- [Using the ArrayMesh - Godot docs (stable)](https://docs.godotengine.org/en/stable/tutorials/3d/procedural_geometry/arraymesh.html)
- [RenderingServer - Godot docs (stable)](https://docs.godotengine.org/en/stable/classes/class_renderingserver.html)
- [RenderingServer in godot-rust API docs](https://godot-rust.github.io/docs/gdext/master/godot/classes/struct.RenderingServer.html)
- [godot-rust built-in types (packed arrays)](https://godot-rust.github.io/book/godot-api/builtins.html)
