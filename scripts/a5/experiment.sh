#!/usr/bin/env bash
# Linux-only, workspace-local; provisional selection, no simulator changes.
set -euo pipefail
ROOT=$(pwd)
MODE=${A5_SELECTION:-smoke}
[[ "$MODE" == smoke || "$MODE" == proposal ]] || exit 2
export PATH="$ROOT/.a5/tools/sail/bin:$ROOT/.a5/tools/gcc/bin:$PATH"
mkdir -p .a5/{tools,evidence,config}
python3 scripts/a5/test_runner.py
python3 scripts/a5/test_accounting.py
python3 scripts/a5/provision.py
python3 scripts/a5/inventory.py
if [[ "$MODE" == proposal ]]; then
  python3 scripts/a5/accounting.py prepare
fi
uv python install 3.12.12
cp scripts/a5/{test_config.yaml,ruscv-rv64i-smoke.yaml,rvmodel_macros.h,rvtest_config.h,link.ld} .a5/config/
# Derive a complete Sail configuration from the pinned upstream schema example.
# This describes the oracle, NOT extra DUT capabilities. Disable optional ISAs.
python3 scripts/a5/sail_config.py
{
  uname -a
  printf 'ImageOS=%s ImageVersion=%s\n' "${ImageOS:-}" "${ImageVersion:-}"
  ruby --version
  bundle --version
  uv --version
  sail_riscv_sim --version
  riscv64-unknown-elf-gcc --version
  riscv64-unknown-elf-objdump --version
  ldd .a5/tools/sail/bin/sail_riscv_sim
} > .a5/evidence/versions.txt
cd .a5/upstream
export BUNDLE_GEMFILE="$PWD/framework/src/act/data/Gemfile"
bundle install
python3 "$ROOT/scripts/a5/udb_overlay.py" "$(bundle show udb)" "$ROOT"
# The upstream calls `udb` directly, not `bundle exec udb`.
export PATH="$(ruby -e 'print Gem.bindir'):$PATH"
bundle exec uv run --frozen --python 3.12.12 --package act act --help > "$ROOT/.a5/evidence/act-help.txt"
# Use the checked-in upstream-generated source unmodified. The ACT4 pipeline
# compiles it, asks Sail for its signature, then links RVTEST_SELFCHECK.
if [[ "$MODE" == smoke ]]; then
  mkdir -p "$ROOT/.a5/selection/rv64i/I"
  cp tests/rv64i/I/I-add-00.S "$ROOT/.a5/selection/rv64i/I/"
  cp -R tests/env "$ROOT/.a5/selection/"
fi
generation_rc=0
bundle exec uv run --frozen --python 3.12.12 --package act act \
  "$ROOT/.a5/config/test_config.yaml" --test-dir "$ROOT/.a5/selection" \
  --workdir "$ROOT/.a5/work" --extensions all --jobs 1 --verbose || generation_rc=$?
cd "$ROOT"
rustup toolchain install 1.93.1 --profile minimal
cargo +1.93.1 build --locked --release
python3 scripts/a5/validate_controls.py "$ROOT"
if [[ "$MODE" == proposal ]]; then
  # Control failure and required-case failure both fail the overall run.
  controls_rc=0
  suite_rc=0
  python3 scripts/a5/suite_controls.py || controls_rc=$?
  python3 scripts/a5/accounting.py run --generation-returncode "$generation_rc" || suite_rc=$?
  [[ "$controls_rc" == 0 && "$suite_rc" == 0 ]]
else
  [[ "$generation_rc" == 0 ]]
  python3 scripts/a5/run.py
fi
