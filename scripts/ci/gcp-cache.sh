#!/usr/bin/env bash
#
# CI cache restore/save against GCS.
#
# Cache-policy invariants (DO NOT regress):
#
#   1. Writes are gated by branch.
#      Only push events on refs/heads/main publish blobs to GCS. PRs and
#      merge_group runs read main's latest blob and recompile their
#      delta locally. They do NOT publish their (stale-relative-to-main)
#      target dirs as the new shared blob. This eliminates the prior
#      "first PR to ever populate this Cargo.lock-keyed blob owns it
#      forever" trap. TSZ_CI_CACHE_OVERWRITE=1 escapes the gate for
#      emergency repairs (workflow_dispatch from main).
#
#   2. Saves overwrite. A stale main blob always loses to the next
#      successful main build. There is no "skip if exists" path on the
#      write side anymore.
#
#   3. Cache keys include rustc version. Mixing artifacts across rustc
#      versions is unsafe — every rustc fingerprint encodes the compiler
#      version, so a cached blob built with rustc 1.95 must not be
#      restored into a workspace running 1.96.
#
#   4. Source mtimes are NOT touched. Cargo's content-hash check is the
#      correctness backstop against using stale .rlib files; bypassing
#      its mtime input via `touch -t 200001010000` (the prior trick) can
#      let actual source changes slip through. sccache is the right tool
#      for cross-commit Rust reuse — it keys on real compiler inputs.
#
# Run with TSZ_CI_DEBUG_CACHE=1 to enable bash xtrace for cache ops.
set -Eeuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

CACHE_BUCKET="${_TSZ_CI_CACHE_BUCKET:?_TSZ_CI_CACHE_BUCKET is required}"
CACHE_BUCKET="${CACHE_BUCKET%/}"

cache_uri() {
  printf '%s/%s\n' "$CACHE_BUCKET" "$1"
}

hash_files() {
  sha256sum "$@" | sha256sum | awk '{print $1}'
}

scripts_deps_hash() {
  local files=(
    scripts/package.json
    scripts/conformance/typescript-versions.json
  )
  if [[ -f scripts/package-lock.json ]]; then
    files+=(scripts/package-lock.json)
  fi
  hash_files "${files[@]}"
}

cargo_lock_hash() {
  hash_files Cargo.lock
}

# rustc identity (version + target triple). Mixing object files / .rmeta
# across rustc versions is unsafe — every rustc fingerprint encodes the
# compiler version, so a cached blob built with rustc 1.95 must not be
# restored into a workspace running 1.96.
rustc_version_hash() {
  if command -v rustc >/dev/null 2>&1; then
    rustc -Vv 2>&1 | sha256sum | awk '{print $1}'
  else
    # Called before host tools install: refuse to share blobs across
    # unknown compilers. Caller's stat will miss; safer than colliding
    # with the rustc-aware blob.
    printf 'no-rustc\n'
  fi
}

# Composite cache key for Rust target caches.
#   <cargo_lock_hash>-<rustc_version_hash>
# Independent of branch and commit. main and PRs read the same blob;
# only main writes (see cache_writes_allowed below).
cargo_target_cache_key() {
  printf '%s-%s\n' "$(cargo_lock_hash)" "$(rustc_version_hash)"
}

# Decide whether this CI run is allowed to write back to the GCS cache.
#
# Only `push` events on main update the cache. PRs and merge_group runs
# read main's latest blob, recompile their delta locally, but never
# publish their own (stale-relative-to-main) target dirs as the new
# shared blob. This eliminates the "first PR to ever populate this lock
# hash owns it forever" trap that the prior `skip if exists` policy
# created.
#
# TSZ_CI_CACHE_OVERWRITE=1 escapes the gate (e.g., emergency
# workflow_dispatch repair).
cache_writes_allowed() {
  if [[ "${TSZ_CI_CACHE_OVERWRITE:-0}" == "1" ]]; then
    return 0
  fi
  case "${GITHUB_REF:-}" in
    refs/heads/main) return 0 ;;
    *) return 1 ;;
  esac
}

typescript_ref() {
  tr -d '[:space:]' < scripts/ci/typescript-submodule-ref
}

typescript_deps_hash() {
  local files=()
  if [[ -f TypeScript/package.json ]]; then
    files+=(TypeScript/package.json)
  fi
  if [[ -f TypeScript/package-lock.json ]]; then
    files+=(TypeScript/package-lock.json)
  fi
  if [[ "${#files[@]}" -eq 0 ]]; then
    printf 'missing-package-files\n'
    return 0
  fi
  hash_files "${files[@]}"
}

