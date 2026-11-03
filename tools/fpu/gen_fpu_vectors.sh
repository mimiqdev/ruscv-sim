#!/usr/bin/env bash
# Regenerate the F/D reference vectors from pinned Berkeley SoftFloat 3e.
#
# Usage:
#   tools/fpu/gen_fpu_vectors.sh           # rewrite tools/fpu/fpu-reference-v1.tsv
#   tools/fpu/gen_fpu_vectors.sh --check   # verify the committed file is reproducible
#
# The download is pinned by SHA-256. Only regeneration needs network access;
# `cargo test` reads the committed TSV.
set -euo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
work=$(mktemp -d "${TMPDIR:-/tmp}/ruscv-fpu-vectors.XXXXXX")
trap 'rm -rf "$work"' EXIT

readonly SOFTFLOAT_VERSION=3e
readonly SOFTFLOAT_URL="https://www.jhauser.us/arithmetic/SoftFloat-${SOFTFLOAT_VERSION}.zip"
readonly SOFTFLOAT_SHA256=21130ce885d35c1fe73fc1e1bf2244178167e05c6747cad5f450cc991714c746

zip="$work/SoftFloat-$SOFTFLOAT_VERSION.zip"
curl -fsSL "$SOFTFLOAT_URL" -o "$zip"
echo "$SOFTFLOAT_SHA256  $zip" | sha256sum --check --status
echo "verified SoftFloat-$SOFTFLOAT_VERSION.zip ($SOFTFLOAT_SHA256)"

unzip -q "$zip" -d "$work"
src="$work/SoftFloat-$SOFTFLOAT_VERSION"
build="$src/build/Linux-x86_64-GCC"

make -C "$build" -j"$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 2)" softfloat.a >/dev/null

gcc -O2 \
    -I"$src/source/include" \
    -o "$work/gen_fpu_vectors" \
    "$here/gen_fpu_vectors.c" \
    "$build/softfloat.a" \
    -lm

"$work/gen_fpu_vectors" > "$work/fpu-reference-v1.tsv"

if [[ "${1:-}" == "--check" ]]; then
    cmp "$work/fpu-reference-v1.tsv" "$here/fpu-reference-v1.tsv"
    echo "tools/fpu/fpu-reference-v1.tsv is reproducible"
else
    mv "$work/fpu-reference-v1.tsv" "$here/fpu-reference-v1.tsv"
    echo "wrote tools/fpu/fpu-reference-v1.tsv"
fi
