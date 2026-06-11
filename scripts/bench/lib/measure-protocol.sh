# shellcheck shell=bash
# Shared perf-measurement protocol primitives (issue #13174).
#
# Sourced (never executed) by scripts/bench/run-with-timeout.sh,
# scripts/bench/measure-tsz.sh, scripts/ci/project-compile-guard.sh, and
# scripts/bench/test-measure-protocol.mjs so the measurement protocol has a
# single, unit-tested definition.
#
# The protocol exists because two measurement hazards produced a false perf
# regression report on a shared box:
#   1. Measuring a live shared binary path (e.g. dist-fast/tsz) that a sibling
#      session can overwrite mid-run. Fix: snapshot the binary to an immutable
#      content-addressed path, verify the copy's hash, and measure the copy.
#   2. Trusting wall-clock timeouts under CPU contention. A run that needs 14s
#      of CPU exceeds any wall timeout at a <4% CPU share with zero code
#      regression. Fix: record process CPU time alongside wall time and treat
#      low-CPU-share wall timeouts as unmeasured, not slow.

# Default contention threshold: a timed-out process that consumed less than
# this percentage of one CPU during the wall-timeout window is classified as
# contended (unmeasured) rather than slow. Healthy CPU-bound compiles sit near
# or above 100%; observed contended runs sit below ~22%.
TSZ_MEASURE_DEFAULT_MIN_CPU_SHARE_PCT=25

# Stable sha256 of a file (Linux sha256sum / macOS shasum). Mirrors
# sha256_of_file in scripts/ci/lib/project-compile-fingerprint.sh, which must
# stay self-contained for its own unit test.
tsz_sha256_of_file() {
  sha256sum "$1" 2>/dev/null | awk '{print $1}' \
    || shasum -a 256 "$1" 2>/dev/null | awk '{print $1}' || true
}

# Copy a binary to an immutable content-addressed path and verify the copy.
#
#   tsz_snapshot_binary <source_binary> <snapshot_dir>
#
# Prints "<snapshot_path> <sha256>" on success. The snapshot name embeds the
# content hash, so concurrent sessions sharing <snapshot_dir> converge on the
# same file and a later rebuild of the source can never mutate an existing
# snapshot. The copy is hashed after the copy and compared against the hash
# read before the copy; a mismatch means the source was overwritten mid-copy
# (the live-binary hazard this exists to catch), so the copy is retried.
tsz_snapshot_binary() {
  local src="$1" dest_dir="$2"
  local attempt h_src h_copy dest tmp

  if [ ! -f "$src" ]; then
    echo "tsz_snapshot_binary: source binary not found: $src" >&2
    return 1
  fi
  mkdir -p "$dest_dir" || return 1

  for attempt in 1 2 3; do
    h_src="$(tsz_sha256_of_file "$src")"
    if [ -z "$h_src" ]; then
      echo "tsz_snapshot_binary: no sha256 tool available (need sha256sum or shasum)" >&2
      return 1
    fi
    dest="$dest_dir/$(basename "$src").${h_src:0:16}"

    if [ -f "$dest" ] && [ "$(tsz_sha256_of_file "$dest")" = "$h_src" ]; then
      printf '%s %s\n' "$dest" "$h_src"
      return 0
    fi

    tmp="${dest}.tmp.$$"
    cp "$src" "$tmp" 2>/dev/null || {
      rm -f "$tmp"
      echo "tsz_snapshot_binary: copy failed: $src -> $tmp" >&2
      return 1
    }
    h_copy="$(tsz_sha256_of_file "$tmp")"
    if [ "$h_copy" = "$h_src" ]; then
      chmod +x "$tmp"
      mv -f "$tmp" "$dest"
      printf '%s %s\n' "$dest" "$h_src"
      return 0
    fi

    # Source mutated between the hash read and the copy: retry against the
    # new content rather than blessing a torn snapshot.
    rm -f "$tmp"
    echo "tsz_snapshot_binary: source changed mid-copy (attempt $attempt), retrying: $src" >&2
  done

  echo "tsz_snapshot_binary: source kept changing during snapshot; refusing to measure a moving binary: $src" >&2
  return 1
}

# Remove stale snapshots in <snapshot_dir> for <source_binary>, keeping only
# <keep_path>. Bounds disk use to one snapshot per binary name.
tsz_prune_binary_snapshots() {
  local src_base dest_dir keep
  src_base="$(basename "$1")"
  dest_dir="$2"
  keep="$3"
  [ -d "$dest_dir" ] || return 0
  local f
  for f in "$dest_dir/$src_base".*; do
    [ -e "$f" ] || continue
    [ "$f" = "$keep" ] || rm -f "$f"
  done
}

# awk function shared by the cputime helpers: parse a ps(1) TIME value into
# seconds. Handles Linux "[dd-]hh:mm:ss" and macOS "mm:ss.cc" / "hh:mm:ss.cc".
_TSZ_CPUTIME_AWK_FN='
function cputime_secs(t,  days, dp, parts, n, i, secs) {
  days = 0
  if (t ~ /-/) {
    split(t, dp, "-")
    days = dp[1]
    t = dp[2]
  }
  n = split(t, parts, ":")
  secs = 0
  for (i = 1; i <= n; i += 1) {
    secs = secs * 60 + parts[i]
  }
  return days * 86400 + secs
}
'

