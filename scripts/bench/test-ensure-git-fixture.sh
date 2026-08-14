#!/bin/bash
# Unit test for tsz_ensure_git_fixture / tsz_git_fixture_is_standalone_repo
# (scripts/bench/project-fixtures.sh).
#
# #17469: nine external fixtures failed to pin because their upstream commits
# were transiently unreachable ("git fetch ... not our ref"). The pin step
# did not check the fetch/checkout exit status, so it fell through, and a
# fixture directory left without its own `.git` (a failed clone, or a
# checkout that never landed) let a later `git -C "$dir" rev-parse HEAD`
# resolve against the ENCLOSING tsz checkout instead — printing
# "pinned at <tsz's own sha>", a green result citing an unrelated
# repository, while the run reported success.
#
# These cases use local bare repos as the "upstream" so they need no network
# and exercise the failure paths deterministically.

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

# Local "upstream" bare repo with one commit it actually serves.
SEED="$WORK/seed"
mkdir -p "$SEED"
git_quiet init --quiet "$SEED"
echo "hello" >"$SEED/index.ts"
git_quiet -C "$SEED" add index.ts
git_quiet -C "$SEED" commit --quiet -m "served commit"
SERVED_SHA="$(git_quiet -C "$SEED" rev-parse HEAD)"
UPSTREAM="$WORK/upstream.git"
git_quiet clone --quiet --bare "$SEED" "$UPSTREAM" >/dev/null 2>&1

# A syntactically valid 40-hex SHA the upstream does not serve (mimics an
# upstream history rewrite / GC / transient outage).
UNREACHABLE_SHA="0123456789012345678901234567890123456789"

# --- Case 1: an unreachable pin fails loudly and fabricates nothing -------
# Run with CWD inside THIS repo so a naive `git -C "$dir"` on a broken
# checkout would alias tsz's own repository, exactly like #17469.
FIXDIR1="$WORK/fixture-unreachable"
err1="$(tsz_ensure_git_fixture "demo" "$UPSTREAM" "$UNREACHABLE_SHA" "$FIXDIR1" 0 2>&1 1>/dev/null)"
rc1=$?
check "unreachable pin returns non-zero" "1" "$rc1"
case "$err1" in
  *"ERROR:"*) check "unreachable pin prints a diagnostic" "yes" "yes" ;;
  *) check "unreachable pin prints a diagnostic" "yes" "no ($err1)" ;;
esac
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

# --- Case 2: a bare non-repo directory is rejected as not standalone ------
NONREPO="$WORK/not-a-repo"
mkdir -p "$NONREPO"
if tsz_git_fixture_is_standalone_repo "$NONREPO"; then
  check "non-repo dir rejected as standalone" "rejected" "accepted"
else
  check "non-repo dir rejected as standalone" "rejected" "rejected"
fi

# --- Case 3: a failed clone must fail, not alias the enclosing tsz repo ---
# Exercised with CWD inside the tsz checkout: the clone never produces a
# `.git`, so a naive `git -C "$dir" rev-parse HEAD` would walk up to tsz's
# own repository and report tsz's SHA as the fixture's pin.
FIXDIR3="$SCRIPT_DIR/.ensure-git-fixture-test-clonefail"
rm -rf "$FIXDIR3"
if tsz_ensure_git_fixture "demo" "$WORK/does-not-exist.git" "$SERVED_SHA" "$FIXDIR3" 0 >/dev/null 2>&1; then
  rc3=0
else
  rc3=1
fi
check "failed clone returns non-zero" "1" "$rc3"
if tsz_git_fixture_is_standalone_repo "$FIXDIR3"; then
  check "failed clone did not alias the tsz repo" "clean" "ALIASED"
else
  check "failed clone did not alias the tsz repo" "clean" "clean"
fi
rm -rf "$FIXDIR3"

# --- Case 4: the happy path still pins the served SHA and returns success -
FIXDIR4="$WORK/fixture-ok"
if tsz_ensure_git_fixture "demo" "$UPSTREAM" "$SERVED_SHA" "$FIXDIR4" 0 >/dev/null 2>&1; then
  rc4=0
else
  rc4=1
fi
check "served pin returns success" "0" "$rc4"
head4="$(git -C "$FIXDIR4" rev-parse HEAD 2>/dev/null || echo none)"
check "served pin lands on the requested SHA" "$SERVED_SHA" "$head4"

echo
echo "tsz_ensure_git_fixture: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
