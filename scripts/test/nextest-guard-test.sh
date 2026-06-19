#!/usr/bin/env bash
# nextest-guard-test.sh — self-test for scripts/test/nextest-guard.sh
#
# Exercises the lock/serialize/steal/reap behavior with fake commands so it runs
# fast and without invoking cargo. Run locally:
#
#   scripts/test/nextest-guard-test.sh
#
# Exits non-zero on the first failing assertion.

set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GUARD="$HERE/nextest-guard.sh"

WORK="$(mktemp -d "${TMPDIR:-/tmp}/nextest-guard-test.XXXXXX")"
export TMPDIR="$WORK" # isolate lock root from any real runs
PASS=0
FAIL=0

ok() {
    PASS=$((PASS + 1))
    echo "ok   - $1"
}
bad() {
    FAIL=$((FAIL + 1))
    echo "FAIL - $1" >&2
}

cleanup() {
    pkill -P $$ 2>/dev/null || true
    rm -rf "$WORK"
}
trap cleanup EXIT

# ── 1. Basic passthrough: exit code and args are forwarded ─────────
out="$("$GUARD" --lock-name t1 -- bash -c 'echo "hi $1"; exit 7' _ world)"
rc=$?
[[ "$rc" -eq 7 ]] && ok "forwards exit code" || bad "exit code ($rc != 7)"
[[ "$out" == "hi world" ]] && ok "forwards args/stdout" || bad "stdout ('$out')"

# ── 2. Lock is released after a normal run ─────────────────────────
"$GUARD" --lock-name t2 -- true
shopt -s nullglob
leftovers=("$WORK"/tsz-nextest-guard/t2*.lock)
[[ "${#leftovers[@]}" -eq 0 ]] && ok "lock released after success" \
    || bad "lock dir left behind: ${leftovers[*]}"
shopt -u nullglob

# ── 3. Serialization: a second run waits for the first ─────────────
start="${EPOCHSECONDS:-$(date +%s)}"
"$GUARD" --lock-name t3 -- sleep 3 &
first=$!
sleep 1 # ensure the first run owns the lock
"$GUARD" --lock-name t3 --interval 1 -- true
second_end="${EPOCHSECONDS:-$(date +%s)}"
wait "$first"
waited=$((second_end - start))
[[ "$waited" -ge 3 ]] && ok "second run serialized behind first (${waited}s)" \
    || bad "second run did not wait (${waited}s < 3s)"

# ── 4. --no-wait fails fast (exit 75) when the lock is live ─────────
"$GUARD" --lock-name t4 -- sleep 3 &
held=$!
sleep 1
"$GUARD" --lock-name t4 --no-wait -- true
nw=$?
[[ "$nw" -eq 75 ]] && ok "--no-wait returns 75 on a live lock" \
    || bad "--no-wait returned $nw (expected 75)"
kill "$held" 2>/dev/null || true
wait "$held" 2>/dev/null || true

# ── 5. Stale-lock steal: a dead holder's lock is taken over ────────
lockdir="$WORK/tsz-nextest-guard/t5.lock"
mkdir -p "$lockdir"
# Pick a PID that is almost certainly dead.
deadpid=999999
while kill -0 "$deadpid" 2>/dev/null; do deadpid=$((deadpid - 1)); done
echo "$deadpid" >"$lockdir/pid"
echo "stale-token" >"$lockdir/token"
out="$("$GUARD" --lock-name t5 --interval 1 --timeout 10 -- echo stole)"
rc=$?
[[ "$rc" -eq 0 && "$out" == "stole" ]] && ok "steals stale (dead-holder) lock" \
    || bad "did not steal stale lock (rc=$rc out='$out')"

# ── 6. Reap on steal: a recorded child of a dead holder is killed ──
lockdir="$WORK/tsz-nextest-guard/t6.lock"
mkdir -p "$lockdir"
# Long-lived fake orchestrator whose comm matches the reuse-guard allowlist.
# Use a copy of `sleep` renamed to contain "nextest" so ps comm matches.
fakebin="$WORK/cargo-nextest-fake"
cp "$(command -v sleep)" "$fakebin" 2>/dev/null && mv "$fakebin" "$WORK/nextest-sleep" || true
if [[ -x "$WORK/nextest-sleep" ]]; then
    "$WORK/nextest-sleep" 30 &
else
    sleep 30 &
fi
child=$!
echo "$deadpid" >"$lockdir/pid"
echo "stale-token" >"$lockdir/token"
echo "$child" >"$lockdir/child"
# Only assert reaping when the comm matches the allowlist (rename succeeded).
if [[ -x "$WORK/nextest-sleep" ]]; then
    "$GUARD" --lock-name t6 --interval 1 --timeout 10 -- true
    sleep 1
    if kill -0 "$child" 2>/dev/null; then
        bad "recorded orchestrator child was not reaped"
        kill "$child" 2>/dev/null || true
    else
        ok "reaped recorded orchestrator child on stale steal"
    fi
else
    "$GUARD" --lock-name t6 --interval 1 --timeout 10 -- true
    kill "$child" 2>/dev/null || true
    ok "stale steal proceeds (reuse-guard rename unavailable; reap not asserted)"
fi
wait "$child" 2>/dev/null || true

# ── 7. Distinct lock names do not serialize against each other ─────
start="${EPOCHSECONDS:-$(date +%s)}"
"$GUARD" --lock-name t7a -- sleep 2 &
a=$!
sleep 1
"$GUARD" --lock-name t7b -- true
end="${EPOCHSECONDS:-$(date +%s)}"
wait "$a"
[[ $((end - start)) -lt 2 ]] && ok "distinct lock keys run concurrently" \
    || bad "distinct lock keys serialized unexpectedly"

echo
echo "passed: $PASS, failed: $FAIL"
[[ "$FAIL" -eq 0 ]]