# Parse one ps TIME string to seconds (printed with two decimals).
tsz_cputime_to_seconds() {
  awk -v t="$1" "${_TSZ_CPUTIME_AWK_FN} BEGIN { printf \"%.2f\", cputime_secs(t) }"
}

# Total CPU seconds consumed by <pid> and its descendants, summed over the
# live process tree (same tree walk as the compile guard's RSS sampler).
# Thread CPU time is included in each process's TIME. Prints a number with
# two decimals, or nothing if ps fails.
tsz_process_tree_cpu_seconds() {
  local root_pid="$1"

  ps -e -o pid=,ppid=,time= 2>/dev/null | awk -v root="$root_pid" "${_TSZ_CPUTIME_AWK_FN}"'
    {
      pid[NR] = $1
      ppid[NR] = $2
      cpu[NR] = cputime_secs($3)
      count = NR
    }
    END {
      if (count == 0) exit 1
      live[root] = 1
      changed = 1
      while (changed) {
        changed = 0
        for (i = 1; i <= count; i += 1) {
          if (live[ppid[i]] && !live[pid[i]]) {
            live[pid[i]] = 1
            changed = 1
          }
        }
      }
      total = 0
      for (i = 1; i <= count; i += 1) {
        if (live[pid[i]]) total += cpu[i]
      }
      printf "%.2f", total
    }
  '
}

# Start a wall-timeout watchdog for <pid>: after <timeout_secs> it samples the
# process tree's CPU seconds into <cpu_file> and SIGKILLs <pid>. Prints the
# watchdog pid. <cpu_file> must not exist beforehand; its existence afterwards
# marks that the watchdog fired, and its content is the CPU evidence for
# classifying the timeout. Sampling before the kill is the protocol-critical
# ordering: a dead process tree has no CPU time left to read.
tsz_start_timeout_watchdog() {
  local timeout_secs="$1" pid="$2" cpu_file="$3"
  (
    sleep "$timeout_secs"
    tsz_process_tree_cpu_seconds "$pid" > "$cpu_file" 2>/dev/null || true
    kill -KILL "$pid" 2>/dev/null || true
  ) &
  echo $!
}

# CPU share percentage: 100 * cpu_seconds / wall_seconds, rounded to an
# integer. Prints nothing when either input is empty or wall is not positive.
tsz_cpu_share_pct() {
  local cpu="$1" wall="$2"
  [ -n "$cpu" ] && [ -n "$wall" ] || return 0
  awk -v c="$cpu" -v w="$wall" 'BEGIN {
    if (w + 0 <= 0) exit 0
    printf "%d", (c * 100 / w) + 0.5
  }'
}

# True (exit 0) when a wall timeout is contention-confirmed: CPU evidence
# exists and the CPU share is below the threshold. An unknown CPU sample is
# NOT contended -- callers must not silently discard a timeout without
# evidence.
#
#   tsz_timeout_is_contended <wall_seconds> <cpu_seconds> <min_share_pct>
tsz_timeout_is_contended() {
  local wall="$1" cpu="$2" min_pct="${3:-$TSZ_MEASURE_DEFAULT_MIN_CPU_SHARE_PCT}"
  local share
  share="$(tsz_cpu_share_pct "$cpu" "$wall")"
  [ -n "$share" ] && [ "$share" -lt "$min_pct" ]
}

# True (exit 0) when a wall timeout is confirmed CPU-bound, i.e. genuinely
# slow: CPU evidence exists and the share is at or above the threshold. This
# is NOT the negation of tsz_timeout_is_contended -- a timeout with no CPU
# sample is neither contended nor CPU-bound, just unmeasured. Persisting an
# unmeasured timeout (e.g. into a result cache) requires this to be true.
#
#   tsz_timeout_is_cpu_bound <wall_seconds> <cpu_seconds> <min_share_pct>
tsz_timeout_is_cpu_bound() {
  local wall="$1" cpu="$2" min_pct="${3:-$TSZ_MEASURE_DEFAULT_MIN_CPU_SHARE_PCT}"
  local share
  share="$(tsz_cpu_share_pct "$cpu" "$wall")"
  [ -n "$share" ] && [ "$share" -ge "$min_pct" ]
}

# One-line classification of a wall timeout for logs and diagnostics.
#
#   tsz_timeout_contention_note <wall_seconds> <cpu_seconds> <min_share_pct>
tsz_timeout_contention_note() {
  local wall="$1" cpu="$2" min_pct="${3:-$TSZ_MEASURE_DEFAULT_MIN_CPU_SHARE_PCT}"
  local share
  share="$(tsz_cpu_share_pct "$cpu" "$wall")"
  if [ -z "$share" ]; then
    printf 'wall timeout after %ss; process CPU time unavailable -- treat as unmeasured unless CPU evidence exists\n' "$wall"
  elif [ "$share" -lt "$min_pct" ]; then
    printf 'wall timeout after %ss with only %ss process CPU (~%s%% CPU share, threshold %s%%): likely CPU contention -- treat as unmeasured, not slow\n' \
      "$wall" "$cpu" "$share" "$min_pct"
  else
    printf 'wall timeout after %ss with %ss process CPU (~%s%% CPU share): CPU-bound timeout\n' \
      "$wall" "$cpu" "$share"
  fi
}
