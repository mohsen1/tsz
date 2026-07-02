#!/usr/bin/env bash
# safe-run-test.sh — self-test for scripts/safe-run.sh
#
# Exercises status/output passthrough, prompt monitor teardown (#15439),
# hung-footprint probe recovery, memory-limit enforcement, and signal
# forwarding with fake commands so it runs fast and hermetically on any
# platform. The macOS-only paths (Darwin detection, `footprint`) are
# driven through fake `uname`/`footprint` executables on PATH. Run locally:
#
#   scripts/test/safe-run-test.sh
#
# Exits non-zero if any assertion failed.

set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SAFE_RUN="$HERE/../safe-run.sh"

WORK="$(mktemp -d "${TMPDIR:-/tmp}/safe-run-test.XXXXXX")"
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

# Marker sleep durations make stray processes attributable to one test
# and let anchored pgrep patterns avoid matching this script itself.
FOOTPRINT_MARK=86399
CHILD_MARK=86398

cleanup() {
    local p
    for p in $(pgrep -f "^sleep ($FOOTPRINT_MARK|$CHILD_MARK)\$" 2>/dev/null); do
        kill -9 "$p" 2>/dev/null || true
    done
    rm -rf "$WORK"
}
trap cleanup EXIT

now() { date +%s; }

kill_test_tree() {
    local pid=$1 child
    for child in $(pgrep -P "$pid" 2>/dev/null); do
        kill_test_tree "$child"
    done
    kill -9 "$pid" 2>/dev/null || true
}

# `timeout` is not part of stock macOS; fall back to a watchdog so the
# self-test still bounds a hang there.
run_bounded() {
    local secs=$1
    shift
    if command -v timeout >/dev/null 2>&1; then
        timeout "$secs" "$@"
        return $?
    fi
    "$@" &
    local pid=$!
    (
        sleep "$secs"
        kill_test_tree "$pid"
    ) &
    local watchdog=$!
    wait "$pid"
    local rc=$?
    kill_test_tree "$watchdog"
    wait "$watchdog" 2>/dev/null
    return $rc
}

# Fake Darwin environment: `uname -s` reports Darwin and `footprint`
# hangs forever, reproducing a wedged probe (#15439).
FAKEBIN="$WORK/bin"
mkdir -p "$FAKEBIN"
printf '#!/usr/bin/env bash\necho Darwin\n' >"$FAKEBIN/uname"
printf '#!/usr/bin/env bash\nexec sleep %s\n' "$FOOTPRINT_MARK" >"$FAKEBIN/footprint"
chmod +x "$FAKEBIN/uname" "$FAKEBIN/footprint"

# Child that holds ~20MB of RSS while sleeping. The trailing `:` stops
# bash from exec-replacing itself with `sleep` (which would drop the
# allocation).
HEAVY_CHILD='x=$(head -c 20000000 /dev/zero | tr "\0" "a"); sleep 60; :'

# ── 1. Exit code, argv, and stdout are forwarded ────────────────────
out="$("$SAFE_RUN" --interval 1 -- bash -c 'echo "hi $1"; exit 7' _ world 2>/dev/null)"
rc=$?
[[ "$rc" -eq 7 ]] && ok "forwards exit code" || bad "exit code ($rc != 7)"
[[ "$out" == "hi world" ]] && ok "forwards args/stdout" || bad "stdout ('$out')"

# ── 2. Wrapper and pipe consumers finish promptly after child exit ──
# Regression for #15439: the monitor (mid `sleep 60`) must be torn down
# with its children, or the orphaned sleep holds the stdout/stderr pipe
# open and `cat` blocks for the rest of the interval.
start=$(now)
run_bounded 30 bash -c "'$SAFE_RUN' --interval 60 -- sleep 1 2>&1 | cat" >/dev/null
rc=$?
elapsed=$(($(now) - start))
[[ "$rc" -eq 0 && "$elapsed" -lt 20 ]] \
    && ok "pipeline gets EOF promptly after child exit (${elapsed}s)" \
    || bad "pipeline blocked after child exit (rc=$rc, ${elapsed}s)"

# ── 3. Hung footprint probe: no wrapper hang, no orphans ────────────
# Regression for #15439: with `footprint` wedged, the wrapper must
# still return the child's status promptly and reap the probe tree.
start=$(now)
banner="$WORK/hung-footprint.stderr"
run_bounded 40 bash -c "PATH='$FAKEBIN':\$PATH '$SAFE_RUN' --interval 1 -- sleep 2 2>'$banner' | cat" >/dev/null
rc=$?
elapsed=$(($(now) - start))
[[ "$rc" -eq 0 && "$elapsed" -lt 20 ]] \
    && ok "returns promptly despite hung footprint probe (${elapsed}s)" \
    || bad "hung footprint probe wedged the wrapper (rc=$rc, ${elapsed}s)"
