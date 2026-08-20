#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

default_targets=(
  tsz-core:emit::tests::erases_type_only_syntax_and_annotations
  tsz-conformance:test_directives::tests::directive_line_basic_forms
)
targets=()
read -r -a targets <<< "${TSZ_MIRI_TARGETS:-${default_targets[*]}}"
# Keep strict provenance enabled. The retained conformance grammar test reads
# the shared spec vectors, so Miri isolation remains disabled for that target.
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
