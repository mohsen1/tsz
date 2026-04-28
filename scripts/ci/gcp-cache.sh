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
#
# Returns either a hex sha256 (rustc available) or the literal "norustc"
# (no internal dash, so the warm-fallback's `${hash##*-}` parser cleanly
# returns the marker as-is rather than splitting on an interior dash).
# Also guards against `rustc -Vv` succeeding the binary check but exiting
# non-zero or producing empty output — in that case we'd otherwise hash
# stderr text and store under a bogus stable-looking key.
rustc_version_hash() {
  if command -v rustc >/dev/null 2>&1; then
    local out rc=0
    out="$(rustc -Vv 2>&1)" || rc=$?
    if [[ "$rc" -eq 0 && -n "$out" ]]; then
      printf '%s' "$out" | sha256sum | awk '{print $1}'
      return 0
    fi
  fi
  printf 'norustc\n'
}

# Hash of profile-affecting workspace inputs. Two workspaces with the
# same Cargo.lock + rustc but different .cargo/config.toml or different
# [profile.*] sections in the workspace Cargo.toml would otherwise share
# a blob and cargo's fingerprint would invalidate every artifact —
# guaranteed cold rebuild that we'd silently pay for. Folding these into
# the cache key isolates the cache automatically when profile shape
# changes.
profile_config_hash() {
  local files=(Cargo.toml)
  if [[ -f .cargo/config.toml ]]; then
    files+=(.cargo/config.toml)
  fi
  hash_files "${files[@]}"
}

# Composite cache key for Rust target caches.
#   <cargo_lock_hash>-<profile_config_hash>-<rustc_version_hash>
# Independent of branch and commit. main and PRs read the same blob;
# only main writes (see cache_writes_allowed below).
# Order matters for the warm-fallback parser: the rustc hash MUST be the
# trailing segment, since the parser at _restore_cargo_target_profile
# extracts it via `${hash##*-}`.
cargo_target_cache_key() {
  printf '%s-%s-%s\n' \
    "$(cargo_lock_hash)" \
    "$(profile_config_hash)" \
    "$(rustc_version_hash)"
}

# Decide whether this CI run is allowed to write back to the GCS cache.
#
# Writes are gated on the workflow ref, not the event name. Any run
# whose GITHUB_REF is refs/heads/main may publish — that includes:
#   - push to main
#   - schedule (cron jobs against main)
#   - workflow_dispatch dispatched against main
#   - merge_group runs (GitHub sets GITHUB_REF to refs/heads/<base>
#     for the merge queue)
# PRs and feature branches always have a non-main ref so they're
# automatically read-only against the shared cache. This eliminates the
# "first PR to ever populate this lock hash owns it forever" trap of
# the prior `skip if exists` policy.
#
# TSZ_CI_CACHE_OVERWRITE=1 escapes the gate (emergency
# workflow_dispatch repair from a feature branch, etc.).
#
# Note: writes still happen even when the suite that produced the
# target dir failed. github-suite.sh skips save when its run rc != 0
# precisely so a half-failed build doesn't get published to main as
# the new shared baseline.
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

create_archive() {
  local archive="$1" base="$2"
  shift 2
  COPYFILE_DISABLE=1 tar --exclude='._*' -czf "$archive" -C "$base" "$@"
}

validate_typescript_cache_tree() {
  local ref="$1"
  python3 - "$ref" <<'PY'
import sys
from pathlib import Path

ref = sys.argv[1]
root = Path("TypeScript")
ref_file = root / ".tsz-cache-ref"

if not (root / "src/lib/es5.d.ts").is_file():
    print("warning: TypeScript cache missing src/lib/es5.d.ts", file=sys.stderr)
    raise SystemExit(1)

if not ref_file.is_file() or ref_file.read_text(encoding="utf-8", errors="replace").strip() != ref:
    print("warning: TypeScript cache ref marker mismatch", file=sys.stderr)
    raise SystemExit(1)

bad = next(root.rglob("._*"), None)
if bad is not None:
    print(f"warning: TypeScript cache contains AppleDouble file: {bad}", file=sys.stderr)
    raise SystemExit(1)
PY
}

