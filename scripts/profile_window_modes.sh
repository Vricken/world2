#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SCENE="res://scenes/profiling/perf_probe.tscn"
SCENARIOS=(
  small_window
  fullscreen_native
  fullscreen_native_no_atmosphere
)

for scenario in "${SCENARIOS[@]}"; do
  echo "== $scenario =="
  "$SCRIPT_DIR/run_godot.sh" "$SCENE" -- --scenario="$scenario" 2>&1 | tee "/tmp/world2_${scenario}.log"
  rg "PERF_RESULT" "/tmp/world2_${scenario}.log"
done
