#!/usr/bin/env bash
# safe-run.sh — Memory-guarded command runner
#
# Monitors a command's process tree and kills it if memory usage exceeds
# a configurable limit. On macOS, uses physical footprint when available;
# elsewhere falls back to RSS. Designed to prevent runaway builds/tests
# from bricking the system via OOM.
#
# Usage:
#   scripts/safe-run.sh [OPTIONS] [--] COMMAND [ARGS...]
#
# Options:
#   --limit MB|%   Memory limit in MB or % of system RAM (default: 75%)
#   --interval S   Check interval in seconds (default: 5)
#   --verbose      Print memory usage on each check
#
# Examples:
#   scripts/safe-run.sh cargo nextest run
#   scripts/safe-run.sh --limit 8192 -- cargo build
#   scripts/safe-run.sh --limit 50% -- ./scripts/conformance/conformance.sh run --filter mappedTypeRelationships
#   scripts/safe-run.sh --verbose -- cargo nextest run --cargo-profile release

set -uo pipefail

# ─── Detect system RAM ──────────────────────────────────────────────

detect_system_ram_mb() {
    if [[ -f /proc/meminfo ]]; then
        awk '/MemTotal/ {printf "%d", $2/1024}' /proc/meminfo
    elif command -v sysctl &>/dev/null && sysctl -n hw.memsize &>/dev/null; then
        sysctl -n hw.memsize 2>/dev/null | awk '{printf "%d", $1/1048576}'
    else
        echo 16384 # fallback: assume 16GB
    fi
}

TOTAL_RAM_MB=$(detect_system_ram_mb)

# ─── Defaults ────────────────────────────────────────────────────────

LIMIT_MB=$((TOTAL_RAM_MB * 75 / 100))
INTERVAL=5
VERBOSE=0

# ─── Parse options ───────────────────────────────────────────────────

while [[ $# -gt 0 ]]; do
    case "$1" in
        --limit)
            if ! [[ "${2:-}" =~ ^[0-9]+%?$ ]]; then
                echo "safe-run: --limit must be a number of MB or a percentage (got '${2:-}')" >&2
                exit 1
            fi
            if [[ "$2" == *% ]]; then
                PCT=${2%\%}
                LIMIT_MB=$((TOTAL_RAM_MB * PCT / 100))
            else
                LIMIT_MB="$2"
            fi
            shift 2
            ;;
        --interval)
            if ! [[ "${2:-}" =~ ^[0-9]+([.][0-9]+)?$ ]]; then
                echo "safe-run: --interval must be a number of seconds (got '${2:-}')" >&2
                exit 1
            fi
            INTERVAL="$2"
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
        *)
            break
            ;;
    esac
done

