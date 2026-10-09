#!/bin/bash
# Public entry point: smoke/calibrated run, strict validate, cohort baseline
# binding and informational compare (P4 CI/retention deferred).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd -P)"
cd "$ROOT"
export PYTHONDONTWRITEBYTECODE=1
exec python3 tools/a10/command.py "$@"
