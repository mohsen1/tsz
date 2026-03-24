#!/usr/bin/env bash
# safe-run.sh — Memory-guarded command runner
#
# Monitors the total RSS of a command's process tree and kills it
# if memory usage exceeds a configurable limit. Designed to prevent
# runaway builds/tests from bricking the system via OOM.
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
#   scripts/safe-run.sh cargo test
#   scripts/safe-run.sh --limit 8192 -- cargo build
#   scripts/safe-run.sh --limit 50% -- ./scripts/conformance/conformance.sh run
#   scripts/safe-run.sh --verbose -- cargo test --release

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
WARN_PRINTED=0

# ─── Parse options ───────────────────────────────────────────────────

while [[ $# -gt 0 ]]; do
    case "$1" in
        --limit)
            if [[ "$2" == *% ]]; then
                PCT=${2%\%}
                LIMIT_MB=$((TOTAL_RAM_MB * PCT / 100))
            else
                LIMIT_MB="$2"
            fi
            shift 2
            ;;
        --interval)
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

# ─── Kill process tree (bottom-up) ──────────────────────────────────

kill_tree() {
    local pid=$1
    local sig=${2:-TERM}
    local children
    children=$(pgrep -P "$pid" 2>/dev/null) || true
    for child in $children; do
        kill_tree "$child" "$sig"
    done
    kill -"$sig" "$pid" 2>/dev/null || true
}

# ─── Cleanup on exit ────────────────────────────────────────────────

MONITOR_PID=""
CMD_PID=""

cleanup() {
    if [[ -n "$MONITOR_PID" ]]; then
        kill "$MONITOR_PID" 2>/dev/null || true
        wait "$MONITOR_PID" 2>/dev/null || true
        MONITOR_PID=""
    fi
    if [[ -n "$CMD_PID" ]] && kill -0 "$CMD_PID" 2>/dev/null; then
        kill_tree "$CMD_PID" TERM
        sleep 1
        kill_tree "$CMD_PID" KILL
        CMD_PID=""
    fi
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

echo "[safe-run] PID $CMD_PID | limit ${LIMIT_MB}MB | interval ${INTERVAL}s | system RAM ${TOTAL_RAM_MB}MB" >&2

# ─── Monitor loop (background) ──────────────────────────────────────

(
    warn_printed=0
    while kill -0 "$CMD_PID" 2>/dev/null; do
        sleep "$INTERVAL"

        # Guard: process may have exited during sleep
        kill -0 "$CMD_PID" 2>/dev/null || break

        RSS_KB=$(get_tree_rss_kb "$CMD_PID")
        RSS_MB=$((RSS_KB / 1024))

        if [[ "$VERBOSE" -eq 1 ]]; then
            echo "[safe-run] RSS: ${RSS_MB}MB / ${LIMIT_MB}MB" >&2
        fi

        if [[ "$RSS_MB" -gt "$LIMIT_MB" ]]; then
            echo "" >&2
            echo "[safe-run] *** MEMORY LIMIT EXCEEDED ***" >&2
            echo "[safe-run] Process tree using ${RSS_MB}MB (limit: ${LIMIT_MB}MB)" >&2
            echo "[safe-run] Killing process tree (PID $CMD_PID)..." >&2
            kill_tree "$CMD_PID" TERM
            sleep 2
            kill_tree "$CMD_PID" KILL
            exit 1
        elif [[ "$RSS_MB" -gt "$WARN_MB" ]] && [[ "$warn_printed" -eq 0 ]]; then
            echo "[safe-run] WARNING: ${RSS_MB}MB used (80% of ${LIMIT_MB}MB limit)" >&2
            warn_printed=1
        fi
    done
) &
MONITOR_PID=$!

# ─── Wait for command ───────────────────────────────────────────────

wait "$CMD_PID" 2>/dev/null
EXIT_CODE=$?
CMD_PID=""

# ─── Stop monitor ───────────────────────────────────────────────────

kill "$MONITOR_PID" 2>/dev/null || true
wait "$MONITOR_PID" 2>/dev/null || true
MONITOR_PID=""

exit "$EXIT_CODE"
