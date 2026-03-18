#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DEFAULT_GODOT_BIN="$PROJECT_ROOT/../godot/bin/godot.macos.editor.arm64"
GODOT_BIN="${GODOT_BIN:-$DEFAULT_GODOT_BIN}"

if [[ ! -x "$GODOT_BIN" ]]; then
  echo "Godot binary not found or not executable: $GODOT_BIN" >&2
  exit 1
fi

exec "$GODOT_BIN" --path "$PROJECT_ROOT" "$@"
