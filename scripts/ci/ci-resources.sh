#!/usr/bin/env bash
# Resource budget helpers for CI suites.
#
# HOST_CPUS and SHARD_COUNT default to sensible values if not already set;
# callers may override them before sourcing.  full-ci.sh sets HOST_CPUS
# before sourcing this file, so the :=... assignment below is a no-op in
# that path.

: "${HOST_CPUS:=$(getconf _NPROCESSORS_ONLN 2>/dev/null || nproc 2>/dev/null || echo 8)}"
: "${SHARD_COUNT:=${TSZ_CI_SHARDS:-4}}"

host_memory_mb() {
  if [[ -r /proc/meminfo ]]; then
    awk '/MemTotal:/ { printf "%d\n", $2 / 1024 }' /proc/meminfo
  elif command -v sysctl >/dev/null 2>&1; then
    local bytes
    bytes="$(sysctl -n hw.memsize 2>/dev/null || echo 0)"
    if [[ "$bytes" =~ ^[0-9]+$ && "$bytes" -gt 0 ]]; then
      printf '%s\n' $((bytes / 1024 / 1024))
    else
      printf '0\n'
    fi
  else
    printf '0\n'
  fi
}

cap_workers() {
  local requested="$1"
  if (( requested < HOST_CPUS )); then
    printf '%s\n' "$requested"
  else
    printf '%s\n' "$HOST_CPUS"
  fi
}

# Cap CARGO_BUILD_JOBS by the available-memory budget before safe-run applies
# its process-level backstop. Suite-specific defaults are conservative starting
# points for the replacement workspace and remain overrideable for measured CI
# tuning.
default_cargo_build_jobs() {
  local cpu_jobs mem_mb mem_per_job_mb mem_jobs
  cpu_jobs="$HOST_CPUS"
  mem_mb="$(host_memory_mb)"
  case "${TSZ_CI_SUITE:-${_TSZ_CI_SUITE:-}}" in
    unit)
      # Keep the first clean-slate unit lane serialized until its hosted-runner
      # peak has enough history to justify parallel compile jobs.
      mem_per_job_mb="${TSZ_CI_UNIT_CARGO_MB_PER_JOB:-24576}"
      ;;
    dist-binaries)
      # dist-fast builds use a separate budget because they do not compile test
      # targets and safe-run remains the final memory-pressure backstop.
      mem_per_job_mb="${TSZ_CI_DIST_CARGO_MB_PER_JOB:-3584}"
      ;;
    *)
      mem_per_job_mb="${TSZ_CI_CARGO_MB_PER_JOB:-7168}"
      ;;
  esac
  if [[ "$mem_mb" =~ ^[0-9]+$ && "$mem_mb" -gt 0 && "$mem_per_job_mb" =~ ^[0-9]+$ && "$mem_per_job_mb" -gt 0 ]]; then
    mem_jobs=$((mem_mb / mem_per_job_mb))
    if (( mem_jobs < 1 )); then mem_jobs=1; fi
    if (( cpu_jobs > mem_jobs )); then
      printf '%s\n' "$mem_jobs"
      return
    fi
  fi
  printf '%s\n' "$cpu_jobs"
}

default_shard_workers() {
  local usable per
  usable=$((HOST_CPUS - 8))
  if (( usable < SHARD_COUNT )); then
    usable="$HOST_CPUS"
  fi
  per=$((usable / SHARD_COUNT))
  if (( per < 20 )); then
    per=20
  elif (( per > 64 )); then
    per=64
  fi
  cap_workers "$per"
}

default_emit_workers() {
  local workers
  workers="$(default_shard_workers)"
  if (( workers > 32 )); then
    workers=32
  fi
  cap_workers "$workers"
}

default_fourslash_workers() {
  local usable per mem_mb mem_per_worker_mb mem_cap shard_count
  # Use all CPUs split evenly across concurrent shards; no large OS reservation needed.
  usable="$HOST_CPUS"
  per=$((usable / SHARD_COUNT))
  if (( per < 1 )); then per=1; fi

  mem_mb="$(host_memory_mb)"
  mem_per_worker_mb="${TSZ_CI_FOURSLASH_MB_PER_WORKER:-1024}"
  shard_count="${SHARD_COUNT:-1}"
  if [[ "$mem_mb" =~ ^[0-9]+$ && "$mem_mb" -gt 0 && "$mem_per_worker_mb" =~ ^[0-9]+$ && "$mem_per_worker_mb" -gt 0 && "$shard_count" -gt 0 ]]; then
    # All shards run concurrently, so divide total budget by shard count for per-shard cap.
    mem_cap=$(( mem_mb / (mem_per_worker_mb * shard_count) ))
    if (( mem_cap < 2 )); then
      mem_cap=2
    fi
    if (( per > mem_cap )); then
      per="$mem_cap"
    fi
  fi

  if (( per < 2 )); then
    per=2
  elif (( per > 32 )); then
    per=32
  fi
  cap_workers "$per"
}

