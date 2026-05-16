#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

packages=(${TSZ_COVERAGE_PACKAGES:-tsz-common tsz-scanner tsz-parser})
output_path="${TSZ_COVERAGE_OUTPUT:-target/llvm-cov/quality-tools.lcov}"

args=()
for package in "${packages[@]}"; do
  args+=("-p" "$package")
done

mkdir -p "$(dirname "$output_path")"
scripts/safe-run.sh --limit "${TSZ_COVERAGE_MEMORY_LIMIT:-75%}" -- \
  cargo llvm-cov "${args[@]}" --lib --lcov --output-path "$output_path"

echo "Coverage report written to ${output_path}"
