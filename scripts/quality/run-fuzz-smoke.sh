#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

target="${TSZ_FUZZ_TARGET:-parser}"
toolchain="${TSZ_FUZZ_TOOLCHAIN:-+nightly}"
args=(-max_len="${TSZ_FUZZ_MAX_LEN:-65536}")

if [[ -n "${TSZ_FUZZ_SECONDS:-}" ]]; then
  args+=(-max_total_time="$TSZ_FUZZ_SECONDS")
else
  args+=(-runs="${TSZ_FUZZ_RUNS:-2048}")
fi

scripts/safe-run.sh --limit "${TSZ_FUZZ_MEMORY_LIMIT:-75%}" -- \
  cargo "$toolchain" fuzz run "$target" -- "${args[@]}"
