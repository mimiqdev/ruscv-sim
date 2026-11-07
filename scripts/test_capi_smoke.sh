#!/bin/bash
# Build the C ABI static library and run the plain-C compile-and-link smoke.
#
# This is the dev-plan D11 control: the library is produced with
# `cargo rustc --lib --features capi --crate-type staticlib`, so no crate type
# and no C++ toolchain is added to a normal build, and a plain-C host can link
# it.  The smoke covers lifecycle, profile selection, the commit log, grant time
# fields and earlier-start rejection, wrong-thread and re-entry rejection,
# panic poisoning, a transport status that must not pass, an unknown completion
# followed by reset, and the direct-memory ABI: a grant, a NULL callback, a
# denial, invalidation between grants and inside a callback, wrong-thread
# invalidation rejection, the invalidation re-entry exception, and reset
# restoring grants after a poisoned transport.
#
# The device additions (device U3, D8) are covered from plain C too: the
# `ruscv_device` layout pins, the `ruscv_payload.device_event` field's offset and
# size, every attach refusal code, and a run whose C device target serves the
# UART window with one transmit event, plus the wrong-window tag that must fail
# the run.
#
# A missing C compiler is a failure, never a skip: CI installs gcc on the
# runner, so an absent compiler means the check did not run.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
PROJECT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd -P)"
CC="${CC:-gcc}"
OUTDIR="${CAPI_SMOKE_OUTDIR:-${PROJECT_DIR}/target/capi-smoke}"

if ! command -v "${CC}" >/dev/null 2>&1; then
    echo "error: no C compiler found (${CC}); the C ABI smoke cannot run" >&2
    exit 2
fi

case "${OUTDIR}" in
    /*) ;;
    *) OUTDIR="${PROJECT_DIR}/${OUTDIR}" ;;
esac
if [ "${OUTDIR}" = "/" ] || [ "${OUTDIR}" = "${PROJECT_DIR}" ]; then
    echo "error: refusing to use ${OUTDIR} as the smoke output directory" >&2
    exit 2
fi
mkdir -p "${OUTDIR}"

target_dir="${CARGO_TARGET_DIR:-${PROJECT_DIR}/target}"
case "${target_dir}" in
    /*) ;;
    *) target_dir="${PROJECT_DIR}/${target_dir}" ;;
esac
library="${target_dir}/debug/libruscv_sim.a"

echo "== building the C ABI static library"
(
    cd "${PROJECT_DIR}"
    cargo rustc --lib --features capi --crate-type staticlib
)
if [ ! -f "${library}" ]; then
    echo "error: expected static library not found at ${library}" >&2
    exit 2
fi

echo "== compiling and linking the plain-C smoke with ${CC}"
"${CC}" -std=c11 -Wall -Wextra -Werror -O1 \
    -I "${PROJECT_DIR}/include" \
    "${PROJECT_DIR}/tests/capi/smoke.c" \
    "${library}" \
    -lpthread -ldl -lm \
    -o "${OUTDIR}/capi_smoke"

echo "== running the plain-C smoke"
"${OUTDIR}/capi_smoke" "${OUTDIR}"
