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
#   - the entry tsconfig content,
#   - the pinned tsc oracle protocol/content identity,
#   - the fixture project's config/source identity (including extended configs
#     and imported files outside the row's conventional source directory).
#
# Callers must set $_TSZ_BINARY_HASH before invoking compute_compile_fingerprint.

# Source/config globs that participate in a project's compile-input identity.
# JavaScript variants matter for allowJs projects; JSON covers tsconfig extends,
# package exports/types metadata, and resolveJsonModule inputs.
TSZ_COMPILE_FINGERPRINT_SOURCE_GLOBS=(
  '*.ts' '*.tsx' '*.mts' '*.cts'
  '*.js' '*.jsx' '*.mjs' '*.cjs'
  '*.json'
)

_TSZ_COMPILE_FINGERPRINT_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
_TSZ_COMPILE_FINGERPRINT_BATCH_HASHER="${_TSZ_COMPILE_FINGERPRINT_LIB_DIR}/project-source-tree-hash.mjs"

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
# Prunes only VCS metadata. Dependency and generated trees are compiler inputs
# when a config/import names them explicitly, so globally excluding
# `node_modules` or `.next` could replay a stale green result. Echoes "absent" when the tree
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

  local listing sorted_listing digest
  listing="$(mktemp)" || return 1
  sorted_listing="$(mktemp)" || {
    rm -f "$listing"
    return 1
  }
  # Follow dependency symlinks so package-manager layouts cannot hide an
  # explicitly compiled declaration outside the lexical row tree. A cycle or
  # unreadable target must disable caching instead of hashing a partial walk.
  if ! find -L "$dir" \
    \( -path '*/.git/*' \) -prune -o \
    -type f \( "${find_name[@]}" \) -print > "$listing" 2>/dev/null; then
    rm -f "$listing" "$sorted_listing"
    return 1
  fi
  # Preserve the legacy path/content stream while hashing every file in one
  # bounded process instead of spawning a checksum process per input file.
  if ! LC_ALL=C sort "$listing" > "$sorted_listing" \
    || ! digest="$(node "$_TSZ_COMPILE_FINGERPRINT_BATCH_HASHER" "$dir" "$sorted_listing" 2>/dev/null)"; then
    rm -f "$listing" "$sorted_listing"
    return 1
  fi
  rm -f "$listing" "$sorted_listing"
  [[ -n "$digest" ]] || return 1
  printf '%s' "$digest"
}

# Resolve the per-row project boundary. Fixture rows live immediately under
# FIXTURE_ROOT; nested app tsconfigs (e.g. repo/apps/web/tsconfig.json) must hash
# from that row root so base configs and imported siblings cannot evade the key.
tsz_fingerprint_project_root() {
  local fixture_dir="$1" fixture_phys="" fixture_root_phys="" relative=""
  fixture_phys="$(tsz_fingerprint_resolve_physical "$fixture_dir" 2>/dev/null || true)"
  fixture_root_phys="$(tsz_fingerprint_resolve_physical "${FIXTURE_ROOT:-}" 2>/dev/null || true)"
  if [[ -n "$fixture_phys" && -n "$fixture_root_phys" ]]; then
    case "$fixture_phys" in
      "$fixture_root_phys"/*)
        relative="${fixture_phys#"$fixture_root_phys"/}"
        printf '%s/%s\n' "$fixture_root_phys" "${relative%%/*}"
        return 0
        ;;
    esac
  fi
  printf '%s\n' "${fixture_phys:-$fixture_dir}"
}

# Content-sensitive identity shared by result and tsc-oracle caches. An owned
# fixture git repo is identified even when the entry tsconfig is nested below
# its toplevel. Generated rows that merely live inside the outer tsz checkout
# use their fixture-row content tree and never inherit the outer repository HEAD.
tsz_compile_input_identity() {
  local tsconfig="$1" src_dir="${2:-}"
  local fixture_dir project_root project_root_phys toplevel=""
  fixture_dir="$(dirname "$tsconfig")"
  [[ -n "$src_dir" ]] || src_dir="$fixture_dir"
  project_root="$(tsz_fingerprint_project_root "$fixture_dir")"
  project_root_phys="$(tsz_fingerprint_resolve_physical "$project_root" 2>/dev/null || true)"
  toplevel="$(git -C "$fixture_dir" rev-parse --show-toplevel 2>/dev/null || true)"

  if [[ -n "$toplevel" && -n "$project_root_phys" && "$toplevel" == "$project_root_phys" ]]; then
    local git_ref="" dirty_marker="" project_tree_marker=""
    git_ref="$(git -C "$toplevel" rev-parse HEAD 2>/dev/null || true)"
    dirty_marker="$(git -C "$toplevel" diff HEAD 2>/dev/null | sha256_of_stdin)"
    project_tree_marker="$(hash_source_tree "$toplevel")" || return 1
    [[ -n "$project_tree_marker" ]] || return 1
    printf '%s' "git:${git_ref}:${dirty_marker}:tree:${project_tree_marker}"
  else
    local project_tree_marker=""
    project_tree_marker="$(hash_source_tree "$project_root")" || return 1
    [[ -n "$project_tree_marker" ]] || return 1
    printf '%s' "tree:${project_tree_marker}"
  fi
}

# Fingerprint for a check_project invocation:
#   <name>|<tsz binary hash>|<oracle hash>|<tsconfig hash>|<input identity>
# Returns empty on failure so callers treat caching as unavailable.
#
# Input identity covers the whole fixture-row project boundary, not only the
# caller's src_dir. Owned git fixtures (including nested app configs) use HEAD,
# tracked dirty content, and relevant config/source content across the repo.
# Generated rows use the same full row content tree without inheriting the outer
# tsz repository HEAD. Caches remain opt-in because whole-row hashing is
# intentionally conservative and can be expensive for installed applications.
compute_compile_fingerprint() {
  local name="$1" tsconfig="$2" src_dir="${3:-}"

  [[ -z "${_TSZ_BINARY_HASH:-}" ]] && return
  local tsconfig_hash=""
  [[ -f "$tsconfig" ]] && tsconfig_hash="$(sha256_of_file "$tsconfig")"
  [[ -f "$tsconfig" && -z "$tsconfig_hash" ]] && return

  local source_id=""
  source_id="$(tsz_compile_input_identity "$tsconfig" "$src_dir")"
  [[ -n "$source_id" ]] || return

  printf '%s' "${name}|${_TSZ_BINARY_HASH}|${_TSZ_TSC_ORACLE_HASH:-unavailable}|${tsconfig_hash}|${source_id}"
}