commit_key() {
  local key="${COMMIT_SHA:-${REVISION_ID:-}}"
  if [[ -z "$key" || "$key" == "HEAD" ]]; then
    key="$(git rev-parse HEAD 2>/dev/null || true)"
  fi
  if [[ -z "$key" ]]; then
    key="unknown"
  fi
  printf '%s\n' "$key"
}

tmp_archive() {
  local label="$1"
  label="${label//[^A-Za-z0-9_.-]/-}"
  printf '/tmp/tsz-cache-%s-%s.tar.gz\n' "$label" "$$"
}

_restore_cargo_target_profile() {
  local label="$1" hash="$2"
  local uri fallback_uri
  uri="$(cache_uri "${label}/${hash}.tar.gz")"
  if gsutil -q stat "$uri"; then
    restore_archive "${label}-${hash}" "$uri" "."
  else
    echo "Cache miss: ${label}-${hash}"
    fallback_uri="$(gsutil ls -l "$(cache_uri "${label}/*.tar.gz")" 2>/dev/null \
      | grep -v '^TOTAL:' \
      | sort -k2 -r \
      | head -1 \
      | awk '{print $NF}' || true)"
    if [[ -n "$fallback_uri" ]]; then
      echo "Cache warm-fallback: ${label} from ${fallback_uri}"
      restore_archive "${label}-warm-fallback" "$fallback_uri" "."
    fi
  fi
}

restore_archive() {
  local label="$1" uri="$2" dest="$3"
  local archive
  archive="$(tmp_archive "$label")"

  if ! gsutil -q stat "$uri"; then
    echo "Cache miss: ${label}"
    return 0
  fi

  local t0=$SECONDS
  if ! gsutil -q cp "$uri" "$archive"; then
    echo "warning: failed to download cache ${label}" >&2
    rm -f "$archive"
    return 0
  fi
  local download_secs=$((SECONDS - t0))
  local size_h
  size_h="$(du -h "$archive" 2>/dev/null | awk '{print $1}')"

  mkdir -p "$dest"
  local t1=$SECONDS
  if ! tar --warning=no-unknown-keyword -xzf "$archive" -C "$dest"; then
    echo "warning: failed to extract cache ${label}" >&2
    rm -f "$archive"
    return 0
  fi
  local extract_secs=$((SECONDS - t1))
  rm -f "$archive"

  echo "Cache hit: ${label} (size=${size_h:-?}, download=${download_secs}s, extract=${extract_secs}s)"
}

# save_archive <label> <gs://...> <base> <path...>
#
# Cache write policy: only push-on-main runs publish blobs. PRs and
# merge_group runs always log the reason a save was skipped.
# TSZ_CI_CACHE_OVERWRITE=1 forces a write regardless of branch.
save_archive() {
  local label="$1" uri="$2" base="$3"
  shift 3

  if ! cache_writes_allowed; then
    echo "Cache save skipped: ${label} (writes disabled on ${GITHUB_REF:-unknown ref})"
    return 0
  fi

  if [[ ! -d "$base" ]]; then
    echo "Cache save skipped: ${label} (${base} missing)"
    return 0
  fi

  local existing=()
  local path
  for path in "$@"; do
    if [[ -e "$base/$path" ]]; then
      existing+=("$path")
    fi
  done

  if [[ "${#existing[@]}" -eq 0 ]]; then
    echo "Cache save skipped: ${label} (no paths)"
    return 0
  fi

  local archive
  archive="$(tmp_archive "$label")"
  local t0=$SECONDS
  tar -czf "$archive" -C "$base" "${existing[@]}"
  local pack_secs=$((SECONDS - t0))
  local size_h
  size_h="$(du -h "$archive" 2>/dev/null | awk '{print $1}')"

  local t1=$SECONDS
  if gsutil -q cp "$archive" "$uri"; then
    local upload_secs=$((SECONDS - t1))
    echo "Cache saved: ${label} (size=${size_h:-?}, pack=${pack_secs}s, upload=${upload_secs}s)"
  else
    echo "warning: failed to upload cache ${label}" >&2
  fi
  rm -f "$archive"
}

suite_needs_rust_compile() {
  local suite
  suite="${_TSZ_CI_SUITE:-${TSZ_CI_SUITE:-all}}"
  case "$suite" in
    all|full|build|lint|unit|wasm|wasm-web|dist-binaries|unit-archive) return 0 ;;
    *) return 1 ;;
  esac
}

