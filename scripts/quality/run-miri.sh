#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

targets=(${TSZ_MIRI_TARGETS:-tsz-common:interner::tests::test_interner_intern_and_resolve tsz-scanner:scanner_impl::tests::scan_identifiers tsz-parser:parser::node_arena::tests::estimated_size_bytes_is_nonzero_for_empty_arena})
miri_flags="${MIRIFLAGS:--Zmiri-strict-provenance}"

for target in "${targets[@]}"; do
  package="${target%%:*}"
  filter=""
  if [[ "$target" == *:* ]]; then
    filter="${target#*:}"
  fi

  echo "==> Miri: ${package}${filter:+ :: ${filter}}"
  args=(+nightly miri test -p "$package" --lib)
  if [[ -n "$filter" ]]; then
    args+=("$filter")
  fi

  MIRIFLAGS="$miri_flags" scripts/safe-run.sh --limit "${TSZ_MIRI_MEMORY_LIMIT:-75%}" -- \
    cargo "${args[@]}"
done
