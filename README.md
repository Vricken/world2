# world2

Phase 02 foundations for a Godot + Rust (godot-rust/gdext) planet runtime.

## What is set up

- Godot project config at `project.godot`.
- Runtime root scene at `scenes/main.tscn` with shell-only layout.
- GDExtension config at `world2.gdextension`.
- Rust extension crate at `rust/` using git dependency on `godot-rust/gdext`.
- Phase 02 runtime data model in `rust/src/runtime.rs`, including chunk identity, payload, RID state, pool compatibility, and bounded payload residency helpers.
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
