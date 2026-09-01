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
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

usage() {
    cat <<'EOF'
Usage: scripts/conformance/oracle.sh <file.ts> [extra tsc flags...]

Runs the manifest-verified pinned TypeScript 7 native oracle
with the same --singleThreaded/--stableTypeOrdering flags the conformance cache
generator uses for TypeScript 7+, so the result matches what
compare-to-parent.sh / conformance.sh actually score.

The shared emit resolver verifies the wrapper package, platform package tree,
native executable, platform, integrity hashes, and exact compiler version.

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
            cat >&2 <<EOF
ERROR: oracle.sh accepts exactly one FILE positional, but also got: $arg

  FILE is appended LAST in the underlying tsc invocation, so a second source
  file is silently reordered ahead of it. A multi-file or file-order claim
  checked this way is NOT verified.

  For multi-file cases invoke the pinned binary directly (order preserved):

    <verified-native-tsc> \\
      --noEmit --pretty false <flags> $FILE $arg

  It is a native binary; 'node <path>' fails on it.
EOF
            exit 2
            ;;
    esac
done

# Package setup is transport/provenance work, never compiler output.  Keep its
# human-readable status on stderr so callers can treat stdout as the exact
# native compiler stream.
if ! "$REPO_ROOT/scripts/setup/ensure-pinned-typescript.sh" "$REPO_ROOT/scripts" >&2; then
    echo "ERROR: verified pinned TypeScript package is unavailable" >&2
    exit 1
fi

if ! ORACLE_JSON="$(node --experimental-strip-types \
    "$REPO_ROOT/scripts/emit/resolve-oracle.mjs" --root "$REPO_ROOT")"; then
    echo "ERROR: pinned native TypeScript oracle verification failed" >&2
    exit 1
fi
TSC_BIN="$(python3 -c \
    'import json,sys; print(json.loads(sys.argv[1])["binaryPath"])' \
    "$ORACLE_JSON")" || exit 1
PINNED_VERSION="$(python3 -c \
    'import json,sys; print(json.loads(sys.argv[1])["provenance"]["version"])' \
    "$ORACLE_JSON")" || exit 1
if [ ! -x "$TSC_BIN" ] || [ "$PINNED_VERSION" != "7.0.2" ]; then
    echo "ERROR: resolver did not return the executable pinned TypeScript 7.0.2 oracle" >&2
    exit 1
fi

EXTRA_FLAGS=(--singleThreaded --stableTypeOrdering true)

echo "# oracle: typescript@$PINNED_VERSION ${EXTRA_FLAGS[*]:-}" >&2
exec "$TSC_BIN" --noEmit --pretty false "${EXTRA_FLAGS[@]}" "$@" "$FILE"
