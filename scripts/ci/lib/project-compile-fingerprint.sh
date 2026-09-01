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

# Hash shell values without delimiter ambiguity. Each value is byte-length and
# NUL framed under the C locale, so embedded newlines and separators cannot
# manufacture the same protocol identity.
tsz_hash_framed_values() {
  local value LC_ALL=C
  {
    printf 'tsz-framed-values-v1\0'
    for value in "$@"; do
      LC_ALL=C printf '%s\0' "${#value}"
      printf '%s\0' "$value"
    done
  } | sha256_of_stdin
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

# Content fingerprint of the compiled project tree under $1. Stable across
# regenerations because it hashes file *content* and the relative graph, never
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

  # One bounded graph walk follows dependency symlinks without expanding every
  # alias into a global path list. It records raw link targets and deterministic
  # back-references while hashing each physical file once. Source mode includes
  # ordinary files with arbitrary suffixes because allowNonTsExtensions can make
  # any explicit root a compiler input; unrelated assets cause only safe misses.
  # These optional
  # environment limits fail closed rather than publishing a partial digest:
  # TSZ_PROJECT_SOURCE_HASH_MAX_{NODES,EDGES,DEPTH,DIRECTORY_ENTRIES,
  # BYTES,PATH_BYTES,MILLISECONDS}.
  local digest
  digest="$(node "$_TSZ_COMPILE_FINGERPRINT_BATCH_HASHER" \
    "$dir" --source-tree 2>/dev/null)" || return 1
  [[ "$digest" =~ ^[0-9a-f]{64}$ ]] || return 1
  printf '%s' "$digest"
}

# Exact identity of the builtin declaration library consumed by TypeScript 7.
# This is deliberately narrower than hash_source_tree: every lib*.d.ts in the
# resolved compiler lib directory participates, and no unrelated package file
# can stand in for a missing builtin library set.
hash_builtin_lib_tree() {
  local dir="$1" digest
  [[ -d "$dir" ]] || return 1
  digest="$(node "$_TSZ_COMPILE_FINGERPRINT_BATCH_HASHER" \
    "$dir" --builtin-libs 2>/dev/null)" || return 1
  [[ "$digest" =~ ^[0-9a-f]{64}$ ]] || return 1
  printf '%s' "$digest"
}

# Bind the oracle protocol to the pinned mapping/package metadata, every
# launcher word that resolves to executable content, the native compiler, and
# all builtin declaration contents. Missing inputs make oracle evidence
# unavailable instead of producing a partial cache key.
tsz_oracle_identity_fingerprint() {
  local protocol="$1" builtin_dir="$2" native_exe="$3"
  local versions_file="$4" wrapper_package_json="$5"
  shift 5
  [[ -f "$versions_file" && -f "$wrapper_package_json" \
    && -f "$native_exe" && -d "$builtin_dir" ]] || return 1

  local native_package_json builtin_hash versions_hash wrapper_hash native_hash native_package_hash="absent"
  native_package_json="$(dirname "$(dirname "$native_exe")")/package.json"
  builtin_hash="$(hash_builtin_lib_tree "$builtin_dir")" || return 1
  versions_hash="$(sha256_of_file "$versions_file")"
  wrapper_hash="$(sha256_of_file "$wrapper_package_json")"
  native_hash="$(sha256_of_file "$native_exe")"
  [[ "$versions_hash" =~ ^[0-9a-f]{64}$ && "$wrapper_hash" =~ ^[0-9a-f]{64}$ \
    && "$native_hash" =~ ^[0-9a-f]{64}$ ]] || return 1
  if [[ -f "$native_package_json" ]]; then
    native_package_hash="$(sha256_of_file "$native_package_json")"
    [[ "$native_package_hash" =~ ^[0-9a-f]{64}$ ]] || return 1
  fi

  local fields=(
    protocol "$protocol"
    pinned-mapping "$versions_hash"
    wrapper-package "$wrapper_hash"
    native-package "$native_package_hash"
    builtin-libs "$builtin_hash"
    native-path "$native_exe"
    native-content "$native_hash"
  )
  local word resolved word_hash
  for word in "$@"; do
    fields+=(command-word "$word")
    resolved=""
    if [[ -f "$word" ]]; then
      resolved="$word"
    elif command -v "$word" >/dev/null 2>&1; then
      resolved="$(command -v "$word")"
    fi
    if [[ -n "$resolved" && -f "$resolved" ]]; then
      word_hash="$(sha256_of_file "$resolved")"
      [[ "$word_hash" =~ ^[0-9a-f]{64}$ ]] || return 1
      fields+=(command-path "$resolved" command-content "$word_hash")
    fi
  done
  tsz_hash_framed_values "${fields[@]}"
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
#   <name>|<tsz binary hash>|<oracle hash>|<source overlay>|<evidence protocol>
#   |<tsconfig hash>|<input identity>
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

  printf '%s' "${name}|${_TSZ_BINARY_HASH}|${_TSZ_TSC_ORACLE_HASH:-unavailable}|${_TSZ_SOURCE_OVERLAY_HASH:-unavailable}|${_TSZ_EVIDENCE_PROTOCOL_HASH:-unavailable}|${tsconfig_hash}|${source_id}"
}
