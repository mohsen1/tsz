#!/bin/bash
# Download TSC cache from GitHub artifacts
#
# Usage: ./scripts/download-tsc-cache.sh [--force]
#
# This script attempts to download the pre-generated TSC cache from GitHub
# artifacts to avoid regenerating it locally.

set -e

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CACHE_FILE="$REPO_ROOT/tsc-cache-full.json"

# Auto-detect GitHub repo from git remote
GITHUB_REPO=$(cd "$REPO_ROOT" && git remote get-url origin 2>/dev/null | sed -E 's|.*github.com[:/]||; s|\.git$||' || echo "")
if [ -z "$GITHUB_REPO" ]; then
    GITHUB_REPO="anthropics/tsz"  # Fallback
fi

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

# Parse args
FORCE=false
for arg in "$@"; do
    case $arg in
        --force)
            FORCE=true
            ;;
    esac
done

# Check if cache already exists
if [ -f "$CACHE_FILE" ] && [ "$FORCE" != "true" ]; then
    echo -e "${YELLOW}Cache already exists: $CACHE_FILE${NC}"
    echo "Use --force to re-download"
    exit 0
fi

# Get TypeScript submodule SHA
cd "$REPO_ROOT"
TS_SHA=$(git rev-parse HEAD:TypeScript 2>/dev/null || echo "")
if [ -z "$TS_SHA" ]; then
    echo -e "${RED}Error: Could not determine TypeScript submodule SHA${NC}"
    echo "Make sure the TypeScript submodule is initialized:"
    echo "  git submodule update --init --depth 1 -- TypeScript"
    exit 1
fi
TS_SHORT="${TS_SHA:0:8}"

echo "TypeScript version: $TS_SHORT ($TS_SHA)"

# Check if gh CLI is available
if ! command -v gh &> /dev/null; then
    echo -e "${YELLOW}GitHub CLI (gh) not found${NC}"
    echo "Install it with: brew install gh"
    echo ""
    echo "Alternatively, download the cache manually from:"
    echo "  https://github.com/$GITHUB_REPO/actions/workflows/tsc-cache.yml"
    exit 1
fi

# Check if jq is available (needed for parsing JSON)
if ! command -v jq &> /dev/null; then
    echo -e "${YELLOW}jq not found${NC}"
    echo "Install it with: brew install jq"
    exit 1
fi

# Check if authenticated
if ! gh auth status &> /dev/null; then
    echo -e "${YELLOW}Not authenticated with GitHub CLI${NC}"
    echo "Run: gh auth login"
    exit 1
fi

echo ""
echo "Searching for cache artifact..."

# Try to download the specific version first
ARTIFACT_NAME="tsc-cache-$TS_SHORT"
echo "Looking for: $ARTIFACT_NAME"

# List available artifacts
ARTIFACT_ID=$(gh api "/repos/$GITHUB_REPO/actions/artifacts" \
    --jq ".artifacts[] | select(.name == \"$ARTIFACT_NAME\" and .expired == false) | .id" \
    2>/dev/null | head -1)

if [ -z "$ARTIFACT_ID" ]; then
    # Fall back to latest
    echo "Specific version not found, trying latest..."
    ARTIFACT_NAME="tsc-cache-latest"
    ARTIFACT_ID=$(gh api "/repos/$GITHUB_REPO/actions/artifacts" \
        --jq ".artifacts[] | select(.name == \"$ARTIFACT_NAME\" and .expired == false) | .id" \
        2>/dev/null | head -1)
fi

if [ -z "$ARTIFACT_ID" ]; then
    echo -e "${RED}No cache artifact found${NC}"
    echo ""
    echo "The cache may not have been generated yet."
    echo "Generate it locally with:"
    echo "  ./scripts/conformance.sh generate --no-cache"
    exit 1
fi

echo "Found artifact: $ARTIFACT_NAME (ID: $ARTIFACT_ID)"
echo "Downloading..."

# Download and extract
TEMP_DIR=$(mktemp -d)
trap "rm -rf $TEMP_DIR" EXIT

gh api "/repos/$GITHUB_REPO/actions/artifacts/$ARTIFACT_ID/zip" > "$TEMP_DIR/cache.zip"
unzip -q "$TEMP_DIR/cache.zip" -d "$TEMP_DIR"

if [ -f "$TEMP_DIR/tsc-cache-full.json" ]; then
    mv "$TEMP_DIR/tsc-cache-full.json" "$CACHE_FILE"
    
    # Show stats
    ENTRIES=$(jq 'length' "$CACHE_FILE" 2>/dev/null || echo "unknown")
    SIZE=$(du -h "$CACHE_FILE" | cut -f1)
    
    echo ""
    echo -e "${GREEN}✓ Cache downloaded successfully${NC}"
    echo "  Location: $CACHE_FILE"
    echo "  Entries: $ENTRIES"
    echo "  Size: $SIZE"
else
    echo -e "${RED}Error: Cache file not found in artifact${NC}"
    exit 1
fi