default_conformance_workers() {
  local workers mem_mb mem_per_worker_mb mem_cap
  workers=$((HOST_CPUS - 8))
  if (( workers < 1 )); then
    workers="$HOST_CPUS"
  fi

  mem_mb="$(host_memory_mb)"
  mem_per_worker_mb="${TSZ_CI_CONFORMANCE_MB_PER_WORKER:-2048}"
  if [[ "$mem_mb" =~ ^[0-9]+$ && "$mem_mb" -gt 0 && "$mem_per_worker_mb" =~ ^[0-9]+$ && "$mem_per_worker_mb" -gt 0 ]]; then
    mem_cap=$((mem_mb / mem_per_worker_mb))
    if (( mem_cap < 8 )); then
      mem_cap=8
    fi
    if (( workers > mem_cap )); then
      workers="$mem_cap"
    fi
  fi

  if (( workers > 128 )); then
    workers=128
  fi
  cap_workers "$workers"
}

# Returns free-for-allocation memory in MB from /proc/meminfo (Linux) or
# vm_stat (macOS). Returns 0 if the information is unavailable.
ci_available_memory_mb() {
  if [[ -r /proc/meminfo ]]; then
    awk '/MemAvailable:/ { printf "%d\n", $2 / 1024 }' /proc/meminfo
  elif command -v sysctl >/dev/null 2>&1; then
    local pages pagesize
    pages="$(sysctl -n vm.page_free_count 2>/dev/null || echo 0)"
    pagesize="$(sysctl -n hw.pagesize 2>/dev/null || echo 4096)"
    if [[ "$pages" =~ ^[0-9]+$ && "$pagesize" =~ ^[0-9]+$ && "$pages" -gt 0 ]]; then
      printf '%d\n' $(( pages * pagesize / 1024 / 1024 ))
    else
      printf '0\n'
    fi
  else
    printf '0\n'
  fi
}

# Preflight memory gate for the best-effort CI cache save.
#
# The cache-save path tars a multi-GB target dir at post-build memory peak
# (cache.sh save). On a memory-starved runner the kernel OOM-killer can
# SIGKILL the build container mid-tar, failing an otherwise-green job and
# wedging the merge queue (#13733). No after-the-fact `|| echo warning` can
# catch a SIGKILL, so the only real lever is to refuse the tar before it runs.
#
# This mirrors bench-shard-prelude.sh's TSZ_BENCH_MIN_FREE_MB gate. The floor
# is TSZ_CI_CACHE_SAVE_MIN_FREE_MB (default 2048 MiB). Setting it to 0 disables
# the gate (always attempt the save).
#
# Returns 0 when there is enough headroom OR when MemAvailable is unknown
# (fail open — never block a save on a host without /proc/meminfo). Returns 1
# only when MemAvailable is a known positive value below the floor.
ci_cache_save_memory_ok() {
  local floor="${TSZ_CI_CACHE_SAVE_MIN_FREE_MB:-2048}"
  local avail
  avail="$(ci_available_memory_mb)"
  if [[ "$floor" =~ ^[0-9]+$ && "$floor" -gt 0 \
        && "$avail" =~ ^[0-9]+$ && "$avail" -gt 0 \
        && "$avail" -lt "$floor" ]]; then
    return 1
  fi
  return 0
}

# Prints a one-line memory status summary for CI diagnostic logs.
# Optional argument is a label tag prepended to the line.
ci_report_memory() {
  local prefix="${1:+[${1}] }"
  if [[ -r /proc/meminfo ]]; then
    local mem_total mem_available swap_total swap_free
    read -r mem_total mem_available swap_total swap_free < <(
      awk '/MemTotal:/{t=$2} /MemAvailable:/{a=$2} /SwapTotal:/{st=$2} /SwapFree:/{sf=$2}
           END{printf "%d %d %d %d\n", t/1024, a/1024, st/1024, sf/1024}' /proc/meminfo
    )
    echo "${prefix}mem: total=${mem_total}MB available=${mem_available}MB swap_used=$(( swap_total - swap_free ))MB"
  elif command -v vm_stat >/dev/null 2>&1; then
    local pages_free pagesize avail_mb
    pages_free="$(vm_stat | awk '/Pages free:/ { gsub("\\.",""); print $3 }')"
    pagesize="$(sysctl -n hw.pagesize 2>/dev/null || echo 4096)"
    if [[ "$pages_free" =~ ^[0-9]+$ && "$pagesize" =~ ^[0-9]+$ ]]; then
      avail_mb=$(( pages_free * pagesize / 1024 / 1024 ))
      echo "${prefix}mem: available≈${avail_mb}MB"
    fi
  fi
}
