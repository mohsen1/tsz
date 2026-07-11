#!/usr/bin/env bash
# Safe TypeScript corpus initialization shared by full-ci and its focused test.

materialize_typescript_corpus() {
  local root_dir="${ROOT_DIR:?ROOT_DIR must be set}"
  local corpus_path="$root_dir/TypeScript"
  local ref_file="$root_dir/scripts/ci/typescript-submodule-ref"
  local reset_helper="$root_dir/scripts/setup/reset-ts-submodule.sh"
  local expected_ref cached_ref

  expected_ref="$(tr -d '[:space:]' < "$ref_file")"

  # GitHub Actions may restore a source-only cache without Git metadata. Keep
  # an exact, complete cache, but never classify a Git checkout or symlink as
  # disposable merely because the historical gitlink no longer exists.
  if [[ ! -L "$corpus_path" \
    && ! -e "$corpus_path/.git" \
    && ! -L "$corpus_path/.git" \
    && -f "$corpus_path/.tsz-cache-ref" \
    && ! -L "$corpus_path/.tsz-cache-ref" ]]; then
    cached_ref="$(tr -d '[:space:]' < "$corpus_path/.tsz-cache-ref")"
    if [[ "$cached_ref" == "$expected_ref" && -f "$corpus_path/src/lib/es5.d.ts" ]]; then
      echo "Using cached TypeScript source tree at ${cached_ref}"
      return 0
    fi

    if [[ "${GITHUB_ACTIONS:-false}" != "true" ]]; then
      echo "error: refusing to delete stale local TypeScript source cache at $corpus_path" >&2
      echo "       Remove it explicitly, or let the guarded corpus reset helper inspect a Git checkout." >&2
      return 1
    fi

    echo "Discarding stale GitHub Actions TypeScript source cache at $corpus_path" >&2
    rm -rf -- "$corpus_path"
  fi

  "$reset_helper" --sparse
  test -f "$corpus_path/src/lib/es5.d.ts"
}
