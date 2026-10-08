#!/bin/bash
# P0 verification only. Not perf-test.sh, measurement phases or a stable P1 command.
set -euo pipefail
cd "$(dirname "$0")/../.."
ROOT="$(pwd -P)"
HEAD="$(git rev-parse HEAD)"
if [ -n "$(git status --porcelain)" ]; then
    echo 'UNAVAILABLE: P0 exact-HEAD verification requires a clean worktree' >&2
    exit 2
fi
IMAGE='ghcr.io/mimiqdev/ruscv-sim-dev@sha256:cc3cfea2499f69d2ee91fc711fb646807a08d8160148c00303d2fa92e3e9a65c'
docker image inspect "$IMAGE" --format '{{.Id}} {{.Os}}/{{.Architecture}} {{json .RepoDigests}}'
mkdir -p target
# No deletion or previous binary/ELF reuse; distinct host/container Cargo output.
OUT="$(mktemp -d "$ROOT/target/a10-p0-verify-XXXXXX")"
REL="${OUT#"$ROOT/"}"
{
    date -u '+%Y-%m-%dT%H:%M:%SZ'
    printf 'source_head=%s\nsource_tree=%s\n' "$HEAD" "$(git rev-parse HEAD^{tree})"
    uname -a
    git diff --check e73b12b8467fd635b398382a5cbc7ce75d842f68..HEAD
    # Production, historical guests, accepted contracts and historical evidence
    # must remain unchanged; the P0 scope is additive harness/config/docs only.
    if [ -n "$(git diff --name-only e73b12b8467fd635b398382a5cbc7ce75d842f68..HEAD -- src tests/bare-metal-riscv-test docs/archive Cargo.lock)" ]; then
        echo 'P0 scope audit failed' >&2; exit 1
    fi
    docker run --rm --init --volume "$ROOT:/workspace" --workdir /workspace \
      --env CARGO_BUILD_JOBS=2 --env "CARGO_TARGET_DIR=$REL/cargo" \
      --env "P0_EVIDENCE_DIR=$REL" --env "RISCV_SOURCE_HEAD=$HEAD" \
      --env RISCV_REQUIRE_RISCV_TOOLCHAIN=1 --env RISCV_REQUIRE_A10_PINNED_TOOLS=1 --env CARGO_TERM_COLOR=never \
      "$IMAGE" bash -c '
        set -euo pipefail
        date -u +%Y-%m-%dT%H:%M:%SZ
        printf "source_head=%s\n" "$RISCV_SOURCE_HEAD"
        rustc -Vv; cargo -V; rustfmt --version; cargo clippy --version; uname -a
        python3 tools/a10/build_fixtures.py --out "$P0_EVIDENCE_DIR/fixtures"
        python3 tools/a10/derive_oracles.py "$P0_EVIDENCE_DIR/fixtures" --check
        cargo fmt --all -- --check
        cargo check --locked --all-features
        cargo clippy --locked --all-features --all-targets -- -D warnings
        cargo test --locked --all-features
        cargo doc --locked --all-features --no-deps
        cargo test --locked --all-features --test a10_p0_oracles -- --nocapture
        cargo test --locked --all-features --test a9_hart_facts --test a9_machine_lifecycle --test a9_facade_composition --test a9_cli_reporting
        sha256sum tools/a10/public-v1.json tools/a10/{mod.rs,routes.rs,probe.rs,build_fixtures.py,derive_oracles.py} Cargo.lock "$CARGO_TARGET_DIR/debug/ruscv-sim" "$CARGO_TARGET_DIR/debug/a10-p0-probe"
        sha256sum "$P0_EVIDENCE_DIR/fixtures/"*.elf "$P0_EVIDENCE_DIR/fixtures/build.json"
        date -u +%Y-%m-%dT%H:%M:%SZ
      '
    test "$HEAD" = "$(git rev-parse HEAD)"
    test -z "$(git status --porcelain)"
    date -u '+%Y-%m-%dT%H:%M:%SZ'
    printf 'P0 correctness verification completed at %s (no timing/performance verdict)\n' "$HEAD"
} 2>&1 | tee "$OUT/verification.log"
printf 'P0 retained local artifacts: %s\n' "$REL"
