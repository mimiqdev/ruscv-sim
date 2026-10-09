#!/bin/bash
# Exact committed P3 calibrated verification. Pins the required-toolchain
# image, records exact clean HEAD, runs pinned P0-P2/A9 gates, then executes
# the public calibrated sequence: ONE continuing inspected allocation, three
# independent session invocations, cohort/baseline binding, and one
# independently produced same-revision candidate comparison. Informational
# only: a successful wrapper is NOT a speed/regression PASS.
set -u
cd "$(dirname "$0")/../.."
ROOT="$(pwd -P)"
HEAD="$(git rev-parse HEAD)"
TREE="$(git rev-parse HEAD^{tree})"
COMMON="$(git rev-parse --path-format=absolute --git-common-dir)"
IMAGE='ghcr.io/mimiqdev/ruscv-sim-dev@sha256:cc3cfea2499f69d2ee91fc711fb646807a08d8160148c00303d2fa92e3e9a65c'
test -z "$(git status --porcelain)" || { echo 'INCONCLUSIVE: clean committed source required' >&2; exit 2; }
test "$(uname -s)" = Darwin || { echo 'INCONCLUSIVE: calibrated policy requires the inspected local Darwin/Colima launcher' >&2; exit 2; }
docker image inspect "$IMAGE" --format 'pinned image {{.Id}} {{.Os}}/{{.Architecture}}' >&2
mkdir -p target
OUT="$(mktemp -d "$ROOT/target/a10-p3-verify-XXXXXX")"
exec > >(tee "$OUT/verification.log") 2>&1
fail(){ echo "P3 VERIFICATION FAILED AT: $1" >&2; exit 1; }
date -u '+%Y-%m-%dT%H:%M:%SZ'
printf 'source_head=%s\nsource_tree=%s\n' "$HEAD" "$TREE"
uname -a
PYTHONDONTWRITEBYTECODE=1 python3 tools/a10/collect.py "$OUT/physical-host.json" || fail 'native host inspection'
echo '== stage 1: pinned-toolchain gates, P0-P2/A9 regressions, fixed-base scope audit =='
bash tools/a10/verify_p2.sh || fail 'pinned P2 gate wrapper'
echo '== stage 2: one continuing inspected allocation with the inspected tmpfs sink =='
BASE="$OUT/collection"
mkdir -p "$BASE"
CID="$(docker create --init --volume "$ROOT:$ROOT" --volume "$COMMON:$COMMON:ro" --workdir "$ROOT" \
  --env GIT_OPTIONAL_LOCKS=0 --env GIT_CONFIG_COUNT=1 --env GIT_CONFIG_KEY_0=safe.directory --env "GIT_CONFIG_VALUE_0=$ROOT" \
  --env CARGO_BUILD_JOBS=2 --env PYTHONDONTWRITEBYTECODE=1 \
  --env RISCV_REQUIRE_RISCV_TOOLCHAIN=1 --env RISCV_REQUIRE_A10_PINNED_TOOLS=1 \
  --tmpfs "$BASE/local-sinks:rw,size=128m" \
  "$IMAGE" sleep 86400)" || fail 'continuing allocation create'
docker start "$CID" >/dev/null || fail 'continuing allocation start'
docker inspect "$CID" > "$OUT/container-inspect.json"
export RISCV_PERF_CALIBRATION_CONTAINER="$CID"
export RISCV_PERF_CALIBRATION_SINKS_ROOT="$BASE/local-sinks"
echo "continuing allocation $CID; sinks root $BASE/local-sinks"
echo '== stage 3: three independent sessions plus one independently produced candidate =='
for i in 1 2 3 4; do
  code=0
  ./scripts/perf-test.sh run --suite public-v1 --profile calibrated --out "$BASE/session-$i" || code=$?
  test "$code" = 2 || fail "session-$i invocation exit $code (2=valid inconclusive session expected)"
  test -f "$BASE/session-$i/report.json" || fail "session-$i report missing"
  ( cd "$BASE/session-$i" && sha256sum report.json bundle.json bundle.sha256 ) || fail "session-$i digests"
done
echo '== stage 4: cohort binding and baseline sealing =='
code=0
./scripts/perf-test.sh cohort --reports "$BASE/session-1/report.json" "$BASE/session-2/report.json" "$BASE/session-3/report.json" --out "$OUT/cohort" || code=$?
test "$code" = 0 || fail "cohort exit $code (usable cohort required)"
echo '== stage 5: independent same-revision comparison (informational) =='
code=0
./scripts/perf-test.sh compare --baseline "$OUT/cohort/baseline.json" --candidate "$BASE/session-4/report.json" --out "$OUT/comparison" || code=$?
test "$code" = 0 || fail "comparison exit $code"
echo '== stage 6: observation record and exact-HEAD recheck =='
PYTHONDONTWRITEBYTECODE=1 python3 - "$OUT" "$BASE" "$HEAD" "$TREE" "$IMAGE" "$CID" <<'PY'
import hashlib, json, os, subprocess, sys
from pathlib import Path
out=Path(sys.argv[1]); base=Path(sys.argv[2]); head=sys.argv[3]; tree=sys.argv[4]
image=sys.argv[5]; cid=sys.argv[6]
def sha(p): return hashlib.sha256(Path(p).read_bytes()).hexdigest()
members={}
for i in (1,2,3,4):
    root=base/f'session-{i}'
    members[f'session-{i}']={'report_sha256':sha(root/'report.json'),'bundle_sha256':(root/'bundle.sha256').read_text().strip(),'exit2':True}
cohort={'cohort_sha256':sha(out/'cohort'/'cohort.json'),'baseline_sha256':sha(out/'cohort'/'baseline.json'),'bundle_sha256':(out/'cohort'/'bundle.sha256').read_text().strip()}
comparison={'comparison_sha256':sha(out/'comparison'/'comparison.json')}
observation={'kind':'a10-p3-calibration/1','source_head':head,'source_tree':tree,'pinned_image':image,'continuing_container':cid,
             'members':members,'cohort':cohort,'comparison':comparison,
             'scope':'informational calibration observation; no speed/regression PASS','status':'completed'}
json_new=out/'observation.json'
with json_new.open('x') as file:
    file.write(json.dumps(observation,sort_keys=True,separators=(',',':'))+'\n')
print('observation', json.dumps(observation,sort_keys=True))
PY
test "$HEAD" = "$(git rev-parse HEAD)" || fail 'HEAD changed during verification'
test -z "$(git status --porcelain)" || fail 'worktree dirtied during verification'
git diff --check e73b12b8467fd635b398382a5cbc7ce75d842f68..HEAD || fail 'fixed-base diff check'
date -u '+%Y-%m-%dT%H:%M:%SZ'
printf 'P3 exact-HEAD calibration observation completed at %s; informational only, NO performance PASS\n' "$HEAD"
printf 'P3 retained local artifacts: %s\n' "$OUT"
