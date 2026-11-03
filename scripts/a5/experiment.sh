#!/usr/bin/env bash
# Linux-only, workspace-local; frozen selection runs, no simulator changes.
#
# A5_SELECTION chooses the frozen selection:
#   smoke     provisional single-source RV64I smoke (original feasibility path)
#   proposal  the frozen 51-case RV64I selection through the full accounting
#   zca       the frozen 32-source rv64i/Zca selection under the Zca-only
#             UDB/Sail configuration (contract D9)
#   fd        the frozen 82-source rv64i/F plus 114-source rv64i/D selection
#             under the F/D UDB/Sail configuration (contract D11)
#   zcd       the frozen 4-source rv64i/Zcd selection under the Zcd
#             UDB/Sail configuration (contract D11)
set -euo pipefail
ROOT=$(pwd)
MODE=${A5_SELECTION:-smoke}

# Everything a mode selects, in one place. The RV64I modes use the accounting
# defaults, so their SELECTED_* values stay empty and their command lines are
# unchanged.
SELECTED_CONFIG=""
SELECTED_MANIFEST=""
SELECTED_PLAN=""
SELECTED_ISA=""
ACCOUNTING=no
case "$MODE" in
  smoke)
    ACT_EXTENSIONS=all
    SAIL_SCRIPT=none
    RVTEST_CONFIG=rvtest_config.h
    ;;
  proposal)
    ACT_EXTENSIONS=all
    SAIL_SCRIPT=none
    RVTEST_CONFIG=rvtest_config.h
    ACCOUNTING=rv64i
    ;;
  zca)
    SELECTED_CONFIG=ruscv-rv64imac-zca
    SELECTED_MANIFEST=docs/verification/a5-zca-source-inventory.json
    SELECTED_PLAN=.a5/evidence/zca-plan.json
    SELECTED_ISA=rv64imac
    ACT_EXTENSIONS=Zca
    SAIL_SCRIPT=sail_config_zca.py
    RVTEST_CONFIG=rvtest_config.h
    ACCOUNTING=selected
    ;;
  fd)
    SELECTED_CONFIG=ruscv-rv64imafd
    SELECTED_MANIFEST=docs/verification/a5-fd-source-inventory.json
    SELECTED_PLAN=.a5/evidence/fd-plan.json
    SELECTED_ISA=rv64imafd
    ACT_EXTENSIONS=F,D
    SAIL_SCRIPT=sail_config_fd.py
    RVTEST_CONFIG=rvtest_config_fd.h
    ACCOUNTING=selected
    ;;
  zcd)
    SELECTED_CONFIG=ruscv-rv64imafdc-zcd
    SELECTED_MANIFEST=docs/verification/a5-zcd-source-inventory.json
    SELECTED_PLAN=.a5/evidence/zcd-plan.json
    SELECTED_ISA=rv64imafdc
    ACT_EXTENSIONS=Zcd
    SAIL_SCRIPT=sail_config_zcd.py
    RVTEST_CONFIG=rvtest_config_fd.h
    ACCOUNTING=selected
    ;;
  *)
    exit 2
    ;;
esac

# The accounting environment for one selected profile.
selected_accounting() {
  A5_CONFIG="$SELECTED_CONFIG" A5_MANIFEST="$SELECTED_MANIFEST" \
    A5_PLAN="$SELECTED_PLAN" A5_RUN_ISA="$SELECTED_ISA" "$@"
}

export PATH="$ROOT/.a5/tools/sail/bin:$ROOT/.a5/tools/gcc/bin:$PATH"
mkdir -p .a5/{tools,evidence,config}
python3 scripts/a5/test_runner.py
python3 scripts/a5/test_accounting.py
python3 scripts/a5/provision.py
python3 scripts/a5/inventory.py
if [[ "$ACCOUNTING" == "rv64i" ]]; then
  python3 scripts/a5/accounting.py prepare
