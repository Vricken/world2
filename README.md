# world2

Phase 05 foundations for a Godot + Rust (godot-rust/gdext) planet runtime.

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
