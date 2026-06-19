#!/usr/bin/env bash
# nextest-guard.sh — serialize cargo-nextest runs and self-heal stale orchestrators
#
# Background (issue #13982): on developer machines `cargo nextest run` can wedge
# at ~0% CPU — the `cargo-nextest` orchestrator stays alive but idle, no `rustc`
# running and no test output, indefinitely. Two triggers were observed:
#
#   1. Concurrent nextest invocations (reproduced even with separate
#      CARGO_TARGET_DIR values).
#   2. Killing a nextest mid-build, which leaves the orchestrator in a hung
#      state so the *next* run re-wedges.
#
# The reliable manual workaround is "serialize nextest, never kill it
# mid-build." This wrapper encodes that workaround as reusable tooling:
#
#   * It holds a host-wide advisory lock for the duration of the wrapped
#     command, so a second guarded invocation queues behind the first instead
#     of racing the orchestrator into a wedge.
#   * On startup, if the previous lock holder died abnormally (e.g. it was
#     killed mid-build), the guard reaps the orchestrator process tree that the
#     prior guarded run recorded — and only that tree, never an arbitrary
#     `cargo-nextest` it did not start — then takes the lock and proceeds.
#
# The lock is advisory: it only constrains commands launched *through* this
# guard. It does not change test behavior or output, so it is safe to wire into
# any local nextest command. It composes with scripts/safe-run.sh in either
# order, e.g.
#
#   scripts/test/nextest-guard.sh -- scripts/safe-run.sh -- cargo nextest run ...
#
# Usage:
#   scripts/test/nextest-guard.sh [OPTIONS] [--] COMMAND [ARGS...]
#
# Options:
#   --scope global|target  Lock scope. "global" (default) serializes every
#                           guarded nextest on the host — the safe choice, since
#                           the wedge reproduces even across distinct target
#                           dirs. "target" serializes only runs that share the
#                           resolved CARGO_TARGET_DIR.
#   --timeout SECONDS      Max time to wait for the lock before giving up
#                           (default: 1800; 0 = wait indefinitely).
#   --interval SECONDS     Poll interval while waiting (default: 2).
#   --no-wait              Fail immediately (exit 75) if the lock is held by a
#                           live run instead of waiting.
#   --lock-name NAME       Override the lock key (advanced/testing).
#   --verbose              Log lock acquisition / steal / release to stderr.
#
# Exit codes:
#   The wrapped command's exit code on success.
#   75 (EX_TEMPFAIL)  could not acquire the lock (timeout or --no-wait).
#    1                usage error.

set -uo pipefail

# ─── Defaults ────────────────────────────────────────────────────────

SCOPE="global"
TIMEOUT=1800
INTERVAL=2
NO_WAIT=0
LOCK_NAME=""
VERBOSE=0

log() {
    [[ "$VERBOSE" -eq 1 ]] && echo "[nextest-guard] $*" >&2
    return 0
}

warn() {
    echo "[nextest-guard] $*" >&2
}

# ─── Parse options ───────────────────────────────────────────────────

while [[ $# -gt 0 ]]; do
    case "$1" in
        --scope)
            SCOPE="${2:-}"
            shift 2
            ;;
        --timeout)
            TIMEOUT="${2:-}"
            shift 2
            ;;
        --interval)
            INTERVAL="${2:-}"
            shift 2
            ;;
        --no-wait)
            NO_WAIT=1
            shift
            ;;
        --lock-name)
            LOCK_NAME="${2:-}"
            shift 2
            ;;
        --verbose)
            VERBOSE=1
            shift
            ;;
        --)
            shift
            break
            ;;
        -*)
            warn "unknown option: $1"
            exit 1
            ;;
        *)
            break
            ;;
    esac
done

if [[ $# -eq 0 ]]; then
    warn "usage: nextest-guard.sh [--scope global|target] [--timeout S] [--interval S] [--no-wait] [--] COMMAND [ARGS...]"
    exit 1
fi

case "$SCOPE" in
    global | target) ;;
    *)
        warn "invalid --scope '$SCOPE' (expected 'global' or 'target')"
        exit 1
        ;;
esac

for n in "$TIMEOUT" "$INTERVAL"; do
    [[ "$n" =~ ^[0-9]+$ ]] || {
        warn "--timeout/--interval must be non-negative integers"
        exit 1
    }
