# shellcheck shell=bash
# Result-cache fingerprint helpers for scripts/ci/project-compile-guard.sh.
#
# This file is sourced (never executed) by the compile guard and by
# scripts/ci/test-project-compile-guard-fingerprint.mjs so the no-op fast-path
# key has a single, unit-tested definition.
#
# The compile guard skips recompiling a project row when its fingerprint
# matches a prior run. For that fast path to be *stable* (no spurious misses
# that burn compile budget on unchanged rows) and *correct* (no stale hits),
# the fingerprint must track exactly the inputs that determine the result:
#   - the tsz binary (callers expose its hash as $_TSZ_BINARY_HASH),
#   - the tsconfig content,
#   - the compiled source tree's identity.
#
# Callers must set $_TSZ_BINARY_HASH before invoking compute_compile_fingerprint.

# Source globs that participate in a project's compiled-source identity. Kept in
# sync with count_ts_files plus the project tsconfigs (which also pull in JSON).
TSZ_COMPILE_FINGERPRINT_SOURCE_GLOBS=('*.ts' '*.tsx' '*.mts' '*.cts' '*.json')

# Stable sha256 of a file (Linux sha256sum / macOS shasum).
sha256_of_file() {
  sha256sum "$1" 2>/dev/null | awk '{print $1}' \
    || shasum -a 256 "$1" 2>/dev/null | awk '{print $1}' || true
}

# Stable sha256 of stdin. Same fallback chain as sha256_of_file; an empty input
# still yields a fixed digest, which keeps a clean/empty source identity stable.
sha256_of_stdin() {
  sha256sum 2>/dev/null | awk '{print $1}' \
    || shasum -a 256 2>/dev/null | awk '{print $1}' || true
}

# Echo the resolved physical path of $1 (handling a missing leaf) or fail.
tsz_fingerprint_resolve_physical() {
  local p="$1"
  if [[ -d "$p" ]]; then
    (cd "$p" 2>/dev/null && pwd -P)
    return
  fi
  local parent base parent_phys
  parent="$(dirname "$p")"
  base="$(basename "$p")"
  parent_phys="$(cd "$parent" 2>/dev/null && pwd -P)" || return 1
  printf '%s/%s\n' "$parent_phys" "$base"
}

# Content fingerprint of the compiled source tree under $1. Stable across
# regenerations because it hashes file *content* and the relative path, never
# mtime, so a regenerated-but-identical fixture keeps hitting the fast path.
# Prunes node_modules/.next like count_ts_files. Echoes "absent" when the tree
# is missing so a still-missing tree stays a stable key rather than an error.
hash_source_tree() {
  local dir="$1"
  [[ -d "$dir" ]] || {
    printf 'absent'
    return 0
  }

  local find_name=() glob first=1
  for glob in "${TSZ_COMPILE_FINGERPRINT_SOURCE_GLOBS[@]}"; do
    if [[ "$first" == "1" ]]; then
      find_name+=(-name "$glob")
      first=0
    else
      find_name+=(-o -name "$glob")
    fi
  done

  {
    find "$dir" \
      \( -path '*/node_modules/*' -o -path '*/.next/*' \) -prune -o \
      -type f \( "${find_name[@]}" \) -print 2>/dev/null || true
  } \
    | LC_ALL=C sort \
    | while IFS= read -r f; do
        printf '%s  %s\n' "$(sha256_of_file "$f")" "${f#"$dir"/}"
      done \
    | sha256_of_stdin
}

# Fingerprint for a check_project invocation:
#   <name>|<tsz binary hash>|<tsconfig hash>|<source identity>
# Returns empty on failure so callers treat caching as unavailable.
#
# Source identity:
#   - When the fixture directory is itself the toplevel of a git repository, use
#     HEAD plus a content-sensitive dirty marker (git diff against HEAD) and the
#     compiled source-tree content hash. A clean tree yields an empty, stable
#     marker; uncommitted tracked edits and untracked compiled source files
#     change the key, so a stale tree cannot falsely hit.
#   - Otherwise fall back to a content hash of the compiled source tree. This is
#     the case for generated-app rows, which have no per-fixture .git and live
#     inside the tsz checkout: a bare `git rev-parse HEAD` there walks up into
#     the tsz repository and reports the tsz toplevel/HEAD, which both ignores
#     the generated sources and changes on every tsz commit -- so the no-op fast
#     path would never be stable for those rows. Comparing the fixture directory
#     against the repo toplevel rejects that inherited-repo case directly.
compute_compile_fingerprint() {
  local name="$1" tsconfig="$2" src_dir="${3:-}"

  [[ -z "${_TSZ_BINARY_HASH:-}" ]] && return
  local tsconfig_hash=""
  [[ -f "$tsconfig" ]] && tsconfig_hash="$(sha256_of_file "$tsconfig")"
  [[ -f "$tsconfig" && -z "$tsconfig_hash" ]] && return

  local fixture_dir fixture_phys
  fixture_dir="$(dirname "$tsconfig")"
  [[ -n "$src_dir" ]] || src_dir="$fixture_dir"
  fixture_phys="$(tsz_fingerprint_resolve_physical "$fixture_dir" 2>/dev/null || true)"

  local source_id="" toplevel=""
  toplevel="$(git -C "$fixture_dir" rev-parse --show-toplevel 2>/dev/null || true)"
  if [[ -n "$toplevel" && -n "$fixture_phys" && "$toplevel" == "$fixture_phys" ]]; then
    local git_ref="" dirty_marker="" source_tree_marker=""
    git_ref="$(git -C "$fixture_dir" rev-parse HEAD 2>/dev/null || true)"
    dirty_marker="$(git -C "$fixture_dir" diff HEAD 2>/dev/null | sha256_of_stdin)"
    source_tree_marker="$(hash_source_tree "$src_dir")"
    source_id="git:${git_ref}:${dirty_marker}:tree:${source_tree_marker}"
  else
    source_id="tree:$(hash_source_tree "$src_dir")"
  fi

  printf '%s' "${name}|${_TSZ_BINARY_HASH}|${tsconfig_hash}|${source_id}"
}