_restore_cargo_target_profile() {
  local label="$1" hash="$2"
  local uri fallback_uri
  uri="$(cache_uri "${label}/${hash}.tar.gz")"
  if gsutil -q stat "$uri"; then
    restore_archive "${label}-${hash}" "$uri" "."
  else
    echo "Cache miss: ${label}-${hash}"
    # Warm-fallback: pick the most recent blob whose key ends with the
    # SAME rustc version as the current host. Crossing rustc versions is
    # unsafe per invariant #3 — cargo's fingerprint check would catch it
    # and force a full rebuild, but we'd still pay the GCS download.
    # The glob restricts to the rustc-suffix portion of the composite key
    # (`<lock>-<rustc>`); different Cargo.lock hashes are still acceptable
    # (cargo will recompile only the changed deps).
    local rustc_part="${hash##*-}"
    fallback_uri="$(gsutil ls -l "$(cache_uri "${label}/*-${rustc_part}.tar.gz")" 2>/dev/null \
      | grep -v '^TOTAL:' \
      | sort -k2 -r \
      | head -1 \
      | awk '{print $NF}' || true)"
    if [[ -n "$fallback_uri" ]]; then
      echo "Cache warm-fallback: ${label} from ${fallback_uri}"
      restore_archive "${label}-warm-fallback" "$fallback_uri" "."
    else
      echo "Cache warm-fallback: ${label} no rustc-${rustc_part} blob available"
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
  create_archive "$archive" "$base" "${existing[@]}"
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

# Per-suite list of "cache feature" tags this suite needs restored.
# Restoring features the suite doesn't use is pure runner-minute and GCS
# bandwidth cost. Lint, for instance, never reads the TypeScript source
# tree, npm cache, or scripts/node_modules — only its Rust state.
#
# Recognized tags:
#   cargo-home               — Cargo registry/git cache (.ci-cache/cargo-home)
#   typescript-source        — TypeScript source tree (lib + tests/cases)
#   npm                      — global npm cache (.ci-cache/npm)
#   scripts-node-modules     — scripts/node_modules
#   typescript-harness       — TypeScript/built/local
#   typescript-node-modules  — TypeScript/node_modules
#   dist-fast-commit         — commit-keyed dist-fast binary tarball
#
# Per-profile cargo target-dir caches (cargo-target-deps, etc.) are
# selected separately via suite_target_caches() and gated implicitly by
# cargo-home (no point restoring a target without registry).
suite_caches() {
  local suite
  suite="${_TSZ_CI_SUITE:-${TSZ_CI_SUITE:-all}}"
  case "$suite" in
    all|full)
      echo "cargo-home typescript-source npm scripts-node-modules typescript-harness typescript-node-modules dist-fast-commit"
      ;;
    lint)
      # Only `cargo clippy` on workspace crates. Doesn't run cargo build,
      # doesn't read TypeScript/ at compile time, doesn't run any Node
      # tooling. cargo-home (registry) is the only useful restore.
      echo "cargo-home"
      ;;
    build|dist-binaries|unit-archive|unit|wasm|wasm-web)
      # cargo build / cargo nextest: workspace crates and tests reference
      # TypeScript/src/lib (and tests/cases for some integration tests),
      # and the wasm post-build step copies TypeScript/src/lib into the
      # pkg output. Need TS source even though no Node tooling is run.
      echo "cargo-home typescript-source"
      ;;
    unit-shard)
      # Downloads the nextest archive directly from GCS. No cache restore.
      echo ""
      ;;
    conformance)
      # tsz-conformance binary comes from the dist-fast-commit blob, the
      # corpus comes from TypeScript source. No npm/harness needed.
      echo "typescript-source dist-fast-commit"
      ;;
    conformance-aggregate|emit-aggregate|fourslash-aggregate)
      # Aggregates pull per-shard JSONs from GCS via gsutil only.
      # No cargo-home, no TS source, no Node modules.
      # Mirror in gcp-full-ci.sh:suite_needs_typescript_source().
      echo ""
      ;;
    emit|fourslash)
      # Full Node-driven test run: TypeScript source + harness + tsz binary.
      echo "typescript-source npm scripts-node-modules typescript-harness typescript-node-modules dist-fast-commit"
      ;;
    emit-shard|fourslash-shard)
      # Shards get TS harness via the node-harness artifact and tsz via
      # the dist-fast-binaries artifact, so no harness/dist-fast restore
      # is needed beyond TS source for path resolution + scripts deps for
      # Node tooling.
      echo "typescript-source npm scripts-node-modules"
      ;;
    node-harness-prep)
      # Builds TypeScript/built/local + scripts/emit/dist for downstream
      # shards.
      echo "typescript-source npm scripts-node-modules typescript-harness typescript-node-modules"
      ;;
    *)
      # Unknown suite: conservative default is empty. Caller should
      # extend this case if a new suite is added.
      echo ""
      ;;
  esac
}

