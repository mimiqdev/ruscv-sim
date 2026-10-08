#!/bin/bash
# Exact committed P1 correctness/scope verification. No calibrated speed verdict.
set -euo pipefail
cd "$(dirname "$0")/../.."
ROOT="$(pwd -P)"
HEAD="$(git rev-parse HEAD)"
COMMON="$(git rev-parse --path-format=absolute --git-common-dir)"
IMAGE='ghcr.io/mimiqdev/ruscv-sim-dev@sha256:cc3cfea2499f69d2ee91fc711fb646807a08d8160148c00303d2fa92e3e9a65c'
test -z "$(git status --porcelain)" || { echo 'INCONCLUSIVE: clean committed source required' >&2; exit 2; }
docker image inspect "$IMAGE" --format '{{.Id}} {{.Os}}/{{.Architecture}} {{json .RepoDigests}}'
mkdir -p target
OUT="$(mktemp -d "$ROOT/target/a10-p1-verify-XXXXXX")"
{
    date -u '+%Y-%m-%dT%H:%M:%SZ'
    printf 'source_head=%s\nsource_tree=%s\n' "$HEAD" "$(git rev-parse HEAD^{tree})"
    uname -a
    git diff --check e73b12b8467fd635b398382a5cbc7ce75d842f68..HEAD
    if [ -n "$(git diff --name-only 29bc58729ade28d611d17c524a26bd3369a64140..HEAD -- src Cargo.lock docs/dev-plan.md docs/architecture/decisions docs/archive tests/bare-metal-riscv-test tools/a10/public-v1.json tools/a10/fixtures .github/workflows)" ]; then
        echo 'P1 preserved-source scope audit failed' >&2; exit 1
    fi
    # Linked worktree .git files contain absolute host paths. Preserve those
    # paths; common Git metadata is read-only, optional index writes disabled.
    # safe.directory applies only to this process/one explicit checkout, never
    # global config or a wildcard trust exception.
    docker run --rm --init --volume "$ROOT:$ROOT" --volume "$COMMON:$COMMON:ro" --workdir "$ROOT" \
      --env GIT_OPTIONAL_LOCKS=0 --env GIT_CONFIG_COUNT=1 --env GIT_CONFIG_KEY_0=safe.directory --env "GIT_CONFIG_VALUE_0=$ROOT" \
      --env CARGO_BUILD_JOBS=2 --env "CARGO_TARGET_DIR=$OUT/cargo" --env "P1_OUT=$OUT" \
      --env RISCV_REQUIRE_RISCV_TOOLCHAIN=1 --env RISCV_REQUIRE_A10_PINNED_TOOLS=1 \
      --env "RISCV_PERF_CONTAINER=$IMAGE" --env CARGO_TERM_COLOR=never --env PYTHONDONTWRITEBYTECODE=1 \
      "$IMAGE" bash -c '
        set -euo pipefail
        rustc -Vv; cargo -V; cargo clippy --version; uname -a
        cargo fmt --all -- --check
        cargo check --locked --all-features
        cargo clippy --locked --all-features --all-targets -- -D warnings
        cargo test --locked --all-features
        cargo doc --locked --all-features --no-deps
        cargo test --locked --all-features --test a10_p1_phases --test a10_p0_oracles -- --nocapture
        cargo test --locked --all-features --test a9_hart_facts --test a9_machine_lifecycle --test a9_facade_composition --test a9_cli_reporting
        ./scripts/perf-test.sh run --suite public-v1 --profile smoke --out "$P1_OUT/smoke"
        python3 tools/a10/check_smoke.py "$P1_OUT/smoke/report.json"
        sha256sum "$P1_OUT/smoke/report.json" "$P1_OUT/smoke/setup.json" "$P1_OUT/smoke/cargo/release/"{ruscv-sim,a10-perf-driver,a10-p0-probe}
        sha256sum "$P1_OUT/smoke/fixtures/"*.elf tools/a10/public-v1.json Cargo.lock
      '
    test "$HEAD" = "$(git rev-parse HEAD)"
    test -z "$(git status --porcelain)"
    date -u '+%Y-%m-%dT%H:%M:%SZ'
    printf 'P1 exact-HEAD scope/correctness verification completed: %s; smoke remains measurement INCONCLUSIVE\n' "$HEAD"
} 2>&1 | tee "$OUT/verification.log"
printf 'P1 retained local artifacts: %s\n' "$OUT"
