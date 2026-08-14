#!/bin/bash
# Unit test for tsz_ensure_git_fixture (scripts/bench/project-fixtures.sh).
#
# #17469: nine external fixtures failed to pin because their upstream-rewritten
# commits were no longer served ("git fetch … not our ref"). The pin step did
# not check the fetch/checkout exit status, so it fell through and later
# `git -C "$dir" rev-parse HEAD` resolved against the ENCLOSING tsz checkout,
# printing "✓ <fixture> pinned at <tsz sha>" — a green check citing an
# unrelated repository's SHA — while the run reported success.
#
# These cases use local bare repos as the "upstream", so they need no network
# and exercise the failure paths deterministically:
#   1. an unreachable pin (a SHA the upstream does not serve) FAILS loudly and
#      never reports a HEAD from an enclosing repository;
#   2. a directory that is not its own git checkout is rejected;
#   3. the happy path (a served SHA) still pins and returns success.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/bench/project-fixtures.sh
source "$SCRIPT_DIR/project-fixtures.sh"

pass=0
fail=0

check() {
  local label="$1" expected="$2" actual="$3"
  if [ "$expected" = "$actual" ]; then
    pass=$((pass + 1))
    echo "  ok   $label"
  else
    fail=$((fail + 1))
    echo "  FAIL $label:"
    echo "    expected: $(printf '%q' "$expected")"
    echo "    actual:   $(printf '%q' "$actual")"
  fi
}

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

git_quiet() { git -c init.defaultBranch=main -c user.email=t@t -c user.name=t "$@"; }

# Build a local "upstream" bare repo with one served commit.
UPSTREAM="$WORK/upstream"
SEED="$WORK/seed"
mkdir -p "$SEED"
git_quiet init --quiet "$SEED"
echo "hello" >"$SEED/index.ts"
git_quiet -C "$SEED" add index.ts
git_quiet -C "$SEED" commit --quiet -m "served commit"
SERVED_SHA="$(git_quiet -C "$SEED" rev-parse HEAD)"
git_quiet clone --quiet --bare "$SEED" "$UPSTREAM" >/dev/null 2>&1

# A 40-hex SHA the upstream does NOT serve (mimics an upstream history rewrite).
UNREACHABLE_SHA="0123456789012345678901234567890123456789"

# --- Case 1: unreachable pin fails loudly and does not leak an enclosing SHA ---
# Run from inside THIS repo's working tree so a naive `git -C "$dir"` on a
# broken checkout would resolve to tsz's own repo — the #17469 aliasing.
FIXDIR1="$WORK/fixture-unreachable"
err1="$(tsz_ensure_git_fixture "demo" "$UPSTREAM" "$UNREACHABLE_SHA" "$FIXDIR1" 0 2>&1 1>/dev/null)"
rc1=$?
check "unreachable pin returns non-zero" "1" "$rc1"
if echo "$err1" | grep -q "ERROR:"; then
  check "unreachable pin prints a diagnostic" "yes" "yes"
else
  check "unreachable pin prints a diagnostic" "yes" "no ($err1)"
fi
# HEAD must not be resolvable to this repo's commit as if it were the fixture.
if tsz_git_fixture_is_standalone_repo "$FIXDIR1"; then
  landed="$(git -C "$FIXDIR1" rev-parse HEAD 2>/dev/null || echo none)"
  if [ "$landed" = "$UNREACHABLE_SHA" ]; then
    check "unreachable pin did not fabricate a HEAD" "clean" "FABRICATED"
  else
    check "unreachable pin did not fabricate a HEAD" "clean" "clean"
  fi
else
  check "unreachable pin did not fabricate a HEAD" "clean" "clean"
fi

# --- Case 2: a non-repo directory is rejected as not standalone ---
NONREPO="$WORK/not-a-repo"
mkdir -p "$NONREPO"
if tsz_git_fixture_is_standalone_repo "$NONREPO"; then
  check "non-repo dir rejected" "rejected" "accepted"
else
  check "non-repo dir rejected" "rejected" "rejected"
fi

# --- Case 2b: a failed CLONE (unreachable repo) must fail, not alias tsz ---
# This is the exact #17469 shape: the clone never produces a `.git`, so a naive
# `git -C "$dir" rev-parse HEAD` would walk up to the tsz checkout and report
# tsz's own SHA as the fixture's pin. Runs with CWD inside the tsz repo.
FIXDIR2B="$SCRIPT_DIR/.ensure-git-fixture-test-clonefail"
rm -rf "$FIXDIR2B"
if tsz_ensure_git_fixture "demo" "$WORK/does-not-exist.git" "$SERVED_SHA" "$FIXDIR2B" 0 >/dev/null 2>&1; then
  rc2b=0
else
  rc2b=1
fi
check "failed clone returns non-zero" "1" "$rc2b"
if tsz_git_fixture_is_standalone_repo "$FIXDIR2B"; then
  check "failed clone did not alias the tsz repo" "clean" "ALIASED"
else
  check "failed clone did not alias the tsz repo" "clean" "clean"
fi
rm -rf "$FIXDIR2B"

# --- Case 3: happy path pins the served SHA and returns success ---
FIXDIR3="$WORK/fixture-ok"
if tsz_ensure_git_fixture "demo" "$UPSTREAM" "$SERVED_SHA" "$FIXDIR3" 0 >/dev/null 2>&1; then
  rc3=0
else
  rc3=1
fi
check "served pin returns success" "0" "$rc3"
head3="$(git -C "$FIXDIR3" rev-parse HEAD 2>/dev/null || echo none)"
check "served pin lands on the requested SHA" "$SERVED_SHA" "$head3"

echo
echo "tsz_ensure_git_fixture: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