done
[[ "$INTERVAL" -ge 1 ]] || INTERVAL=1

# ─── Resolve the lock key ───────────────────────────────────────────
# A short, stable, filesystem-safe token. "global" yields one host-wide key;
# "target" derives the key from the resolved CARGO_TARGET_DIR so distinct
# target dirs lock independently.

hash_string() {
    # Stable short hash, dependency-free across macOS/Linux.
    if command -v cksum >/dev/null 2>&1; then
        printf '%s' "$1" | cksum | awk '{print $1}'
    else
        # Fallback: sanitize the raw string.
        printf '%s' "$1" | tr -c 'A-Za-z0-9' '_'
    fi
}

resolve_target_dir() {
    if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
        printf '%s' "$CARGO_TARGET_DIR"
        return
    fi
    local root
    root="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
    printf '%s/target' "$root"
}

sanitize() {
    printf '%s' "$1" | tr -c 'A-Za-z0-9._-' '_'
}

if [[ -n "$LOCK_NAME" ]]; then
    # An explicit lock name is used verbatim (sanitized to be path-safe) so it
    # is predictable for scripting and tests.
    KEY="$(sanitize "$LOCK_NAME")"
elif [[ "$SCOPE" == "target" ]]; then
    KEY="target-$(hash_string "$(resolve_target_dir)")"
else
    KEY="global"
fi

LOCK_ROOT="${TMPDIR:-/tmp}/tsz-nextest-guard"
LOCK_DIR="$LOCK_ROOT/$KEY.lock"
STEAL_DIR="$LOCK_ROOT/$KEY.steal"
mkdir -p "$LOCK_ROOT" 2>/dev/null || true

# A token unique to this run, so release only removes a lock we still own.
OWNER_TOKEN="$$@$(hostname 2>/dev/null || echo host)-${EPOCHSECONDS:-$(date +%s)}-$RANDOM"

OWN_LOCK=0

# ─── Process-tree kill (bottom-up), mirrors safe-run.sh ─────────────

kill_tree() {
    local pid=$1
    local sig=${2:-TERM}
    local child
    for child in $(pgrep -P "$pid" 2>/dev/null); do
        kill_tree "$child" "$sig"
    done
    kill -"$sig" "$pid" 2>/dev/null || true
}

pid_alive() {
    local pid=$1
    [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null
}

# Reap the orchestrator process tree a previous guarded run recorded. Only the
# recorded child PID is touched, and only if the recorded owner PID is dead, so
# this can never kill a live unrelated nextest. A PID-reuse guard checks that
# the recorded command name still looks like a cargo/nextest process.
reap_recorded_child() {
    local dir=$1
    local child cmd
    child="$(cat "$dir/child" 2>/dev/null || true)"
    pid_alive "$child" || return 0
    cmd="$(ps -o comm= -p "$child" 2>/dev/null || true)"
    # PID-reuse guard: the recorded PID is only killed if its current command
    # still looks like the command this guard launches — `cargo`/`cargo-nextest`
    # or a `safe-run.sh` wrapper around them. If the PID was recycled into an
    # unrelated process, the name will not match and we leave it untouched.
    case "$cmd" in
        *cargo* | *nextest* | *safe-run*)
            warn "reaping wedged orchestrator from a previous aborted run (pid $child: ${cmd:-?})"
            kill_tree "$child" TERM
            sleep 1
            kill_tree "$child" KILL
            ;;
        *)
            log "recorded child pid $child is now '$cmd', not a nextest tree; leaving it alone"
            ;;
    esac
}

# Grace period before an empty (half-written) lock dir is treated as abandoned.
# Comfortably longer than the gap between `mkdir`-ing the lock dir and writing
# its pid file (two adjacent statements), so a lock that is merely mid-init is
# never reclaimed out from under its owner.
GRACE_SECONDS=10

# Under the steal mutex, drop the lock dir if it is still reclaimable — its
# recorded holder is dead, or it never recorded one (abandoned half-init). Reaps
# the orchestrator the prior run recorded before removing the dir. Returns 1 if
# another waiter holds the steal mutex (it will do the cleanup instead).
reclaim_lock_dir() {
    local reason=$1 holder
    mkdir "$STEAL_DIR" 2>/dev/null || return 1
    # Re-read under the mutex: the holder may have finished and released between
    # the caller's check and here.
    holder="$(cat "$LOCK_DIR/pid" 2>/dev/null || true)"
    if [[ -z "$holder" ]] || ! pid_alive "$holder"; then
        log "reclaiming lock '$KEY' ($reason; holder ${holder:-none})"
        reap_recorded_child "$LOCK_DIR"
        rm -rf "$LOCK_DIR"
    fi
    rmdir "$STEAL_DIR" 2>/dev/null || true
    return 0
}