# Which cargo-target-* GCS archives a suite actually needs.
# Restoring archives a job will not use is pure overhead. Each suite lists
# only the profile(s) it compiles into; sccache GCS handles cross-commit
# rustc-level reuse for all profiles.
#
# lint deliberately gets nothing: the ci-lint profile lives in
# .target/ci-lint/ and the cost of archiving + transferring +
# fingerprint-revalidating that target dir routinely exceeded the
# wall-clock saved by skipping recompilation. sccache GCS is the right
# tool for cross-commit lint — it keys on actual rustc inputs and can't
# go stale silently. See the prior "cargo-target-debug stale forever"
# bug fixed in this redesign.
suite_target_caches() {
  local suite
  suite="${_TSZ_CI_SUITE:-${TSZ_CI_SUITE:-all}}"
  case "$suite" in
    all|full)              echo "cargo-target-deps cargo-target-unit cargo-target-wasm" ;;
    build)                 echo "cargo-target-deps cargo-target-unit" ;;
    dist-binaries)         echo "cargo-target-deps" ;;
    unit-archive|unit)     echo "cargo-target-unit" ;;
    lint)                  echo "" ;;
    wasm|wasm-web)         echo "cargo-target-wasm" ;;
    *)                     echo "" ;;
  esac
}

restore_typescript() {
  local ref cache archive
  ref="$(typescript_ref)"
  cache="$(cache_uri "typescript/${ref}.tar.gz")"
  archive="/tmp/typescript-${ref}.tar.gz"

  rm -rf TypeScript
  if gsutil -q stat "$cache"; then
    echo "TypeScript cache hit: ${cache}"
    if gsutil -q cp "$cache" "$archive" \
      && tar --warning=no-unknown-keyword -xzf "$archive" -C . \
      && [[ -f TypeScript/src/lib/es5.d.ts ]] \
      && [[ "$(tr -d '[:space:]' < TypeScript/.tsz-cache-ref)" == "$ref" ]]; then
      return 0
    fi
    echo "warning: TypeScript cache was unusable; refetching ${ref}" >&2
    rm -rf TypeScript
  else
    echo "TypeScript cache miss: ${cache}"
  fi

  if ! command -v curl >/dev/null 2>&1; then
    apt-get update -qq
    apt-get install -y --no-install-recommends ca-certificates curl
  fi

  mkdir -p TypeScript
  curl -fsSL "https://codeload.github.com/microsoft/TypeScript/tar.gz/${ref}" \
    -o "${archive}.upstream"
  tar -xzf "${archive}.upstream" -C TypeScript --strip-components=1
  echo "$ref" > TypeScript/.tsz-cache-ref
  tar -czf "$archive" TypeScript
  gsutil -q cp "$archive" "$cache"

  test -f TypeScript/src/lib/es5.d.ts
}

restore_caches() {
  local cargo_hash node_hash ts_ref ts_deps_hash commit cargo_target_key
  cargo_hash="$(cargo_lock_hash)"
  node_hash="$(scripts_deps_hash)"
  ts_ref="$(typescript_ref)"
  commit="$(commit_key)"

  restore_typescript
  ts_deps_hash="$(typescript_deps_hash)"

  mkdir -p .ci-cache/cargo-home .ci-cache/npm .target scripts

  if suite_needs_rust_compile; then
    restore_archive \
      "cargo-home-${cargo_hash}" \
      "$(cache_uri "cargo-home/${cargo_hash}.tar.gz")" \
      ".ci-cache/cargo-home"

    # Per-profile Cargo target caches keyed by (Cargo.lock + rustc version).
    # External dep artifacts are valid across commits (same lock = same
    # versions) and let Cargo skip those crates entirely. Cross-rustc
    # mixing is unsafe so a rustc upgrade rolls the key.
    cargo_target_key="$(cargo_target_cache_key)"
    echo "Cargo target cache key: ${cargo_target_key}"
    local target_cache
    for target_cache in $(suite_target_caches); do
      _restore_cargo_target_profile "$target_cache" "$cargo_target_key"
    done
    # NOTE: we deliberately do NOT backdate source-file mtimes. A previous
    # version of this script ran "touch -t 200001010000" over every .rs and
    # Cargo.toml after a target-dir restore, on the theory that mtimes older
    # than the cached fingerprints would let Cargo skip recompilation.
    # That is exactly the case where Cargo's content-hash safety net is
    # supposed to catch real source changes — and bypassing the mtime input
    # to that check can mask genuine staleness. Correctness > cache hits.
    # sccache handles cross-commit reuse via content-keyed lookups, which is
    # the right tool for "same source, same compile flags, skip rustc."
  else
    echo "Cache restore skipped: cargo-home + cargo-target (suite does not compile Rust)"
  fi

  restore_archive \
    "npm-${node_hash}" \
    "$(cache_uri "npm/${node_hash}.tar.gz")" \
    ".ci-cache/npm"

  restore_archive \
    "scripts-node-modules-${node_hash}" \
    "$(cache_uri "scripts-node-modules/${node_hash}.tar.gz")" \
    "scripts"

  if [[ "${TSZ_CI_SKIP_TS_HARNESS_RESTORE:-0}" != "1" ]]; then
    restore_archive \
      "typescript-harness-${ts_ref}" \
      "$(cache_uri "typescript-harness/${ts_ref}.tar.gz")" \
      "TypeScript"

    restore_archive \
      "typescript-node-modules-${ts_ref}-${ts_deps_hash}" \
      "$(cache_uri "typescript-node-modules/${ts_ref}-${ts_deps_hash}.tar.gz")" \
      "TypeScript" \
      node_modules
  fi

  if [[ "${TSZ_CI_SKIP_DIST_RESTORE:-0}" != "1" && "$commit" != "unknown" ]]; then
    local dist_cache
    dist_cache="$(cache_uri "dist-fast/${commit}.tar.gz")"
    if gsutil -q stat "$dist_cache"; then
      restore_archive \
        "dist-fast-${commit}" \
        "$dist_cache" \
        ".target"
      mkdir -p .ci-cache
      printf '%s\n' "$commit" > .ci-cache/dist-fast-cache-hit
      touch -c \
        .target/dist-fast/tsz \
        .target/dist-fast/tsz-server \
        .target/dist-fast/tsz-conformance \
        .target/dist-fast/generate-tsc-cache
    else
      echo "Cache miss: dist-fast-${commit}"
    fi
  fi
}

