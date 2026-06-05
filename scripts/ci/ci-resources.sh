#!/usr/bin/env bash
# Resource budget helpers for CI suites.
#
# HOST_CPUS and SHARD_COUNT default to sensible values if not already set;
# callers may override them before sourcing.  gcp-full-ci.sh sets HOST_CPUS
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

# Cap CARGO_BUILD_JOBS by memory to prevent rustc/linker SIGKILL during large
# crate compiles. tsz-checker spawns many parallel codegen threads per rustc,
# so the practical per-job RSS at peak (linker time) is bounded by the
# `codegen-units` setting on the active profile. With dist-fast/ci-unit at
# codegen-units=8, peak per-job RSS is ~7 GiB (down from ~12 GiB at cgu=16).
#
# We compute `memory_mb / mb_per_compile_job`, default 7168 MiB/job, then take
# min(cpu, mem). Sizing examples:
#   8 vCPU × 32 GiB  → min(8, 4)   = 4 jobs   (~28 GiB peak)
#   16 vCPU × 64 GiB → min(16, 9)  = 9 jobs   (~63 GiB peak)
#   32 vCPU × 128 GiB → min(32, 18) = 18 jobs (~126 GiB peak)
default_cargo_build_jobs() {
  local cpu_jobs mem_mb mem_per_job_mb mem_jobs
  cpu_jobs="$HOST_CPUS"
  mem_mb="$(host_memory_mb)"
  case "${TSZ_CI_SUITE:-${_TSZ_CI_SUITE:-}}" in
    unit|unit-archive|unit-shard)
      # Force `CARGO_BUILD_JOBS=1` on unit. Observed RSS-per-rustc on this
      # workspace's lib-test compiles (notably tsz-checker, tsz-emitter,
      # tsz-solver, tsz-core lib-test) now exceeds 16 GiB per process during
      # the LLVM codegen phase. With any -j > 1, peaks coincide and SIGKILL
      # fires on the 8 vCPU × 32 GiB Cloud Run runner.
      #
      # History of this knob, in order:
      #   * commit 111d24ba98 — TSZ_CI_CARGO_MB_PER_JOB=7168 globally (4 jobs)
      #   * commit 1bddbbfbf4 — TSZ_CI_UNIT_CARGO_MB_PER_JOB=16384 (2 jobs) +
      #       sccache disablement, after silent-exit incidents.
      #   * PR #7573 (rolled back here) — 8192 (4 jobs). Validated on one
      #       run-of-the-day; sustained PR load on 2026-05-16 surfaced SIGKILL
      #       in tsz-solver/checker/emitter lib-test compile.
      #   * 12288 (2 jobs) intermediate — still SIGKILLs (this PR's first run).
      #   * 24576 (1 job) ← current. Safe on 32 GiB box; floor(32768/24576)=1.
      #
      # The real fix for compile time is a bigger box (Cloud Build private
      # pool e2-highcpu-32 in PR #7591). Once that lands and is promoted, this
      # cap stops mattering — Cloud Build runs the same compile at -j32 on a
      # box where memory isn't the constraint.
      mem_per_job_mb="${TSZ_CI_UNIT_CARGO_MB_PER_JOB:-24576}"
      ;;
    dist-binaries)
      # sccache is disabled for dist-binaries (TSZ_CI_DISABLE_SCCACHE=1 in
      # GitHub CI) so every codegen unit compiles from scratch. The observed
      # peak RSS per cargo job is slightly higher than the sccache-assisted
      # path; budget 8192 MiB/job instead of the default 7168 to keep total
      # cargo RSS below ~87% of RAM before OS overhead.
      mem_per_job_mb="${TSZ_CI_DIST_CARGO_MB_PER_JOB:-8192}"
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
