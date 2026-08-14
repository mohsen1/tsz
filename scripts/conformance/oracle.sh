#!/usr/bin/env bash
# Run the pinned tsc oracle with the exact flags the conformance cache
# generator uses, so a manual spot-check agrees with what
# compare-to-parent.sh/conformance.sh actually score.
#
# Without this, a plain `tsc file.ts` can silently disagree with the cache:
# typescript@7.0.2 (typescript-go) reports different diagnostics for an
# unlabeled/bare block statement depending on --singleThreaded, which
# generate-tsc-cache.rs always passes for TypeScript 7+ (see #16413). This
# wrapper reproduces that exact invocation shape so the two never diverge.
#
# Uses a small dedicated scratch install (not scripts/node_modules) so a
# cold container only fetches the `typescript` package itself, not every
# unrelated dependency in scripts/package.json.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VERSIONS_FILE="$SCRIPT_DIR/typescript-versions.json"
CACHE_DIR="${TSZ_ORACLE_CACHE_DIR:-${TMPDIR:-/tmp}/tsz-oracle}"

usage() {
    cat <<'EOF'
Usage: scripts/conformance/oracle.sh <file.ts> [extra tsc flags...]

Runs the pinned typescript-go oracle (scripts/conformance/typescript-versions.json)
with the same --singleThreaded/--stableTypeOrdering flags the conformance cache
generator uses for TypeScript 7+, so the result matches what
compare-to-parent.sh / conformance.sh actually score.

Installs the pinned TypeScript into a small scratch cache dir on first use
(override with TSZ_ORACLE_CACHE_DIR), separate from scripts/node_modules.

Example:
  scripts/conformance/oracle.sh case.ts --strict --lib es2022 --target es2022
EOF
}

if [ $# -lt 1 ] || [ "$1" == "-h" ] || [ "$1" == "--help" ]; then
    usage
    exit "$([ $# -lt 1 ] && echo 2 || echo 0)"
fi

FILE="$1"
shift

# The exec at the end appends "$FILE" LAST, after "$@". A second source file
# passed as an "extra flag" is therefore silently reordered AHEAD of it, so a
# multi-file invocation does not test the file order it appears to. That
# produced a confidently-wrong "oracle-verified" claim in #17437, caught only
# after it merged (#17481). Reject extra source positionals rather than
# reordering them.
for arg in "$@"; do
    case "$arg" in
        -*) continue ;;
        *.ts | *.tsx | *.mts | *.cts | *.js | *.jsx | *.mjs | *.cjs)
            if [ -f "$arg" ]; then
                cat >&2 <<EOF
ERROR: oracle.sh accepts exactly one FILE positional, but also got: $arg

  FILE is appended LAST in the underlying tsc invocation, so a second source
  file is silently reordered ahead of it. A multi-file or file-order claim
  checked this way is NOT verified.

  For multi-file cases invoke the pinned binary directly (order preserved):

    scripts/node_modules/@typescript/typescript-darwin-arm64/lib/tsc \\
      --noEmit --pretty false <flags> $FILE $arg

  It is a native binary; 'node <path>' fails on it.
EOF
                exit 2
            fi
            ;;
    esac
done

if [ ! -f "$VERSIONS_FILE" ]; then
    echo "ERROR: Missing versions file: $VERSIONS_FILE" >&2
    exit 1
fi

PINNED_VERSION="$(node -e "const fs = require('fs'); const cfg = JSON.parse(fs.readFileSync(process.argv[1], 'utf8')); const current = cfg.current || ''; const mapped = current && cfg.mappings && cfg.mappings[current] && cfg.mappings[current].npm; const fallback = cfg.default && cfg.default.npm; process.stdout.write(mapped || fallback || '');" "$VERSIONS_FILE")"

if [ -z "$PINNED_VERSION" ]; then
    echo "ERROR: Could not resolve pinned TypeScript version from $VERSIONS_FILE" >&2
    exit 1
fi

mkdir -p "$CACHE_DIR"
INSTALLED_VERSION=""
PACKAGE_JSON="$CACHE_DIR/node_modules/typescript/package.json"
if [ -f "$PACKAGE_JSON" ]; then
    INSTALLED_VERSION="$(node -e "try { process.stdout.write(require(process.argv[1]).version); } catch {}" "$PACKAGE_JSON")"
fi

if [ "$INSTALLED_VERSION" != "$PINNED_VERSION" ]; then
    echo "# installing pinned typescript@$PINNED_VERSION into $CACHE_DIR ..." >&2
    (cd "$CACHE_DIR" && npm install --silent --no-audit --no-fund --no-save --no-package-lock "typescript@${PINNED_VERSION}" >&2)
fi

TSC_JS="$CACHE_DIR/node_modules/typescript/lib/tsc.js"
if [ ! -f "$TSC_JS" ]; then
    echo "ERROR: pinned tsc not found at $TSC_JS after install" >&2
    exit 1
fi

TSC_MAJOR="${PINNED_VERSION%%.*}"
EXTRA_FLAGS=()
if [ "$TSC_MAJOR" -ge 7 ] 2>/dev/null; then
    EXTRA_FLAGS+=(--singleThreaded --stableTypeOrdering true)
fi

echo "# oracle: typescript@$PINNED_VERSION ${EXTRA_FLAGS[*]:-}" >&2
exec node "$TSC_JS" --noEmit --pretty false "${EXTRA_FLAGS[@]}" "$@" "$FILE"