if [[ $# -eq 0 ]]; then
    echo "Usage: safe-run.sh [--limit MB|%] [--interval S] [--verbose] [--] COMMAND [ARGS...]" >&2
    exit 1
fi

WARN_MB=$((LIMIT_MB * 80 / 100))

if [[ "$(uname -s 2>/dev/null)" == "Darwin" ]] && command -v footprint &>/dev/null; then
    MEMORY_MODE="physical footprint"
else
    MEMORY_MODE="RSS"
fi

# ─── Process tree RSS (KB) ──────────────────────────────────────────
# Walks the full descendant tree from a root PID using a single ps
# snapshot. Multi-pass awk ensures children appearing before parents
# in ps output are still counted.

get_tree_rss_kb() {
    local root_pid=$1
    ps -eo pid=,ppid=,rss= 2>/dev/null | awk -v root="$root_pid" '
    {
        pid[NR] = $1; ppid[NR] = $2; rss[NR] = $3; n = NR
    }
    END {
        tree[root] = 1
        changed = 1
        while (changed) {
            changed = 0
            for (i = 1; i <= n; i++) {
                if (!tree[pid[i]] && tree[ppid[i]]) {
                    tree[pid[i]] = 1
                    changed = 1
                }
            }
        }
        total = 0
        for (i = 1; i <= n; i++) {
            if (tree[pid[i]]) total += rss[i]
        }
        print total
    }'
}

get_tree_pids() {
    local root_pid=$1
    ps -eo pid=,ppid= 2>/dev/null | awk -v root="$root_pid" '
    {
        pid[NR] = $1; ppid[NR] = $2; n = NR
    }
    END {
        tree[root] = 1
        changed = 1
        while (changed) {
            changed = 0
            for (i = 1; i <= n; i++) {
                if (!tree[pid[i]] && tree[ppid[i]]) {
                    tree[pid[i]] = 1
                    changed = 1
                }
            }
        }
        for (i = 1; i <= n; i++) {
            if (tree[pid[i]]) print pid[i]
        }
    }'
}

get_tree_footprint_kb() {
    local root_pid=$1
    local footprint_args=()
    local pid

    while IFS= read -r pid; do
        [[ -n "$pid" ]] || continue
        footprint_args+=("-p" "$pid")
    done < <(get_tree_pids "$root_pid")

    [[ "${#footprint_args[@]}" -gt 0 ]] || return 1

    local bytes
    bytes=$(footprint -f bytes --noCategories "${footprint_args[@]}" 2>/dev/null | awk '
    /Summary Footprint:/ { summary = $(NF - 1) }
    /^[[:space:]]*phys_footprint:/ {
        phys += $(NF - 1)
        phys_found = 1
    }
    !/Summary/ && /Footprint:/ {
        for (i = 1; i <= NF; i++) {
            if ($i == "Footprint:") {
                header += $(i + 1)
                header_found = 1
            }
        }
    }
    END {
        if (summary != "") print summary
        else if (phys_found) print phys
        else if (header_found) print header
        else print 0
    }') || return 1

    [[ "$bytes" =~ ^[0-9]+$ ]] || return 1
    printf "%d\n" $(((bytes + 1023) / 1024))
}

get_tree_memory_kb() {
    local root_pid=$1

    if [[ "$MEMORY_MODE" == "physical footprint" ]]; then
        get_tree_footprint_kb "$root_pid" && return 0
    fi

    get_tree_rss_kb "$root_pid"
}

# ─── Bounded memory probe ────────────────────────────────────────────
# Runs get_tree_memory_kb in the background with a deadline so a slow
# or blocked `footprint`/`ps` invocation cannot wedge the monitor loop
# (and with it, memory enforcement). On timeout the probe's process
# tree is killed and the sample is reported as failed.
# SAFE_RUN_PROBE_TIMEOUT_SECS overrides the default 10s deadline.

PROBE_TIMEOUT_SECS=${SAFE_RUN_PROBE_TIMEOUT_SECS:-10}
if ! [[ "$PROBE_TIMEOUT_SECS" =~ ^[0-9]+$ ]]; then
    echo "safe-run: SAFE_RUN_PROBE_TIMEOUT_SECS must be a whole number of seconds (got '$PROBE_TIMEOUT_SECS')" >&2
    exit 1
fi
INTERVAL_WHOLE_SECS=${INTERVAL%%.*}
if [[ "$INTERVAL_WHOLE_SECS" -gt "$PROBE_TIMEOUT_SECS" ]]; then
    PROBE_TIMEOUT_SECS=$INTERVAL_WHOLE_SECS
fi

sample_tree_memory_kb() {
    local root_pid=$1
    local mode=${2:-$MEMORY_MODE}
    local probe_pid ticks kb

    MEMORY_MODE="$mode" get_tree_memory_kb "$root_pid" >"$PROBE_OUT" 2>/dev/null &
    probe_pid=$!

    ticks=$((PROBE_TIMEOUT_SECS * 5))
    while kill -0 "$probe_pid" 2>/dev/null; do
        if [[ "$ticks" -le 0 ]]; then
            kill_tree "$probe_pid" KILL
            wait "$probe_pid" 2>/dev/null
            return 1
        fi
        sleep 0.2 2>/dev/null || sleep 1
        ticks=$((ticks - 1))
    done
    wait "$probe_pid" 2>/dev/null || return 1

    # The result stays in $PROBE_OUT; callers read it with the
    # fork-free $(<...) instead of capturing this function's stdout.
    kb=$(<"$PROBE_OUT")
    [[ "$kb" =~ ^[0-9]+$ ]]
}

# ─── Kill process tree (freeze, then bottom-up) ─────────────────────
# The root is stopped before its children are enumerated so it cannot
# spawn new ones between the snapshot and the kill. A stopped process
# does not act on TERM until resumed, so it is continued afterwards
# (harmless after KILL).

kill_tree() {
    local pid=$1
    local sig=${2:-TERM}
    kill -STOP "$pid" 2>/dev/null || true
    local children child
    children=$(pgrep -P "$pid" 2>/dev/null) || true
    for child in $children; do
        kill_tree "$child" "$sig"
    done
    kill -"$sig" "$pid" 2>/dev/null || true
    kill -CONT "$pid" 2>/dev/null || true
}

# ─── Cleanup on exit ────────────────────────────────────────────────

MONITOR_PID=""
CMD_PID=""
PROBE_OUT=$(mktemp "${TMPDIR:-/tmp}/safe-run.XXXXXX") || exit 1

# Tear down the monitor and every probe/sleep child it may have in
# flight. SIGKILL, not SIGTERM: bash defers terminating signals while
# blocked in a foreground child (e.g. a wedged `footprint` probe on
# macOS), so a TERM'd monitor can linger and an unbounded `wait` on it
# hangs the wrapper; its orphaned probe children also inherit our
# stdout/stderr and keep downstream pipe readers (`tee`, `cat`) alive
# after the wrapped command has exited (#15439). The monitor holds no
# state worth a graceful shutdown.
stop_monitor() {
    [[ -n "$MONITOR_PID" ]] || return 0
    kill_tree "$MONITOR_PID" KILL
    wait "$MONITOR_PID" 2>/dev/null || true
    MONITOR_PID=""
}

cleanup() {
    stop_monitor
    if [[ -n "$CMD_PID" ]] && kill -0 "$CMD_PID" 2>/dev/null; then
        kill_tree "$CMD_PID" TERM
        sleep 1
        kill_tree "$CMD_PID" KILL
        CMD_PID=""
    fi
    rm -f "$PROBE_OUT"
}
trap cleanup EXIT

# Forward SIGINT/SIGTERM to child
forward_signal() {
    if [[ -n "$CMD_PID" ]] && kill -0 "$CMD_PID" 2>/dev/null; then
        kill_tree "$CMD_PID" TERM
    fi
}
trap forward_signal INT TERM

# ─── Launch command ──────────────────────────────────────────────────

"$@" &
CMD_PID=$!

echo "[safe-run] PID $CMD_PID | limit ${LIMIT_MB}MB | interval ${INTERVAL}s | mode ${MEMORY_MODE} | system RAM ${TOTAL_RAM_MB}MB" >&2

# ─── Monitor loop (background) ──────────────────────────────────────

(
    warn_printed=0
    probe_failures=0
    while kill -0 "$CMD_PID" 2>/dev/null; do
        sleep "$INTERVAL"

        # Guard: process may have exited during sleep
        kill -0 "$CMD_PID" 2>/dev/null || break

        if sample_tree_memory_kb "$CMD_PID"; then
            probe_failures=0
        else
            # The probe timed out or produced garbage. Retry as a
            # bounded RSS sample on the same tick so memory
            # enforcement never silently stops, and after three
            # consecutive failures stop paying the footprint timeout
            # every tick. When the failed sample already was RSS,
            # skip the tick instead of probing twice.
            probe_failures=$((probe_failures + 1))
            [[ "$MEMORY_MODE" != "RSS" ]] || continue
            if [[ "$probe_failures" -ge 3 ]]; then
                echo "[safe-run] ${MEMORY_MODE} probe failed ${probe_failures}x; switching to RSS" >&2
                MEMORY_MODE="RSS"
            fi
            sample_tree_memory_kb "$CMD_PID" RSS || continue
        fi
        MEMORY_KB=$(<"$PROBE_OUT")
        MEMORY_MB=$((MEMORY_KB / 1024))

        if [[ "$VERBOSE" -eq 1 ]]; then
            echo "[safe-run] ${MEMORY_MODE}: ${MEMORY_MB}MB / ${LIMIT_MB}MB" >&2
        fi

        if [[ "$MEMORY_MB" -gt "$LIMIT_MB" ]]; then
            echo "" >&2
            echo "[safe-run] *** MEMORY LIMIT EXCEEDED ***" >&2
            echo "[safe-run] Process tree using ${MEMORY_MB}MB ${MEMORY_MODE} (limit: ${LIMIT_MB}MB)" >&2
            echo "[safe-run] Killing process tree (PID $CMD_PID)..." >&2
            kill_tree "$CMD_PID" TERM
            sleep 2
            kill_tree "$CMD_PID" KILL
            exit 1
        elif [[ "$MEMORY_MB" -gt "$WARN_MB" ]] && [[ "$warn_printed" -eq 0 ]]; then
            echo "[safe-run] WARNING: ${MEMORY_MB}MB ${MEMORY_MODE} used (80% of ${LIMIT_MB}MB limit)" >&2
            warn_printed=1
        fi
    done
) &
MONITOR_PID=$!

# ─── Wait for command ───────────────────────────────────────────────
# `wait` returns 128+sig when interrupted by a trapped signal (the
# INT/TERM forwarder) before the child exits; re-wait until the child
# is actually gone so its real status is captured and forwarded.

while :; do
    wait "$CMD_PID" 2>/dev/null
    EXIT_CODE=$?
    if [[ "$EXIT_CODE" -gt 128 ]] && kill -0 "$CMD_PID" 2>/dev/null; then
        continue
    fi
    break
done
CMD_PID=""

# The EXIT trap (cleanup -> stop_monitor) tears down the monitor tree.
exit "$EXIT_CODE"
