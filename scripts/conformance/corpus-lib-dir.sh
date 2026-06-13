#!/bin/bash
# Resolve the corpus lib directory whose built `lib.*.d.ts` files match the
# pinned TypeScript version recorded in
# `scripts/conformance/typescript-versions.json` and used to generate
# `scripts/conformance/tsc-cache-full.json`.
#
# Why this exists (#13400):
# Conformance fingerprints (`file:line:col`) are only stable when tsz
# type-checks against the EXACT same lib set `tsc` used to produce the cache.
# When `TSZ_LIB_DIR` is unset, tsz falls back to `default_lib_dir()`
# auto-discovery, whose candidate priority list is environment-dependent — a
# stray root `node_modules/typescript/lib` at a different TS version can win
# over the pinned corpus libs. That made the SAME commit produce different
# diagnostics locally vs in CI (e.g. tsz resolving `DateTimeFormatPart` locally
# but not in CI). This resolver removes that ambiguity.
#
# It prints the resolved absolute directory to stdout and exits 0, or prints an
# actionable message to stderr and exits 1 when no pinned-version corpus lib
# directory is present. The candidates all carry the built `lib.*.d.ts` layout
# the cache fingerprints expect — `TypeScript/src/lib` is intentionally NOT a
# candidate, because its files lack the `lib.` prefix and would mismatch every
# cached fingerprint. All candidates resolve to the same pinned TS version, so
# whichever exists first yields identical fingerprints; the ordering only fixes
# WHICH directory is chosen so local and CI agree.
#
# Usage:
#   corpus-lib-dir.sh [--repo-root DIR]
#
# A caller-provided `TSZ_LIB_DIR` wins, but it must be an existing directory: a
# stale override would silently reintroduce the divergence this resolver exists
# to prevent, so it is rejected with a clear error instead.

set -u

repo_root=""
while [ $# -gt 0 ]; do
    case "$1" in
        --repo-root)
            repo_root="${2:-}"
            if [ -z "$repo_root" ]; then
                echo "corpus-lib-dir.sh: --repo-root requires a value" >&2
                exit 2
            fi
            shift 2
            ;;
        *)
            echo "corpus-lib-dir.sh: unknown argument '$1'" >&2
            exit 2
            ;;
    esac
done

if [ -z "$repo_root" ]; then
    repo_root="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")/../.." && pwd)"
fi

# Explicit override wins — but only if it actually points at a directory.
if [ -n "${TSZ_LIB_DIR:-}" ]; then
    if [ -d "$TSZ_LIB_DIR" ]; then
        printf '%s\n' "$TSZ_LIB_DIR"
        exit 0
    fi
    echo "corpus-lib-dir.sh: TSZ_LIB_DIR is set but is not a directory: $TSZ_LIB_DIR" >&2
    exit 1
fi

for candidate in \
    "$repo_root/TypeScript/built/local" \
    "$repo_root/TypeScript/lib" \
    "$repo_root/scripts/node_modules/typescript/lib"; do
    if [ -d "$candidate" ]; then
        printf '%s\n' "$candidate"
        exit 0
    fi
done

cat >&2 <<EOF
corpus-lib-dir.sh: no pinned-version TypeScript lib directory found under
  $repo_root
Conformance requires the built lib.*.d.ts set used to generate tsc-cache-full.json.
Provide one of (checked in this order):
  - TypeScript/built/local                 (build the pinned submodule)
  - TypeScript/lib
  - scripts/node_modules/typescript/lib    (run: cd scripts && npm install)
or set TSZ_LIB_DIR to that directory explicitly.
EOF
exit 1
