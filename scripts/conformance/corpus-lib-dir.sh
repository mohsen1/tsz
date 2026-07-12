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
# TypeScript 7's wrapper package contains the CLI launcher but not the standard
# libraries. Those live in its platform package, so directory existence alone
# is not sufficient: every automatic or explicit candidate must contain both
# `lib.d.ts` and `lib.es5.d.ts`.
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

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
lib_resolver="$script_dir/../setup/resolve-typescript-lib-dir.mjs"

has_compiled_libs() {
    [ -f "$1/lib.d.ts" ] && [ -f "$1/lib.es5.d.ts" ]
}

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
    if [ -d "$TSZ_LIB_DIR" ] && has_compiled_libs "$TSZ_LIB_DIR"; then
        printf '%s\n' "$TSZ_LIB_DIR"
        exit 0
    fi
    echo "corpus-lib-dir.sh: TSZ_LIB_DIR does not contain compiled TypeScript libs: $TSZ_LIB_DIR" >&2
    exit 1
fi

resolver_error=""
wrapper_package="$repo_root/scripts/node_modules/typescript/package.json"
if [ -f "$wrapper_package" ]; then
    resolver_output="$(node "$lib_resolver" "$wrapper_package" 2>&1)"
    resolver_status=$?
    if [ "$resolver_status" -eq 0 ]; then
        printf '%s\n' "$resolver_output"
        exit 0
    fi
    resolver_error="$resolver_output"
fi

for candidate in \
    "$repo_root/TypeScript/built/local" \
    "$repo_root/TypeScript/lib" \
    "$repo_root/scripts/node_modules/typescript/lib"; do
    if [ -d "$candidate" ] && has_compiled_libs "$candidate"; then
        printf '%s\n' "$candidate"
        exit 0
    fi
done

cat >&2 <<EOF
corpus-lib-dir.sh: no pinned-version TypeScript lib directory found under
  $repo_root
Conformance requires the built lib.*.d.ts set used to generate tsc-cache-full.json.
Provide one of (checked in this order):
  - scripts/node_modules/@typescript/typescript-<platform>-<arch>/lib
      (run: cd scripts && npm install; resolved and version-checked automatically)
  - TypeScript/built/local                 (build the pinned submodule)
  - TypeScript/lib
  - scripts/node_modules/typescript/lib    (TypeScript 6 compatibility layout)
or set TSZ_LIB_DIR to that directory explicitly.
EOF
if [ -n "$resolver_error" ]; then
    printf 'Installed TypeScript package error:\n%s\n' "$resolver_error" >&2
fi
exit 1
