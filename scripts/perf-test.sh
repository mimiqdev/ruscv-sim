#!/bin/bash
# Public entry point: smoke/baseline run and strict validate. The removed
# subcommands were deleted 2026-10-10; P4 CI/retention remains deferred.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd -P)"
cd "$ROOT"
export PYTHONDONTWRITEBYTECODE=1
exec python3 tools/a10/command.py "$@"