suite_has_cache() {
  local needle="$1"
  local needles=" $(suite_caches) "
  [[ "$needles" == *" $needle "* ]]
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
      && validate_typescript_cache_tree "$ref"; then
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
  validate_typescript_cache_tree "$ref"
  create_archive "$archive" . TypeScript
  gsutil -q cp "$archive" "$cache"

  test -f TypeScript/src/lib/es5.d.ts
}

restore_caches() {
  local cargo_hash node_hash ts_ref ts_deps_hash commit cargo_target_key
  cargo_hash="$(cargo_lock_hash)"
  node_hash="$(scripts_deps_hash)"
  ts_ref="$(typescript_ref)"
  commit="$(commit_key)"

  echo "Cache features for suite '${_TSZ_CI_SUITE:-${TSZ_CI_SUITE:-all}}': $(suite_caches)"

  if suite_has_cache typescript-source; then
    restore_typescript
    ts_deps_hash="$(typescript_deps_hash)"
  else
    echo "Cache restore skipped: TypeScript source (suite does not need TS corpus)"
    ts_deps_hash="not-needed"
  fi

  mkdir -p .ci-cache/cargo-home .ci-cache/npm .target scripts

  if suite_has_cache cargo-home; then
    restore_archive \
      "cargo-home-${cargo_hash}" \
      "$(cache_uri "cargo-home/${cargo_hash}.tar.gz")" \
      ".ci-cache/cargo-home"

    # Per-profile Cargo target caches keyed by
    # (Cargo.lock + Cargo.toml/.cargo/config.toml + rustc version).
    # External dep artifacts are valid across commits (same lock = same
    # versions) and let Cargo skip those crates entirely. Cross-rustc
    # mixing is unsafe so a rustc upgrade rolls the key, and profile
    # config changes also roll the key so a [profile.*] tweak doesn't
    # silently reuse mismatched .rlib metadata.
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

  if suite_has_cache npm; then
    restore_archive \
      "npm-${node_hash}" \
      "$(cache_uri "npm/${node_hash}.tar.gz")" \
      ".ci-cache/npm"
  else
    echo "Cache restore skipped: npm (suite does not run Node tooling)"
  fi

  if suite_has_cache scripts-node-modules; then
    restore_archive \
      "scripts-node-modules-${node_hash}" \
      "$(cache_uri "scripts-node-modules/${node_hash}.tar.gz")" \
      "scripts"
  else
    echo "Cache restore skipped: scripts/node_modules (suite does not run Node tooling)"
  fi

  if suite_has_cache typescript-harness && [[ "${TSZ_CI_SKIP_TS_HARNESS_RESTORE:-0}" != "1" ]]; then
    restore_archive \
      "typescript-harness-${ts_ref}" \
      "$(cache_uri "typescript-harness/${ts_ref}.tar.gz")" \
      "TypeScript"
  else
    echo "Cache restore skipped: typescript-harness"
  fi

  if suite_has_cache typescript-node-modules && [[ "${TSZ_CI_SKIP_TS_HARNESS_RESTORE:-0}" != "1" ]]; then
    restore_archive \
      "typescript-node-modules-${ts_ref}-${ts_deps_hash}" \
      "$(cache_uri "typescript-node-modules/${ts_ref}-${ts_deps_hash}.tar.gz")" \
      "TypeScript" \
      node_modules
  else
    echo "Cache restore skipped: typescript-node-modules"
  fi

  if suite_has_cache dist-fast-commit && [[ "${TSZ_CI_SKIP_DIST_RESTORE:-0}" != "1" && "$commit" != "unknown" ]]; then
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
