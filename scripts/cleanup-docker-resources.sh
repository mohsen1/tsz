#!/bin/bash
# Cleanup stale Docker resources for tsz
#
# This script cleans up:
# - Old wasm-target Docker volumes (keeps recent ones)
# - Stopped Docker containers
# - Dangling Docker images
#
# Usage:
#   ./scripts/cleanup-docker-resources.sh          # Dry run
#   ./scripts/cleanup-docker-resources.sh --force  # Actually cleanup
#   ./scripts/cleanup-docker-resources.sh --all    # Remove all wasm-target volumes

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
KEEP_DAYS=7
DRY_RUN=true
CLEAN_ALL=false

# Parse arguments
for arg in "$@"; do
    case $arg in
        --force)
            DRY_RUN=false
            ;;
        --all)
            CLEAN_ALL=true
            ;;
        --days=*)
            KEEP_DAYS="${arg#*=}"
            ;;
        --help)
            echo "Usage: $0 [--force] [--all] [--days=N]"
            echo ""
            echo "Options:"
            echo "  --force     Actually perform cleanup (default: dry run)"
            echo "  --all       Remove ALL wasm-target volumes, not just old ones"
            echo "  --days=N    Keep volumes newer than N days (default: 7)"
            echo ""
            echo "Examples:"
            echo "  $0                    # Show what would be cleaned"
            echo "  $0 --force            # Actually cleanup old volumes"
            echo "  $0 --force --all      # Remove all wasm-target volumes"
            exit 0
            ;;
        *)
            echo "Unknown option: $arg"
            echo "Run '$0 --help' for usage"
            exit 1
            ;;
    esac
done

echo -e "${BLUE}🧹 tsz Docker Resource Cleanup${NC}"
echo ""

# Function to calculate age in days
get_volume_age_days() {
    local created=$1
    # macOS date command
    if date -j -f "%Y-%m-%dT%H:%M:%S" "$created" +%s 2>/dev/null; then
        local created_secs=$(date -j -f "%Y-%m-%dT%H:%M:%S.%f%z" "$created" +%s 2>/dev/null || echo 0)
        local now_secs=$(date +%s)
        echo $(( (now_secs - created_secs) / 86400 ))
    else
        # Linux date command fallback
        local created_secs=$(date -d "$created" +%s 2>/dev/null || echo 0)
        local now_secs=$(date +%s)
        echo $(( (now_secs - created_secs) / 86400 ))
    fi
}

# ============================================================================
# Clean up wasm-target Docker volumes
# ============================================================================

echo -e "${YELLOW}📦 Checking wasm-target Docker volumes...${NC}"

volume_count=0
removed_count=0
total_size=0

docker volume ls -q | grep wasm-target | sort | while read -r vol; do
    volume_count=$((volume_count + 1))

    # Get volume info
    created=$(docker volume inspect "$vol" --format '{{.CreatedAt}}' 2>/dev/null || echo "unknown")
    mountpoint=$(docker volume inspect "$vol" --format '{{.Mountpoint}}' 2>/dev/null || echo "unknown")
    age_days=$(get_volume_age_days "$created")

    # Calculate size if available
    if [ -d "$mountpoint" ]; then
        size=$(du -sh "$mountpoint" 2>/dev/null | cut -f1 || echo "unknown")
    else
        size="unknown"
    fi

    # Determine if we should remove this volume
    should_remove=false

    if [ "$CLEAN_ALL" = true ]; then
        should_remove=true
    elif [ "$age_days" -gt "$KEEP_DAYS" ]; then
        should_remove=true
    fi

    if [ "$should_remove" = true ]; then
        if [ "$DRY_RUN" = true ]; then
            echo -e "  ${RED}Would remove:${NC} $vol"
            echo "    Created: $created (${age_days} days old, ${size})"
        else
            echo -e "  ${GREEN}Removing:${NC} $vol (${age_days} days old, ${size})"
            docker volume rm "$vol" 2>/dev/null || echo -e "    ${RED}Failed to remove${NC}"
        fi
        removed_count=$((removed_count + 1))
    else
        echo -e "  ${GREEN}Keeping:${NC} $vol (${age_days} days old, ${size})"
    fi
done

echo ""
echo -e "${BLUE}Volumes: $volume_count total, $removed_count to remove${NC}"

# ============================================================================
# Clean up stopped containers
# ============================================================================

echo ""
echo -e "${YELLOW}🛑 Checking stopped Docker containers...${NC}"

stopped_count=$(docker ps -a -q --filter "status=exited" | wc -l | tr -d ' ')

if [ "$stopped_count" -gt 0 ]; then
    echo "Found $stopped_count stopped containers"

    if [ "$DRY_RUN" = true ]; then
        echo -e "  ${RED}Would remove $stopped_count stopped containers${NC}"
        docker ps -a --filter "status=exited" --format "table {{.Names}}\t{{.Status}}\t{{.CreatedAt}}" | head -10
    else
        echo -e "  ${GREEN}Removing stopped containers...${NC}"
        docker ps -a -q --filter "status=exited" | xargs -r docker rm -v
    fi
else
    echo -e "  ${GREEN}No stopped containers${NC}"
fi

# ============================================================================
# Clean up dangling images
# ============================================================================

echo ""
echo -e "${YELLOW}🖼️ Checking dangling Docker images...${NC}"

dangling_count=$(docker images -q -f "dangling=true" | wc -l | tr -d ' ')

if [ "$dangling_count" -gt 0 ]; then
    echo "Found $dangling_count dangling images"

    if [ "$DRY_RUN" = true ]; then
        echo -e "  ${RED}Would remove $dangling_count dangling images${NC}"
    else
        echo -e "  ${GREEN}Removing dangling images...${NC}"
        docker images -q -f "dangling=true" | xargs -r docker rmi
    fi
else
    echo -e "  ${GREEN}No dangling images${NC}"
fi

# ============================================================================
# Summary
# ============================================================================

echo ""
echo -e "${BLUE}────────────────────────────────────────${NC}"

if [ "$DRY_RUN" = true ]; then
    echo -e "${YELLOW}This was a DRY RUN${NC}"
    echo ""
    echo "To actually cleanup, run:"
    echo -e "  ${GREEN}./scripts/cleanup-docker-resources.sh --force${NC}"
else
    echo -e "${GREEN}✅ Cleanup complete!${NC}"
fi

echo ""
echo "To see current Docker usage:"
echo "  docker system df"
echo ""
echo "To see all wasm-target volumes:"
echo "  docker volume ls | grep wasm-target"
