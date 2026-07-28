#!/usr/bin/env bash
# Measure a branch's conformance delta against its PARENT COMMIT.
#
# Why this exists: the obvious thing to diff against is the checked-in
# `conformance-baseline.txt`, and that is always wrong. That file is a
# chore-refreshed artifact (see `git log -- scripts/conformance/conformance-baseline.txt`)
# whose content reflects whatever main looked like when someone last ran a
# refresh PR. Diffing a branch against it mixes your change together with every
# merge that landed in between — in one measured case it reported 70 newly
# passing rows for a change that flipped none, and in another it showed 4
# "newly failing" tests that may simply have been broken on main already.
#
# The only sound comparison is: same methodology, both sides, parent vs branch.
# That needs two builds and two snapshots, which is exactly why people skip it.
# This script makes it one command.
#
#   scripts/conformance/compare-to-parent.sh                 # HEAD vs HEAD~1
#   scripts/conformance/compare-to-parent.sh <base-ref>      # HEAD vs <base-ref>
#
# Prints newly-passing and newly-failing test names. NEWLY FAILING IS THE GATE:
# a rising total can still hide a regression, because a fix that flips two rows
# while breaking one still nets +1.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

BASE_REF="${1:-HEAD~1}"
BRANCH_REF="$(git rev-parse --abbrev-ref HEAD)"
[ "$BRANCH_REF" = "HEAD" ] && BRANCH_REF="$(git rev-parse HEAD)"

if ! git diff --quiet || ! git diff --cached --quiet; then
    echo "error: working tree is dirty. Commit or stash first — this script checks out $BASE_REF." >&2
    exit 1
fi

BASE_SHA="$(git rev-parse "$BASE_REF")"
WORK="$(mktemp -d)"
trap 'git checkout -q "$BRANCH_REF" 2>/dev/null || true; git checkout -q -- scripts/conformance/ 2>/dev/null || true; rm -rf "$WORK"' EXIT

snapshot_into() { # $1 = destination file
    cargo build --profile dist-fast --target-dir .target -p tsz-cli --bin tsz 2>&1 \
        | grep -E '^error' && { echo "error: build failed" >&2; exit 1; }
    ./scripts/conformance/conformance.sh snapshot --workers 12 --force 2>&1 \
        | grep -E 'Snapshot saved' || true
    cp scripts/conformance/conformance-baseline.txt "$1"
    git checkout -q -- scripts/conformance/
}

echo "==> branch $BRANCH_REF"
snapshot_into "$WORK/branch.txt"

echo "==> base $BASE_REF ($BASE_SHA)"
git checkout -q "$BASE_SHA"
snapshot_into "$WORK/base.txt"
git checkout -q "$BRANCH_REF"

python3 - "$WORK/base.txt" "$WORK/branch.txt" <<'PY'
import re, sys

def fails(path):
    out = {}
    for line in open(path):
        if line.startswith('FAIL '):
            m = re.match(r'FAIL (\S+) \|(.*)', line.strip())
            if m:
                out[m.group(1)] = m.group(2).strip()
    return out

base, branch = fails(sys.argv[1]), fails(sys.argv[2])
newly_passing = sorted(set(base) - set(branch))
newly_failing = sorted(set(branch) - set(base))
changed = [t for t in set(base) & set(branch) if base[t] != branch[t]]

short = lambda t: t.rsplit('/', 1)[-1]
print(f"\nbase failing={len(base)}  branch failing={len(branch)}")
print(f"\nNEWLY PASSING ({len(newly_passing)}):")
for t in newly_passing:
    print("   +", short(t))
if not newly_passing:
    print("   (none)")
print(f"\nNEWLY FAILING ({len(newly_failing)}):")
for t in newly_failing:
    print("   -", short(t), "|", branch[t][:90])
if not newly_failing:
    print("   (none)")
print(f"\nstill failing, fingerprint changed ({len(changed)}):")
for t in sorted(changed)[:10]:
    print("   ~", short(t))

sys.exit(1 if newly_failing else 0)
PY
