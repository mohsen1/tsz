#!/usr/bin/env bash
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

  echo "Cache hit: ${label}"
  mkdir -p "$dest"
  if ! gsutil -q cp "$uri" "$archive"; then
    echo "warning: failed to download cache ${label}" >&2
    return 0
  fi
  if ! tar --warning=no-unknown-keyword -xzf "$archive" -C "$dest"; then
    echo "warning: failed to extract cache ${label}" >&2
    return 0
  fi
}

save_archive() {
  local label="$1" uri="$2" base="$3"
  shift 3

  if [[ "${TSZ_CI_CACHE_OVERWRITE:-0}" != "1" ]] && gsutil -q stat "$uri"; then
    echo "Cache save skipped: ${label} (already exists)"
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
  tar -czf "$archive" -C "$base" "${existing[@]}"
  if gsutil -q cp "$archive" "$uri"; then
    echo "Cache saved: ${label}"
  else
    echo "warning: failed to upload cache ${label}" >&2
  fi
}

suite_needs_rust_compile() {
  local suite
  suite="${_TSZ_CI_SUITE:-${TSZ_CI_SUITE:-all}}"
  case "$suite" in
    all|full|build|lint|unit|wasm|dist-binaries|unit-archive) return 0 ;;
    *) return 1 ;;
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

normalize_rust_source_mtimes() {
  local stamp="${TSZ_CI_CARGO_SOURCE_MTIME:-200001010000.00}"
  {
    printf '%s\0' Cargo.lock Cargo.toml .cargo/config.toml
    find crates -type f \
      \( -name '*.rs' -o -name Cargo.toml -o -name build.rs \) \
      -print0
  } | xargs -0 touch -t "$stamp"
}

restore_caches() {
  local cargo_hash node_hash ts_ref ts_deps_hash commit
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

    # Per-profile Cargo target caches, each keyed by Cargo.lock hash.
    # External dep artifacts are valid across commits (same lock = same versions)
    # and let Cargo skip those crates entirely. Workspace crate artifacts inside
    # are stale after any source change but get recompiled via sccache.
    # Separate archives per profile so each job only saves what it built.
    _restore_cargo_target_profile "cargo-target-deps"  "$cargo_hash"   # .target/dist-fast (dist-binaries, build_test_binaries)
    _restore_cargo_target_profile "cargo-target-unit"  "$cargo_hash"   # .target/ci-unit  (unit-archive, run_unit_tests)
    _restore_cargo_target_profile "cargo-target-debug" "$cargo_hash"   # .target/debug   (lint: cargo clippy / cargo fmt)
    _restore_cargo_target_profile "cargo-target-wasm"  "$cargo_hash"   # .target/wasm32-unknown-unknown (wasm-pack)
    if [[ -d .target/dist-fast || -d .target/ci-unit || -d .target/debug || -d .target/wasm32-unknown-unknown ]]; then
      normalize_rust_source_mtimes
    fi
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
  local cargo_hash node_hash ts_ref ts_deps_hash commit
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

    # Per-profile target caches keyed by Cargo.lock. Each job saves only the
    # profile it built; save_archive is a no-op when the path is absent.
    save_archive \
      "cargo-target-deps-${cargo_hash}" \
      "$(cache_uri "cargo-target-deps/${cargo_hash}.tar.gz")" \
      "." \
      .target/dist-fast
    save_archive \
      "cargo-target-unit-${cargo_hash}" \
      "$(cache_uri "cargo-target-unit/${cargo_hash}.tar.gz")" \
      "." \
      .target/ci-unit
    save_archive \
      "cargo-target-debug-${cargo_hash}" \
      "$(cache_uri "cargo-target-debug/${cargo_hash}.tar.gz")" \
      "." \
      .target/debug
    save_archive \
      "cargo-target-wasm-${cargo_hash}" \
      "$(cache_uri "cargo-target-wasm/${cargo_hash}.tar.gz")" \
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
