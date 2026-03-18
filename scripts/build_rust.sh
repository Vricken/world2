#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

BUILD_MODE="debug"
if [[ "${1:-}" == "--release" ]]; then
  BUILD_MODE="release"
fi

cd "$PROJECT_ROOT/rust"
if [[ "$BUILD_MODE" == "release" ]]; then
  cargo build --release
else
  cargo build
fi

echo "Built world2_runtime ($BUILD_MODE)."
