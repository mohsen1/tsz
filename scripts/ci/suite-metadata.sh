#!/usr/bin/env bash

_TSZ_CI_GITHUB_SUITES=(
  unit
  conformance
  conformance-aggregate
  emit-shard
  emit-aggregate
  fourslash-shard
  fourslash-aggregate
)

_TSZ_CI_FULL_SUITES=(
  dist-binaries
  node-harness-prep
  lint
  unit
  lsp-e2e
  conformance
  conformance-aggregate
  emit-shard
  emit-aggregate
  fourslash-shard
  fourslash-aggregate
)

_TSZ_CI_CACHE_SUITES=(
  "${_TSZ_CI_FULL_SUITES[@]}"
  bench
)

ci_suite_names() {
  case "${1:-full}" in
    full)
      printf '%s\n' "${_TSZ_CI_FULL_SUITES[@]}"
      ;;
    github)
      printf '%s\n' "${_TSZ_CI_GITHUB_SUITES[@]}"
      ;;
    cache)
      printf '%s\n' "${_TSZ_CI_CACHE_SUITES[@]}"
      ;;
    *)
      return 2
      ;;
  esac
}

ci_suite_list() {
  local scope="${1:-full}" sep="${2:-|}" out="" suite
  while IFS= read -r suite; do
    if [[ -n "$out" ]]; then
      out+="$sep"
    fi
    out+="$suite"
  done < <(ci_suite_names "$scope")
  printf '%s\n' "$out"
}

ci_suite_usage() {
  printf '<%s>\n' "$(ci_suite_list "${1:-full}" "|")"
}

ci_suite_is_known() {
  local scope="$1" target="$2" suite
  while IFS= read -r suite; do
    if [[ "$suite" == "$target" ]]; then
      return 0
    fi
  done < <(ci_suite_names "$scope")
  return 1
}

ci_suite_needs_group() {
  local suite="$1" group="$2"
  case "$group" in
    lint)
      [[ "$suite" == "lint" ]]
      ;;
    unit)
      [[ "$suite" == "unit" ]]
      ;;
    node)
      [[ "$suite" == "lint" || "$suite" == conformance* || "$suite" == emit* || "$suite" == fourslash* || "$suite" == "node-harness-prep" ]]
      ;;
    rust_compile)
      ci_suite_needs_rust_compile "$suite"
      ;;
    *)
      return 1
      ;;
  esac
}

ci_suite_needs_rust_compile() {
  case "$1" in
    all|full|bench|lint|unit|dist-binaries)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

# Per-suite list of "cache feature" tags this suite needs restored.
# Restoring features the suite does not use is pure runner-minute cost.
#
# Recognized tags:
#   cargo-home               - Cargo registry/git cache (.ci-cache/cargo-home)
#   typescript-source        - TypeScript source tree (lib + tests/cases)
#   npm                      - global npm cache (.ci-cache/npm)
#   scripts-node-modules     - scripts/node_modules
#   typescript-harness       - TypeScript/built/local
#   typescript-node-modules  - TypeScript/node_modules
#   dist-fast-commit         - commit-keyed dist-fast binary tarball
#
# Per-profile cargo target-dir caches (cargo-target-deps, etc.) are selected
# separately via ci_suite_target_caches() and gated implicitly by cargo-home.
ci_suite_caches() {
  case "${1:-all}" in
    all|full)
      echo "cargo-home typescript-source npm scripts-node-modules typescript-harness typescript-node-modules dist-fast-commit"
      ;;
    lint)
      # Cargo/workspace lint plus lightweight Node script guardrails.
      # It does not read the TypeScript corpus or install npm packages.
      echo "cargo-home"
      ;;
    unit)
      # Rewrite tests may read retained oracle assets at runtime.
      echo "cargo-home typescript-source"
      ;;
    dist-binaries)
      # Rust compile/archive only; downstream suites restore corpus or
      # harness state when they actually need it.
      echo "cargo-home"
      ;;
    bench)
      # Bench builds an optimized tsz binary, reads the TypeScript corpus,
      # and uses npm's cache for pinned tsgo/tsc installs.
      echo "cargo-home typescript-source npm"
      ;;
    lsp-e2e)
      # Reuses the dist-fast `tsz-lsp` binary artifact and a Node protocol
      # driver; no TypeScript corpus or Cargo cache is needed in this job.
      echo ""
      ;;
    conformance)
      # tsz-conformance comes from the dist-fast blob; the corpus comes
      # from TypeScript source. No npm/harness restore needed.
      echo "typescript-source dist-fast-commit"
      ;;
    conformance-aggregate|emit-aggregate|fourslash-aggregate)
      # Aggregates only read per-shard JSONs from GitHub Actions artifacts.
      echo ""
      ;;
    emit-shard)
      # Shards get Node runtime artifacts and tsz via CI artifacts, so only
      # the TypeScript source restore is useful here.
      echo "typescript-source"
      ;;
    fourslash-shard)
      # The runner initializes the TypeScript checkout to read lib and
      # tests/cases inputs before executing the shard.
      echo "typescript-source"
      ;;
    node-harness-prep)
      # Builds TypeScript/built/local and scripts/emit/dist for shards.
      echo "typescript-source npm scripts-node-modules typescript-harness typescript-node-modules"
      ;;
    *)
      echo ""
      ;;
  esac
}

ci_suite_has_cache() {
  local suite="$1" needle="$2"
  local needles=" $(ci_suite_caches "$suite") "
  [[ "$needles" == *" $needle "* ]]
}

ci_suite_target_caches() {
  case "${1:-all}" in
    all|full)
      echo "cargo-target-deps cargo-target-unit"
      ;;
    dist-binaries)
      echo "cargo-target-deps"
      ;;
    unit)
      echo "cargo-target-unit"
      ;;
    lint)
      # The ci-lint target dir was more expensive to transfer than rebuild;
      # sccache handles cross-commit lint reuse.
      echo ""
      ;;
    *)
      echo ""
      ;;
  esac
}
