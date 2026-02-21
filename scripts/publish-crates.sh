#!/bin/bash
# Publish tsz crates to crates.io in correct dependency order.
#
# Usage:
#   ./scripts/publish-crates.sh            # publish all crates
#   ./scripts/publish-crates.sh --dry-run  # list files that would be published
#   ./scripts/publish-crates.sh tsz-common # publish a single named crate
#
# Publish order respects the dependency graph:
#   tsz-common -> tsz-scanner -> tsz-parser -> tsz-binder -> tsz-solver
#   -> tsz-lowering -> tsz-emitter -> tsz-checker -> tsz-lsp -> tsz -> tsz-cli

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

CRATES=(
    tsz-common
    tsz-scanner
    tsz-parser
    tsz-binder
    tsz-solver
    tsz-lowering
    tsz-emitter
    tsz-checker
    tsz-lsp
    tsz
    tsz-cli
)

DRY_RUN=0
SINGLE_CRATE=""

for arg in "$@"; do
    case "$arg" in
        --dry-run) DRY_RUN=1 ;;
        --*) echo "Unknown flag: $arg"; exit 1 ;;
        *) SINGLE_CRATE="$arg" ;;
    esac
done

get_version() {
    grep '^version' "$PROJECT_ROOT/Cargo.toml" | head -1 | sed 's/.*"\(.*\)"//'
}

is_already_published() {
    local crate="$1"
    local version="$2"
    local http_status
    http_status=$(curl -s -o /dev/null -w "%{http_code}"         "https://crates.io/api/v1/crates/${crate}/${version}"         -H "User-Agent: tsz-publish-script/1.0")
    [ "$http_status" = "200" ]
}

publish_crate() {
    local crate="$1"
    local version
    version=$(get_version)

    if [ "$DRY_RUN" -eq 1 ]; then
        echo "  [dry-run] cargo package --list --no-verify -p $crate"
        cargo package --list --no-verify -p "$crate"
        return 0
    fi

    if is_already_published "$crate" "$version"; then
        echo "  [skip] $crate@$version already on crates.io"
        return 0
    fi

    echo "  publishing $crate@$version ..."

    # Attempt publish; treat 'already uploaded' as success (race condition)
    local output exit_code=0
    output=$(cargo publish -p "$crate" 2>&1) || exit_code=$?

    if [ "$exit_code" -eq 0 ]; then
        echo "  [ok] $crate@$version published"
    elif echo "$output" | grep -qi 'already uploaded\|already exists'; then
        echo "  [skip] $crate@$version already uploaded (race — OK)"
    else
        echo "$output"
        echo "  [FAIL] $crate@$version publish failed (exit $exit_code)"
        return 1
    fi

    # crates.io needs time to index before dependents can resolve.
    echo "  waiting 25s for crates.io to index $crate ..."
    sleep 25
}

cd "$PROJECT_ROOT"

VERSION=$(get_version)
echo "==> Workspace version: $VERSION"

if [ -n "$SINGLE_CRATE" ]; then
    echo "==> Publishing single crate: $SINGLE_CRATE"
    publish_crate "$SINGLE_CRATE"
else
    echo "==> Publishing ${#CRATES[@]} crates in dependency order..."
    FAILED=0
    for crate in "${CRATES[@]}"; do
        echo ""
        echo "--- $crate ---"
        if ! publish_crate "$crate"; then
            echo "  ERROR: stopping publish chain."
            FAILED=1
            break
        fi
    done
    echo ""
    if [ "$FAILED" -eq 0 ]; then
        echo "==> All crates published successfully!"
    else
        echo "==> Publishing stopped due to failure."
        exit 1
    fi
fi
