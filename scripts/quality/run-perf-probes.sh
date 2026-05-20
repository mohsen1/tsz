#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

scripts/safe-run.sh --limit "${TSZ_PERF_MEMORY_LIMIT:-75%}" -- \
  cargo build --profile flame -p tsz-cli --bin tsz

scripts/safe-run.sh --limit "${TSZ_PERF_MEMORY_LIMIT:-75%}" -- \
  cargo bloat -p tsz-cli --bin tsz --profile dist-fast --crates -n "${TSZ_BLOAT_ROWS:-20}"
