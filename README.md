# world2

Phase 10 precision/origin hardening for a Godot + Rust (godot-rust/gdext) planet runtime.

## What is set up

- Godot project config at `project.godot`.
- Runtime root scene at `scenes/main.tscn` with shell-only layout.
- GDExtension config at `world2.gdextension`.
- Rust extension crate at `rust/` using git dependency on `godot-rust/gdext`.
- Phase 02 runtime data model in `rust/src/runtime.rs`, including chunk identity, payload, RID state, pool compatibility, and bounded payload residency helpers.
- Phase 03 geometry helpers in `rust/src/geometry.rs`, including deterministic face bases, chunk-local sample mapping, default spherified cube projection, and a 3D planet-space displacement field with seam continuity tests.
- Phase 04 topology helpers in `rust/src/topology.rs`, including basis-derived cross-face edge transforms, same-LOD neighbor lookup, and runtime metadata neighbor normalization without manual face-edge tables.
- Phase 05 canonical mesh topology in `rust/src/mesh_topology.rs`, including locked `32/33/35` chunk constants, precomputed base plus 16 stitch index variants, and fine-to-coarse stitch-mask derivation.
- Phase 05 surface compatibility tightening in `rust/src/runtime.rs`, including topology/stitch/index/material/format class keys, stride-aware byte validation, and warm-path fallback routing when reuse is incompatible.
- Phase 06 visibility and LOD selection in `rust/src/runtime.rs`, including lazy chunk-metadata caching, horizon-first visibility traversal, projected-error split/merge hysteresis, neighbor LOD delta normalization, separate render/physics active sets, and budgeted commit/upload deferral metrics.
- Phase 06 runtime tick integration in `rust/src/lib.rs`, including active-camera frustum capture, per-frame selector execution, and headless-friendly debug counters/logging.
- Phase 07 configurable metadata prebuild and payload policy in `rust/src/runtime.rs`, including `metadata_precompute_max_lod`, `payload_precompute_max_lod`, and default startup metadata prebuild through LOD 5 with lazy metadata fallback above that window.
- Phase 07 scalar-field sampling and mesh derivation in `rust/src/runtime.rs`, including `35 x 35` border-ring sample grids, seam-safe cube-surface remapping across face edges, normals derived from sampled global field, tangents/UVs/colors, and stitch-mask-driven index selection.
- Phase 07 byte-region packing and logical warm-path preparation in `rust/src/runtime.rs`, including separated vertex/attribute/index region packing for the shipped `0x1B` surface format class, logical render lifecycle commands, physics-ready collider payloads, and reusable Godot-owned `PackedByteArray` staging on the live runtime path.
- Phase 08 server-side render commit path in `rust/src/runtime.rs`, including cold `RenderingServer` mesh/instance creation, warm vertex/attribute/index region updates, transform/scenario rebinding on pooled activation, strict surface-class compatibility checks, and per-class render pool watermarks.
- Phase 08 conservative collision commit path in `rust/src/runtime.rs`, including `PhysicsServer3D` static-body residency, concave shape refresh for near-camera chunks, bounded physics pooling, and explicit RID teardown on shutdown.
- Phase 08 runtime logging in `rust/src/lib.rs`, including per-frame cold/warm commit counts, fallback-reason counters, and render/physics pool occupancy for headless validation.
- Phase 09 threaded render payload generation in `rust/src/runtime.rs`, including persistent Rust worker threads, deterministic request/result ordering, single-lane commit ownership, reusable per-worker scratch buffers for sampling/mesh/packing work, and explicit queue/wait/allocation-style metrics.
- Phase 09 runtime logging in `rust/src/lib.rs`, including worker-thread counts, queued job peaks, worker wait counts, scratch reuse hits, and scratch growth events in the headless validation output.
- Phase 10 precision/origin policy in `rust/src/runtime.rs`, including `f64` chunk anchors as authority, chunk-local `f32` mesh/collider buffers, explicit render/physics transform conversion from a shared origin snapshot, thresholded camera-relative origin recentering, and active transform rebinds when the shared origin shifts.
- Phase 10 scene-root rebasing in `rust/src/lib.rs`, including camera-state recovery from render-relative coordinates, root-node origin shifts for child gameplay nodes, and headless logging for origin rebases and transform rebind counts.
- Headless debug scene tuning in `scenes/main.tscn`, placing the default camera outside the planet so horizon/frustum selection can be validated without editor interaction.
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
