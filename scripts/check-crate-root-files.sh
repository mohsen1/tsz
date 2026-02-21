#!/usr/bin/env bash
set -euo pipefail

THRESHOLD="${TSZ_CRATE_ROOT_FILE_THRESHOLD:-4}"
CLI_BASE="${1:-}"
ENV_BASE="${TSZ_CRATE_ROOT_FILE_BASE:-}"
BASE_REF="${CLI_BASE:-$ENV_BASE}"

if ! [[ "$THRESHOLD" =~ ^[0-9]+$ ]]; then
  echo "TSZ_CRATE_ROOT_FILE_THRESHOLD must be an integer, got: $THRESHOLD" >&2
  exit 2
fi

resolve_base() {
  if [[ -n "$BASE_REF" ]] && git rev-parse --verify "$BASE_REF" >/dev/null 2>&1; then
    git rev-parse "$BASE_REF"
    return
  fi

  if [[ -n "$BASE_REF" ]]; then
    echo "crate-root-file check: provided base '$BASE_REF' is invalid; falling back" >&2
  fi

  if git rev-parse --verify origin/main >/dev/null 2>&1; then
    git merge-base HEAD origin/main
    return
  fi

  git rev-parse HEAD~1 2>/dev/null || git rev-parse HEAD
}

BASE_COMMIT="$(resolve_base)"

mapfile -t ADDED_ROOT_RS < <(
  git diff --diff-filter=A --name-only "$BASE_COMMIT"...HEAD \
    | awk '/^crates\/[^/]+\/src\/[^/]+\.rs$/'
)

if [[ "${#ADDED_ROOT_RS[@]}" -eq 0 ]]; then
  echo "crate-root-file check: no newly added root src/*.rs files detected"
  exit 0
fi

violations=()

for file in "${ADDED_ROOT_RS[@]}"; do
  base="$(basename "$file")"
  if [[ "$base" == "lib.rs" || "$base" == "main.rs" ]]; then
    continue
  fi

  crate_dir="$(dirname "$(dirname "$file")")"
  non_facade_root_count="$(find "$crate_dir/src" -maxdepth 1 -type f -name '*.rs' ! -name 'lib.rs' ! -name 'main.rs' | wc -l | tr -d '[:space:]')"

  if (( non_facade_root_count > THRESHOLD )); then
    violations+=("$file (crate non-facade root files: $non_facade_root_count, threshold: $THRESHOLD)")
  fi
done

if [[ "${#violations[@]}" -gt 0 ]]; then
  echo "crate-root-file check failed (base: $BASE_COMMIT):" >&2
  for violation in "${violations[@]}"; do
    echo "  - $violation" >&2
  done
  echo "Move new modules into domain folders (api/core/passes/diagnostics/tests fixtures) or raise threshold intentionally." >&2
  exit 1
fi

echo "crate-root-file check passed (base: $BASE_COMMIT)"