acquire_lock() {
    local start now holder grace_logged=0 grace_start=""
    start="${EPOCHSECONDS:-$(date +%s)}"

    while true; do
        if mkdir "$LOCK_DIR" 2>/dev/null; then
            printf '%s\n' "$$" >"$LOCK_DIR/pid"
            printf '%s\n' "$OWNER_TOKEN" >"$LOCK_DIR/token"
            OWN_LOCK=1
            log "acquired lock '$KEY'"
            return 0
        fi

        # Lock is held. Classify the holder, reclaiming it if it is dead or
        # abandoned, otherwise waiting (or failing fast under --no-wait).
        now="${EPOCHSECONDS:-$(date +%s)}"
        holder="$(cat "$LOCK_DIR/pid" 2>/dev/null || true)"
        if [[ -z "$holder" ]]; then
            # Half-initialized lock: reclaim only after the grace window so we
            # never race an owner that is mid-init.
            if [[ -z "$grace_start" ]]; then
                grace_start="$now"
            elif [[ $((now - grace_start)) -ge "$GRACE_SECONDS" ]]; then
                reclaim_lock_dir "no pid after ${GRACE_SECONDS}s grace" && grace_start=""
            fi
        elif ! pid_alive "$holder"; then
            grace_start=""
            reclaim_lock_dir "dead holder pid $holder"
        else
            grace_start=""
            if [[ "$NO_WAIT" -eq 1 ]]; then
                warn "lock '$KEY' held by live run (pid $holder); --no-wait set"
                return 75
            fi
            if [[ "$grace_logged" -eq 0 ]]; then
                warn "waiting for in-flight nextest (lock '$KEY', held by pid $holder)…"
                grace_logged=1
            fi
        fi

        if [[ "$TIMEOUT" -gt 0 && $((now - start)) -ge "$TIMEOUT" ]]; then
            warn "timed out after ${TIMEOUT}s waiting for lock '$KEY'"
            return 75
        fi
        sleep "$INTERVAL"
    done
}

release_lock() {
    [[ "$OWN_LOCK" -eq 1 ]] || return 0
    # Only remove the lock if we still own it (a stealer may have taken over
    # after declaring us stale).
    local tok
    tok="$(cat "$LOCK_DIR/token" 2>/dev/null || true)"
    if [[ "$tok" == "$OWNER_TOKEN" ]]; then
        rm -rf "$LOCK_DIR"
        log "released lock '$KEY'"
    fi
    OWN_LOCK=0
}

# ─── Run the wrapped command under the lock ─────────────────────────

CMD_PID=""

cleanup() {
    if [[ -n "$CMD_PID" ]] && kill -0 "$CMD_PID" 2>/dev/null; then
        kill_tree "$CMD_PID" TERM
        sleep 1
        kill_tree "$CMD_PID" KILL
    fi
    CMD_PID=""
    release_lock
}
trap cleanup EXIT

forward_signal() {
    [[ -n "$CMD_PID" ]] && kill -0 "$CMD_PID" 2>/dev/null && kill_tree "$CMD_PID" TERM
    # Restore default disposition and re-raise so the exit status reflects the
    # signal; the EXIT trap still runs release_lock.
    trap - INT TERM
    kill -"${1:-TERM}" "$$" 2>/dev/null || true
}
trap 'forward_signal INT' INT
trap 'forward_signal TERM' TERM

acquire_lock
rc=$?
if [[ "$rc" -ne 0 ]]; then
    exit "$rc"
fi

"$@" &
CMD_PID=$!
# Record the orchestrator PID so a future run can reap it if we are killed
# mid-build before our EXIT trap can clean up.
printf '%s\n' "$CMD_PID" >"$LOCK_DIR/child" 2>/dev/null || true

wait "$CMD_PID"
EXIT_CODE=$?
CMD_PID=""

release_lock
trap - EXIT
exit "$EXIT_CODE"
