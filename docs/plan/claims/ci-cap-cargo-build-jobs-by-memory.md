# ci: cap CARGO_BUILD_JOBS by host memory to prevent rustc SIGKILL OOM

- **Date**: 2026-04-26
- **Branch**: `ci/cap-cargo-build-jobs-by-memory`
- **PR**: TBD
- **Status**: claim

## Intent

The unit lane has been failing across many in-flight PRs (#1377, #1378, #1379,
#1382, etc.) with `(signal: 9, SIGKILL: kill)` during `tsz-checker` rustc
compile. Root cause: with the new 32 vCPU / 128 GiB cloud-runner (#1354),
`CARGO_BUILD_JOBS=$HOST_CPUS=32` runs 32 parallel rustc instances at ~5 GiB
each (≈160 GiB), exceeding the 128 GiB ceiling. The kernel OOM-killer SIGKILLs
rustc.

PR #1343 added `ci-unit` profile with `codegen-units=16` to reduce per-link
memory, but that wasn't enough — the bottleneck is the parallel job count, not
codegen units per binary.

This PR adds a `default_cargo_build_jobs()` helper that takes
`min(HOST_CPUS, host_memory_mb / TSZ_CI_CARGO_MB_PER_JOB)` with a default
12288 MiB per job (so 32 vCPU × 128 GiB → 10 jobs, leaving ~8 GiB headroom
for cargo metadata + the OS; on an 8 vCPU × 32 GiB host → 2 jobs).

## Files Touched

- `scripts/ci/gcp-full-ci.sh` (~20 LOC: new helper + applied to env export)

## Verification

- Bash syntax: `bash -n scripts/ci/gcp-full-ci.sh`
- Once merged: rebase a blocked PR (e.g. #1379) and verify unit lane goes green.
