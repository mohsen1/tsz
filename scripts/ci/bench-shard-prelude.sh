#!/usr/bin/env bash
# bench-shard-prelude.sh — hardening helpers for self-hosted bench shards.
#
# Self-hosted bench shards on the tsz-cloud-run pool run sequentially on the
# same machine. When one shard leaks memory or a heavy benchmark hits the
# runner's OOM ceiling, the runner agent loses communication with GitHub and
# subsequent shards fail with no usable diagnostics (issue #7601).
#
# This helper:
#   * `prelude`    — runs before each shard. Kills orphan tsz/tsgo/hyperfine
#                    processes left over from prior shards, reports memory and
#                    disk headroom, and asserts a minimum free-memory floor so
#                    a doomed shard fails fast instead of bricking the runner.
#   * `postmortem` — runs when a shard fails. Snapshots memory/disk/top RSS
#                    processes and, when readable, recent dmesg/kernel-log OOM
#                    events. Output goes to a single file the workflow uploads
#                    alongside the bench artifact.
#
# Both subcommands are idempotent and safe to invoke outside CI.
#
# Usage:
#   scripts/ci/bench-shard-prelude.sh prelude    [--label LABEL]
#   scripts/ci/bench-shard-prelude.sh postmortem [--label LABEL] [--output PATH]

set -euo pipefail

SUBCOMMAND="${1:-}"
shift || true

LABEL=""
OUTPUT=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --label)
            LABEL="${2:-}"
            shift 2
            ;;
        --output)
            OUTPUT="${2:-}"
            shift 2
            ;;
        *)
            echo "bench-shard-prelude.sh: unknown argument: $1" >&2
            exit 2
            ;;
    esac
done

# Minimum free memory (MiB) required before a shard starts. The largest bench
# shard (`large-ts-repo`) peaks near 1.6 GiB; demanding 2 GiB headroom keeps a
# comfortable margin without rejecting healthy starts on typical runners.
MIN_FREE_MB="${TSZ_BENCH_MIN_FREE_MB:-2048}"

# Process name patterns prior shards may leave behind. These are emitted by
# the bench harness itself, so killing them from a clean prelude is safe.
ORPHAN_PATTERNS=(
    'bench-vs-tsgo'
    'hyperfine'
    'tsgo'
    '\.target-bench/dist/tsz'
)

ORPHANS_KILLED=0

meminfo_mb() {
    local key="$1"
    if [[ -r /proc/meminfo ]]; then
        awk -v k="${key}:" '$1 == k { printf "%d\n", $2 / 1024; exit }' /proc/meminfo
    else
        echo 0
    fi
}

kernel_log_tail() {
    local since="$1"
    dmesg -T 2>/dev/null \
        || journalctl -k --since "$since" --no-pager 2>/dev/null \
        || true
}

report_state() {
    local title="$1"
    echo "==[ ${title}${LABEL:+ — $LABEL} ]=="
    echo "-- date --"
    date -Is || true
    echo "-- uptime --"
    uptime || true
    echo "-- memory (free -h) --"
    free -h 2>/dev/null || true
    echo "-- memory floor: ${MIN_FREE_MB} MiB --"
    echo "MemAvailable_MB=$(meminfo_mb MemAvailable)"
    echo "MemTotal_MB=$(meminfo_mb MemTotal)"
    echo "-- disk (df -h .) --"
    df -h . 2>/dev/null || true
    echo "-- top processes by RSS --"
    ps -eo pid,ppid,user,%cpu,%mem,rss,comm --sort=-rss 2>/dev/null | head -20 || true
}

signal_orphans() {
    local sig="$1"
    local pattern pid pids
    for pattern in "${ORPHAN_PATTERNS[@]}"; do
        pids="$(pgrep -f "$pattern" 2>/dev/null || true)"
        [[ -z "$pids" ]] && continue
        for pid in $pids; do
            [[ "$pid" == "$$" ]] && continue
            if kill -0 "$pid" 2>/dev/null; then
                [[ "$sig" == "TERM" ]] && echo "killing orphan ($pattern): pid=$pid"
                kill -"$sig" "$pid" 2>/dev/null || true
                ORPHANS_KILLED=$((ORPHANS_KILLED + 1))
            fi
        done
    done
}

kill_orphans() {
    ORPHANS_KILLED=0
    signal_orphans TERM
    if (( ORPHANS_KILLED > 0 )); then
        sleep 2
        signal_orphans KILL
    fi
    echo "orphans_terminated=${ORPHANS_KILLED}"
}

usage() {
    cat <<USAGE
Usage: $(basename "$0") {prelude|postmortem} [--label LABEL] [--output PATH]

Subcommands:
  prelude     Cleanup orphan bench processes and verify free-memory floor.
              Exit 75 (EX_TEMPFAIL) when memory floor is not met.
  postmortem  Capture memory/disk/process/OOM diagnostics to --output (or
              ./bench-postmortem[-LABEL].log) for upload as a shard artifact.

Environment:
  TSZ_BENCH_MIN_FREE_MB   Minimum MemAvailable required by prelude (default 2048).
USAGE
}

case "$SUBCOMMAND" in
    prelude)
        report_state "prelude (pre-cleanup)"
        kill_orphans
        if (( ORPHANS_KILLED > 0 )); then
            report_state "prelude (post-cleanup)"
        fi
        free_now="$(meminfo_mb MemAvailable)"
        if [[ "$free_now" =~ ^[0-9]+$ && "$free_now" -gt 0 && "$free_now" -lt "$MIN_FREE_MB" ]]; then
            echo "bench-shard-prelude: refusing to start; MemAvailable=${free_now} MiB < floor ${MIN_FREE_MB} MiB" >&2
            exit 75 # EX_TEMPFAIL
        fi
        ;;
    postmortem)
        if [[ -z "$OUTPUT" ]]; then
            OUTPUT="bench-postmortem${LABEL:+-${LABEL}}.log"
        fi
        kern_log="$(kernel_log_tail '60 min ago')"
        {
            report_state "postmortem"
            echo "-- recent kernel messages (dmesg, may be unavailable) --"
            if [[ -n "$kern_log" ]]; then
                printf '%s\n' "$kern_log" | tail -200
            else
                echo "(dmesg/journalctl unavailable to current user)"
            fi
            echo "-- OOM kill scan --"
            printf '%s\n' "$kern_log" | grep -iE 'oom-?kill|out of memory|invoked oom' \
                || echo "(no OOM events in scanned window)"
        } >"$OUTPUT" 2>&1 || true
        echo "wrote postmortem to $OUTPUT"
        ;;
    -h|--help)
        usage
        ;;
    '')
        usage >&2
        exit 2
        ;;
    *)
        echo "bench-shard-prelude.sh: unknown subcommand: $SUBCOMMAND" >&2
        exit 2
        ;;
esac
