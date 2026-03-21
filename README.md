# world2

Phase 07 mesh-generation pipeline foundations for a Godot + Rust (godot-rust/gdext) planet runtime.

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
- Phase 07 runtime logging in `rust/src/lib.rs`, including per-frame sample/mesh/pack/staging/cold-vs-warm counters for headless validation.
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
