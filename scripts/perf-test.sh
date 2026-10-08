#!/bin/bash
# Public P1 smoke entry point; calibrated/compare are explicitly unavailable.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd -P)"
cd "$ROOT"
export PYTHONDONTWRITEBYTECODE=1
exec python3 tools/a10/command.py "$@"
