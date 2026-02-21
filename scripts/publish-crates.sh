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
#   tsz-common -> tsz-scanner -> tsz-parser -> tsz-binder -> tsz-lowering
#   -> tsz-solver -> tsz-emitter -> tsz-checker -> tsz-lsp -> tsz-cli -> tsz

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
    tsz-lowering
    tsz-solver
    tsz-emitter
    tsz-checker
    tsz-lsp
    tsz-cli
    tsz
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

publish_crate() {
    local crate="$1"
    if [ "$DRY_RUN" -eq 1 ]; then
        # `cargo package --list` shows which files would be included in the
        # published tarball without hitting the crates.io registry — safe to
        # run before any crate in the chain is actually published.
        echo "  [dry-run] cargo package --list --no-verify -p $crate"
        cargo package --list --no-verify -p "$crate"
    else
        echo "  cargo publish -p $crate"
        cargo publish -p "$crate"
        # crates.io needs a moment to index each crate before dependents can
        # reference the new version.
        echo "  Waiting 20 s for crates.io to index $crate ..."
        sleep 20
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
