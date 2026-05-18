#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

default_targets=(
  tsz-common:interner::tests::test_interner_intern_and_resolve
  tsz-scanner:scanner_impl::tests::scan_identifiers
  tsz-parser:parser::node_arena::tests::estimated_size_bytes_is_nonzero_for_empty_arena
  tsz-core:parallel::lib_snapshot::tests::disk_round_trip_resolves_identifier_text_and_symbols
)
targets=(${TSZ_MIRI_TARGETS:-${default_targets[*]}})
# The snapshot-cache target creates a temp directory. Keep strict provenance
# enabled, but disable Miri isolation so that filesystem-backed test can run.
miri_flags="${MIRIFLAGS:--Zmiri-strict-provenance -Zmiri-disable-isolation}"

for target in "${targets[@]}"; do
  package="${target%%:*}"
  filter=""
  if [[ "$target" == *:* ]]; then
    filter="${target#*:}"
  fi

  echo "==> Miri: ${package}${filter:+ :: ${filter}}"
  args=(run nightly cargo miri test -p "$package" --lib)
  if [[ -n "$filter" ]]; then
    args+=("$filter")
  fi

  MIRIFLAGS="$miri_flags" scripts/safe-run.sh --limit "${TSZ_MIRI_MEMORY_LIMIT:-75%}" -- \
    rustup "${args[@]}"
done
