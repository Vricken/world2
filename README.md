# world2

Phase 15 strategy-layer refinement for a Godot + Rust (godot-rust/gdext) planet runtime.

## What is set up

- Godot project config at `project.godot`.
- Runtime root scene at `scenes/main.tscn` with shell-only layout.
- GDExtension config at `world2.gdextension`.
- Rust extension crate at `rust/` using git dependency on `godot-rust/gdext`.
- Phase 02 runtime data model in `rust/src/runtime/data.rs`, including chunk identity, payload, RID state, pool compatibility, and bounded payload residency helpers.
- Phase 03 geometry helpers in `rust/src/geometry.rs`, including deterministic face bases, chunk-local sample mapping, default spherified cube projection, and a 3D planet-space displacement field with seam continuity tests.
- Phase 04 topology helpers in `rust/src/topology.rs`, including basis-derived cross-face edge transforms, same-LOD neighbor lookup, and runtime metadata neighbor normalization without manual face-edge tables.
- Phase 05 canonical mesh topology in `rust/src/mesh_topology.rs`, including locked `32/33/35` chunk constants, precomputed base plus 16 stitch index variants, and fine-to-coarse stitch-mask derivation.
- Phase 05 surface compatibility tightening in `rust/src/runtime/data.rs`, including topology/stitch/index/material/format class keys, stride-aware byte validation, and warm-path fallback routing when reuse is incompatible.
- Phase 06 visibility and LOD selection in `rust/src/runtime/pipeline/selection.rs`, including sampled per-chunk min/max height metadata, full metadata availability through the runtime `max_lod`, horizon-first visibility traversal with no near-surface hard disable, projected-error split/merge hysteresis, cached split-ancestor bookkeeping, capped near-camera physics residency, coverage-safe render retirement, and budgeted commit/upload deferral metrics.
- Phase 06 runtime tick integration in `rust/src/lib.rs`, including active-camera frustum capture, per-frame selector execution, and headless-friendly debug counters/logging.
- Phase 07 configurable metadata prebuild and payload policy in `rust/src/runtime/core.rs`, including `metadata_precompute_max_lod`, `payload_precompute_max_lod`, and startup metadata prebuild through the effective runtime `max_lod`.
- Phase 07 scalar-field sampling and mesh derivation in `rust/src/runtime/pipeline/selection.rs` and `rust/src/runtime/workers/payloads.rs`, including `35 x 35` border-ring sample grids, seam-safe cube-surface remapping across face edges, normals derived from sampled global field, tangents/UVs/colors, and stitch-mask-driven index selection.
- Phase 07 byte-region packing and logical warm-path preparation in `rust/src/runtime/pipeline/selection.rs` and `rust/src/runtime/workers/payloads.rs`, including separated vertex/attribute/index region packing for the shipped `0x1B` surface format class, logical render lifecycle commands, physics-ready collider payloads, and reusable Godot-owned `PackedByteArray` staging on the live runtime path.
- Phase 08 server-side render commit path in `rust/src/runtime/pipeline/commit.rs`, including cold `RenderingServer` mesh/instance creation, warm mesh/instance RID reuse with full surface refresh on update, transform/scenario rebinding on pooled activation, strict surface-class compatibility checks, and per-class render pool watermarks.
- Phase 08 conservative collision commit path in `rust/src/runtime/pipeline/commit.rs`, including `PhysicsServer3D` static-body residency, worker-precomputed concave face payloads for a capped near-camera chunk set, reduced commit-lane collision preprocessing, bounded physics pooling, and explicit RID teardown on shutdown.
- Phase 08 runtime logging in `rust/src/lib.rs`, including per-frame cold/warm commit counts, fallback-reason counters, and render/physics pool occupancy for headless validation.
- Phase 09 threaded render payload generation in `rust/src/runtime/workers/payloads.rs`, including persistent Rust worker threads, async request submission, epoch-tagged stale-result rejection, queue-side supersession of older overlapping requests, off-thread collision face preprocessing, single-lane commit ownership, reusable per-worker scratch buffers for sampling/mesh/packing/slope work, and explicit queue/inflight/scratch metrics.
- Phase 09 threaded metadata and asset-group precompute in `rust/src/runtime/workers/metadata.rs`, `rust/src/runtime/workers/asset_groups.rs`, `rust/src/runtime/pipeline/selection.rs`, and `rust/src/runtime/pipeline/commit.rs`, including compact dense metadata slabs keyed by `(lod, face, x, y)` instead of a per-chunk hash map, async chunk-metadata generation only when the configured prebuild window is intentionally lower than runtime `max_lod`, parent retention while child metadata is still in flight, neighbor-normalization collapsing over-fine branches back to the nearest valid ancestor until required child metadata arrives instead of spinning or leaving invalid delta>1 seams behind, worker-built desired asset groups and local bounds, and single-lane `RenderingServer` multimesh commits once prepared results are ready.
- Phase 09 runtime logging in `rust/src/lib.rs`, including worker-thread counts, submitted/ready/stale/superseded/inflight job counters, queued job peaks, scratch reuse hits, and scratch growth events in the headless validation output.
- Phase 10 precision/origin policy across `rust/src/runtime/core.rs`, `rust/src/runtime/pipeline/commit.rs`, and `rust/src/runtime/math/utils.rs`, including `f64` chunk anchors as authority, chunk-local `f32` mesh/collider buffers, explicit render/physics transform conversion from a shared origin snapshot, thresholded camera-relative origin recentering, and active transform rebinds when the shared origin shifts.
- Phase 10 scene-root rebasing in `rust/src/lib.rs`, including per-frame selector execution for streaming throughput, physics-tick root-node origin shifts for child gameplay nodes, collision-contact rebase deferral for camera-owned `CharacterBody3D` controllers, immediate render/physics RID rebinds on actual rebases, interpolation reset on actual rebases, and headless logging for origin rebases and transform rebind counts.
- Phase 11 seam-validation coverage in `rust/src/runtime/tests.rs`, including rendered cross-face edge matching across all 24 directed face seams, delta-1 fine-to-coarse stitched-edge checks for all four edge directions, and deterministic seam-class warm-path rejection coverage.
- Phase 11 seam diagnostics in `rust/src/runtime/data.rs`, `rust/src/runtime/core.rs`, and `rust/src/lib.rs`, including active and pooled stitch-mask summaries, stitched-edge counters, pending seam-mismatch detection, and headless/loggable inspection hooks on `PlanetRoot`.
- Phase 12 deterministic asset placement in `rust/src/runtime/assets.rs`, `rust/src/runtime/pipeline/selection.rs`, and `rust/src/runtime/workers/payloads.rs`, including `(planet_seed, chunk_key, cell_id, family_id)` placement hashes, terrain-projected reject filters for biome/slope/curvature/altitude/procedural-mask/exclusion radius, and chunk-owned accepted transforms stored directly in `ChunkPayload.assets`.
- Phase 12 compact asset residency in `rust/src/runtime/pipeline/commit.rs`, `rust/src/runtime/workers/asset_groups.rs`, and `rust/src/lib.rs`, including async desired-group precompute for `RenderingServer` multimeshes grouped by `(face, lod, 2x2 chunk batch, asset family)`, per-group custom AABBs, shared family mesh RIDs, origin-shift rebind support, and headless counters for active asset groups/instances.
- Phase 13 runtime default controls in `rust/src/runtime.rs`, `rust/src/runtime/data.rs`, `rust/src/lib.rs`, `rust/src/topology.rs`, `rust/src/runtime/pipeline/commit.rs`, and `rust/src/runtime/tests.rs`, including radius-derived default `max_lod` from planet size with a `32 m` minimum average chunk surface span target, a Project Settings-backed `world2/runtime/max_lod_cap` ceiling defaulting to `10`, topology support through LOD `16`, clamped `payload_precompute_max_lod`, explicit commit/upload budgets, per-kind activation caps, a conservative `physics_pool_watermark = 4` below the render per-class watermark, and regression coverage for the documented starting values.
- PlanetRoot tool-time editor integration in `rust/src/lib.rs`, including an exported `planet_radius` inspector property, runtime reconstruction from editor-authored radius values, and a simple in-editor sphere preview that matches the configured planet radius.
- Phase 14 build-order continuity in `rust/src/runtime.rs`, `rust/src/runtime/tests.rs`, and `rust/src/lib.rs`, including the explicit 23-step implementation sequence, phase-to-step handoff metadata for phases 01-13, runtime-accessible build-order summaries, and regression coverage that prevents sequencing drift.
- Phase 15 strategy seams in `rust/src/runtime/strategy.rs`, `rust/src/runtime/data.rs`, `rust/src/runtime/pipeline/selection.rs`, `rust/src/runtime/pipeline/commit.rs`, `rust/src/runtime/workers/payloads.rs`, `rust/src/runtime/assets.rs`, `rust/src/runtime/tests.rs`, and `rust/src/lib.rs`, including config-backed projection/visibility/backend/staging policies, default strategy summaries in runtime logs, and regression coverage that keeps the shipped strategy stack behaviorally aligned with phases 01-14.
- Fly debug controller in `scripts/player/fly_controller.gd` and `scenes/main.tscn`, with WASD + Space/Shift flight, mouse look, and Up/Down speed scaling over the default spawn outside the planet.
- Launch and build scripts in `scripts/`.

## Prerequisites

- Rust toolchain installed.
- Godot binary available at `../godot/bin/godot.macos.editor.arm64` or set `GODOT_BIN`.

## Build

```bash
./scripts/build_rust.sh
```

Release build:

```bash
./scripts/build_rust.sh --release
```

## Run

```bash
./scripts/run_godot.sh
```

Use a custom binary:

```bash
GODOT_BIN=/absolute/path/to/godot ./scripts/run_godot.sh
```

## Controls

- `W`, `A`, `S`, `D`: fly
- `Space` / `Shift`: move up / down
- Mouse: look
- `Up`: double fly speed
- `Down`: halve fly speed
- `Esc`: release mouse capture