grep -q "mode physical footprint" "$banner" \
    && ok "fake Darwin engaged the footprint path" \
    || bad "footprint path not engaged; test env broken"
sleep 1
if pgrep -f "^sleep $FOOTPRINT_MARK\$" >/dev/null 2>&1; then
    bad "orphaned footprint probe processes remain"
else
    ok "no orphaned probe processes after exit"
fi

# ── 4. Probe timeout falls back to RSS and downgrades the mode ──────
stderr="$WORK/downgrade.stderr"
start=$(now)
run_bounded 40 env PATH="$FAKEBIN:$PATH" SAFE_RUN_PROBE_TIMEOUT_SECS=1 \
    "$SAFE_RUN" --interval 1 -- sleep 8 2>"$stderr"
rc=$?
elapsed=$(($(now) - start))
[[ "$rc" -eq 0 ]] && ok "survives probe timeouts (rc=$rc, ${elapsed}s)" \
    || bad "probe timeouts broke the run (rc=$rc, ${elapsed}s)"
grep -q "switching to RSS" "$stderr" \
    && ok "downgrades to RSS after repeated probe failures" \
    || bad "no RSS downgrade after repeated probe failures"

# ── 5. Memory limit enforcement still kills the tree (RSS mode) ─────
start=$(now)
stderr="$WORK/limit.stderr"
run_bounded 40 "$SAFE_RUN" --limit 5 --interval 1 -- bash -c "$HEAVY_CHILD" 2>"$stderr"
rc=$?
elapsed=$(($(now) - start))
[[ "$rc" -ne 0 && "$elapsed" -lt 30 ]] \
    && ok "kills over-limit child (rc=$rc, ${elapsed}s)" \
    || bad "over-limit child not killed (rc=$rc, ${elapsed}s)"
grep -q "MEMORY LIMIT EXCEEDED" "$stderr" \
    && ok "reports the limit breach" \
    || bad "missing limit-breach report"

# ── 6. Limit enforcement works while the footprint probe is wedged ──
# The bounded probe must fail over to an RSS sample on the same tick so
# a hung `footprint` never disables the memory guard.
start=$(now)
run_bounded 40 env PATH="$FAKEBIN:$PATH" SAFE_RUN_PROBE_TIMEOUT_SECS=1 \
    "$SAFE_RUN" --limit 5 --interval 1 -- bash -c "$HEAVY_CHILD" 2>/dev/null
rc=$?
elapsed=$(($(now) - start))
[[ "$rc" -ne 0 && "$elapsed" -lt 30 ]] \
    && ok "enforces limit despite hung probe (rc=$rc, ${elapsed}s)" \
    || bad "hung probe disabled enforcement (rc=$rc, ${elapsed}s)"

# ── 7. TERM is forwarded to the child tree ──────────────────────────
"$SAFE_RUN" --interval 1 -- sleep "$CHILD_MARK" 2>/dev/null &
wrapper=$!
sleep 1
kill -TERM "$wrapper" 2>/dev/null
start=$(now)
wait "$wrapper" 2>/dev/null
rc=$?
elapsed=$(($(now) - start))
[[ "$rc" -ne 0 && "$elapsed" -lt 10 ]] \
    && ok "TERM'd wrapper exits promptly (rc=$rc, ${elapsed}s)" \
    || bad "TERM'd wrapper lingered (rc=$rc, ${elapsed}s)"
sleep 1
if pgrep -f "^sleep $CHILD_MARK\$" >/dev/null 2>&1; then
    bad "child survived forwarded TERM"
else
    ok "child tree gone after forwarded TERM"
fi

# ── 8. Bad option values fail fast instead of exploding later ───────
"$SAFE_RUN" --interval bogus -- true 2>/dev/null
[[ $? -ne 0 ]] && ok "rejects non-numeric --interval" \
    || bad "accepted non-numeric --interval"
"$SAFE_RUN" --limit bogus -- true 2>/dev/null
[[ $? -ne 0 ]] && ok "rejects non-numeric --limit" \
    || bad "accepted non-numeric --limit"
# A typo'd percentage must not slip through as a 0MB limit.
"$SAFE_RUN" --limit bogus% -- true 2>/dev/null
[[ $? -ne 0 ]] && ok "rejects non-numeric percentage --limit" \
    || bad "accepted non-numeric percentage --limit"

echo
echo "passed: $PASS, failed: $FAIL"
[[ "$FAIL" -eq 0 ]]
