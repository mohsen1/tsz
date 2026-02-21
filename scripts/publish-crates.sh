#!/bin/bash
# Publish tsz crates to crates.io in correct dependency order.
#
# Usage:
#   ./scripts/publish-crates.sh            # publish all crates
#   ./scripts/publish-crates.sh --dry-run  # list files that would be published for each crate (no registry lookup)
#   ./scripts/publish-crates.sh tsz-common # publish a single named crate
#
# Prerequisites:
#   cargo login   (token from https://crates.io/settings/tokens)
#
# Publish order respects the dependency graph:
#   tsz-common -> tsz-scanner -> tsz-parser -> tsz-binder -> tsz-solver
#   -> tsz-lowering -> tsz-emitter -> tsz-checker -> tsz-lsp -> tsz -> tsz-cli

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Ordered list — do NOT reorder; each crate must be on crates.io before
# the next one can be published (because path deps are rewritten to version deps).
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

is_already_published() {
    local crate="$1"
    local version
    version=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
    # Returns 0 (true) if this exact version already exists on crates.io
    local http_status
    http_status=$(curl -s -o /dev/null -w "%{http_code}" \
        "https://crates.io/api/v1/crates/${crate}/${version}" \
        -H "User-Agent: tsz-publish-script/1.0")
    [ "$http_status" = "200" ]
}

publish_crate() {
    local crate="$1"
    if [ "$DRY_RUN" -eq 1 ]; then
        # `cargo package --list` shows which files would be included in the
        # published tarball without hitting the crates.io registry — safe to
        # run before any crate in the chain is actually published.
        echo "  [dry-run] cargo package --list --no-verify -p $crate"
        cargo package --list --no-verify -p "$crate"
    else
        if is_already_published "$crate"; then
            echo "  [skip] $crate is already published at this version"
        else
            echo "  cargo publish -p $crate"
            cargo publish -p "$crate"
            # crates.io needs a moment to index each crate before dependents can
            # reference the new version.
            echo "  Waiting 20 s for crates.io to index $crate ..."
            sleep 20
        fi
    fi
}

cd "$PROJECT_ROOT"

if [ -n "$SINGLE_CRATE" ]; then
    echo "==> Publishing single crate: $SINGLE_CRATE"
    publish_crate "$SINGLE_CRATE"
else
    echo "==> Publishing all tsz crates in dependency order..."
    for crate in "${CRATES[@]}"; do
        echo ""
        echo "--- $crate ---"
        publish_crate "$crate"
    done
    echo ""
    echo "==> All crates published successfully!"
fi