save_caches() {
  local cargo_hash node_hash ts_ref ts_deps_hash commit cargo_target_key
  cargo_hash="$(cargo_lock_hash)"
  node_hash="$(scripts_deps_hash)"
  ts_ref="$(typescript_ref)"
  ts_deps_hash="$(typescript_deps_hash)"
  commit="$(commit_key)"

  if suite_needs_rust_compile; then
    save_archive \
      "cargo-home-${cargo_hash}" \
      "$(cache_uri "cargo-home/${cargo_hash}.tar.gz")" \
      ".ci-cache/cargo-home" \
      registry git

    # Per-profile target caches keyed by (Cargo.lock + rustc version).
    # Each job saves only the profile it built; save_archive is a no-op
    # when the path is absent. cargo-target-debug intentionally removed:
    # the lint suite now writes to .target/ci-lint/ via the dedicated
    # ci-lint Cargo profile, and sccache GCS handles cross-commit reuse
    # for that workspace path. The old cargo-target-debug blob was
    # routinely stale because PRs would never overwrite an existing
    # blob; new write policy (main-only) plus profile isolation makes
    # the dedicated lint cache redundant.
    cargo_target_key="$(cargo_target_cache_key)"
    save_archive \
      "cargo-target-deps-${cargo_target_key}" \
      "$(cache_uri "cargo-target-deps/${cargo_target_key}.tar.gz")" \
      "." \
      .target/dist-fast
    save_archive \
      "cargo-target-unit-${cargo_target_key}" \
      "$(cache_uri "cargo-target-unit/${cargo_target_key}.tar.gz")" \
      "." \
      .target/ci-unit
    save_archive \
      "cargo-target-wasm-${cargo_target_key}" \
      "$(cache_uri "cargo-target-wasm/${cargo_target_key}.tar.gz")" \
      "." \
      .target/wasm32-unknown-unknown
  fi

  save_archive \
    "npm-${node_hash}" \
    "$(cache_uri "npm/${node_hash}.tar.gz")" \
    ".ci-cache" \
    npm

  save_archive \
    "scripts-node-modules-${node_hash}" \
    "$(cache_uri "scripts-node-modules/${node_hash}.tar.gz")" \
    "scripts" \
    node_modules

  save_archive \
    "typescript-harness-${ts_ref}" \
    "$(cache_uri "typescript-harness/${ts_ref}.tar.gz")" \
    "TypeScript" \
    built/local

  save_archive \
    "typescript-node-modules-${ts_ref}-${ts_deps_hash}" \
    "$(cache_uri "typescript-node-modules/${ts_ref}-${ts_deps_hash}.tar.gz")" \
    "TypeScript" \
    node_modules

  if [[ "$commit" != "unknown" ]]; then
    save_archive \
      "dist-fast-${commit}" \
      "$(cache_uri "dist-fast/${commit}.tar.gz")" \
      ".target" \
      dist-fast/tsz \
      dist-fast/tsz-server \
      dist-fast/tsz-conformance \
      dist-fast/generate-tsc-cache
  fi
}

main() {
  case "${1:-}" in
    restore)
      restore_caches
      ;;
    save)
      save_caches
      ;;
    *)
      echo "usage: $0 restore|save" >&2
      return 2
      ;;
  esac
}

main "$@"