fi
if [[ -n "$SELECTED_CONFIG" ]]; then
  case "$SELECTED_CONFIG" in
    ruscv-rv64imac-zca) python3 scripts/a5/inventory_zca.py ;;
    ruscv-rv64imafd) python3 scripts/a5/inventory_fd.py ;;
    ruscv-rv64imafdc-zcd) python3 scripts/a5/inventory_zcd.py ;;
  esac
  selected_accounting python3 scripts/a5/accounting.py prepare
fi
uv python install 3.12.12
cp scripts/a5/{test_config.yaml,ruscv-rv64i-smoke.yaml,rvmodel_macros.h,$RVTEST_CONFIG,link.ld} .a5/config/
if [[ "$RVTEST_CONFIG" != rvtest_config.h ]]; then
  # The FP profile declares F and D so ACT4 enables mstatus.FS before the
  # first F, D or Zcd instruction; the non-FP selections keep the unchanged
  # header that declares no floating-point support.
  mv .a5/config/$RVTEST_CONFIG .a5/config/rvtest_config.h
fi
if [[ -n "$SELECTED_CONFIG" ]]; then
  # The ACT4 framework config must point at the selected UDB config; the I
  # smoke config stays untouched and is only used by the other modes.
  case "$SELECTED_CONFIG" in
    ruscv-rv64imac-zca) cp scripts/a5/test_config_zca.yaml .a5/config/test_config.yaml ;;
    ruscv-rv64imafd) cp scripts/a5/test_config_fd.yaml .a5/config/test_config.yaml ;;
    ruscv-rv64imafdc-zcd) cp scripts/a5/test_config_zcd.yaml .a5/config/test_config.yaml ;;
  esac
  cp scripts/a5/"$SELECTED_CONFIG".yaml .a5/config/
fi
# Derive a complete Sail configuration from the pinned upstream schema example.
# This describes the oracle, NOT extra DUT capabilities. Disable optional ISAs.
python3 scripts/a5/sail_config.py
if [[ "$SAIL_SCRIPT" != none ]]; then
  # The selected oracle enables exactly the selected profile, and refuses to
  # write a configuration with any other enabled extension.
  python3 scripts/a5/"$SAIL_SCRIPT"
fi
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
python3 "$ROOT/scripts/a5/udb_overlay.py" "$(bundle show udb)" "$ROOT" ruscv-rv64i-smoke
if [[ -n "$SELECTED_CONFIG" ]]; then
  python3 "$ROOT/scripts/a5/udb_overlay.py" "$(bundle show udb)" "$ROOT" "$SELECTED_CONFIG"
fi
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
ACT_ENV=()
if [[ -n "$SELECTED_CONFIG" ]]; then
  ACT_ENV=(env "A5_CONFIG=$SELECTED_CONFIG")
fi
"${ACT_ENV[@]}" bundle exec uv run --frozen --python 3.12.12 --package act act \
  "$ROOT/.a5/config/test_config.yaml" --test-dir "$ROOT/.a5/selection" \
  --workdir "$ROOT/.a5/work" --extensions "$ACT_EXTENSIONS" --jobs 1 --verbose || generation_rc=$?
cd "$ROOT"
rustup toolchain install 1.93.1 --profile minimal
cargo +1.93.1 build --locked --release
python3 scripts/a5/validate_controls.py "$ROOT"
if [[ "$ACCOUNTING" == "rv64i" ]]; then
  # Control failure and required-case failure both fail the overall run.
  controls_rc=0
  suite_rc=0
  python3 scripts/a5/suite_controls.py || controls_rc=$?
  python3 scripts/a5/accounting.py run --generation-returncode "$generation_rc" || suite_rc=$?
  [[ "$controls_rc" == 0 && "$suite_rc" == 0 ]]
elif [[ "$ACCOUNTING" == "selected" ]]; then
  # Control failure and required-case failure both fail the overall run.
  controls_rc=0
  suite_rc=0
  python3 scripts/a5/suite_controls.py || controls_rc=$?
  selected_accounting python3 scripts/a5/accounting.py run \
    --generation-returncode "$generation_rc" || suite_rc=$?
  [[ "$controls_rc" == 0 && "$suite_rc" == 0 ]]
else
  [[ "$generation_rc" == 0 ]]
  python3 scripts/a5/run.py
fi
