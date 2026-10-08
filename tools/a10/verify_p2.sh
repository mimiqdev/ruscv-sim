#!/bin/bash
# Exact committed P2 evidence/schema/own-oracle verification, not calibration.
set -euo pipefail
cd "$(dirname "$0")/../.."
ROOT="$(pwd -P)"
HEAD="$(git rev-parse HEAD)"
COMMON="$(git rev-parse --path-format=absolute --git-common-dir)"
IMAGE='ghcr.io/mimiqdev/ruscv-sim-dev@sha256:cc3cfea2499f69d2ee91fc711fb646807a08d8160148c00303d2fa92e3e9a65c'
test -z "$(git status --porcelain)" || { echo 'INCONCLUSIVE: clean committed source required' >&2; exit 2; }
docker image inspect "$IMAGE" --format '{{.Id}} {{.Os}}/{{.Architecture}} {{json .RepoDigests}}'
mkdir -p target
OUT="$(mktemp -d "$ROOT/target/a10-p2-verify-XXXXXX")"
PYTHONDONTWRITEBYTECODE=1 python3 tools/a10/collect.py "$OUT/physical-host.json"
CID="$(docker create --rm --init --volume "$ROOT:$ROOT" --volume "$COMMON:$COMMON:ro" --workdir "$ROOT" \
  --env GIT_OPTIONAL_LOCKS=0 --env GIT_CONFIG_COUNT=1 --env GIT_CONFIG_KEY_0=safe.directory --env "GIT_CONFIG_VALUE_0=$ROOT" \
  --env CARGO_BUILD_JOBS=2 --env "CARGO_TARGET_DIR=$OUT/cargo" --env "P2_OUT=$OUT" \
  --env RISCV_REQUIRE_RISCV_TOOLCHAIN=1 --env RISCV_REQUIRE_A10_PINNED_TOOLS=1 \
  --env "RISCV_PERF_PHYSICAL_HOST_RECORD=$OUT/physical-host.json" --env "RISCV_PERF_CONTAINER_RECORD=$OUT/container.json" \
  --env CARGO_TERM_COLOR=never --env PYTHONDONTWRITEBYTECODE=1 \
  "$IMAGE" bash -c '
    set -euo pipefail
    rustc -Vv; cargo -V; cargo clippy --version; uname -a
    cargo fmt --all -- --check
    cargo check --locked --all-features
    cargo clippy --locked --all-features --all-targets -- -D warnings
    cargo test --locked --all-features
    cargo doc --locked --all-features --no-deps
    cargo test --locked --all-features --test a10_p2_schema --test a10_p1_phases --test a10_p0_oracles -- --nocapture
    cargo test --locked --all-features --test a9_hart_facts --test a9_machine_lifecycle --test a9_facade_composition --test a9_cli_reporting
    set +e
    ./scripts/perf-test.sh run --suite public-v1 --profile smoke --out "$P2_OUT/smoke"
    STATUS=$?
    set -e
    test "$STATUS" = 2 || { echo "P2 smoke unexpected status $STATUS" >&2; exit 1; }
    python3 tools/a10/check_smoke.py "$P2_OUT/smoke/raw-report.json"
    DRIVER="$(python3 -c '\''import json,os; print(json.load(open(os.environ["P2_OUT"]+"/smoke/setup.json"))["binaries"]["driver"]["path"])'\'')"
    python3 tools/a10/test_bundle.py "$P2_OUT/smoke/report.json" "$DRIVER"
    ./scripts/perf-test.sh validate "$P2_OUT/smoke/report.json" --bundle-sha256 "$(cat "$P2_OUT/smoke/bundle.sha256")"
    sha256sum "$P2_OUT/smoke/"{report.json,raw-report.json,setup.json,bundle.json,bundle.sha256} "$P2_OUT/smoke/evidence/bin/"{ruscv-sim,a10-perf-driver,a10-p0-probe}
    sha256sum "$P2_OUT/smoke/fixtures/"*.elf tools/a10/public-v1.json Cargo.lock
  ')"
docker inspect "$CID" > "$OUT/container-inspect.json"
docker image inspect "$IMAGE" > "$OUT/image-inspect.json"
PYTHONDONTWRITEBYTECODE=1 python3 - "$OUT" <<'PY'
import json
from pathlib import Path
import sys
out=Path(sys.argv[1])
with (out/'container.json').open('x') as file:
    json.dump({'container':json.loads((out/'container-inspect.json').read_text())[0], 'image':json.loads((out/'image-inspect.json').read_text())[0]},file)
PY
{
    date -u '+%Y-%m-%dT%H:%M:%SZ'
    printf 'source_head=%s\nsource_tree=%s\ncontainer=%s\n' "$HEAD" "$(git rev-parse HEAD^{tree})" "$CID"
    uname -a
    git diff --check e73b12b8467fd635b398382a5cbc7ce75d842f68..HEAD
    if [ -n "$(git diff --name-only 766c223be36520a4555b6e98e66a95e4d797d22f..HEAD -- src Cargo.toml Cargo.lock docs/dev-plan.md docs/architecture/decisions docs/archive tests/bare-metal-riscv-test tools/a10/public-v1.json tools/a10/fixtures .github/workflows benches)" ]; then
        echo 'P2 preserved-source scope audit failed' >&2; exit 1
    fi
    # Actual inspected container ID, original absolute worktree paths and
    # read-only common Git metadata; no wildcard/global safe.directory changes.
    docker start --attach "$CID"
    test "$HEAD" = "$(git rev-parse HEAD)"
    test -z "$(git status --porcelain)"
    date -u '+%Y-%m-%dT%H:%M:%SZ'
    printf 'P2 exact-HEAD schema/oracle/bundle verification completed: %s; measurement INCONCLUSIVE, no performance PASS\n' "$HEAD"
} 2>&1 | tee "$OUT/verification.log"
printf 'P2 retained local artifacts: %s\n' "$OUT"
