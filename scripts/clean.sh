#!/usr/bin/env bash
# Cargo build artifact cleanup script
# Helps prevent disk from filling up with .target directory growth

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
TARGET_DIR="$PROJECT_ROOT/target"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Function to format bytes to human-readable
format_bytes() {
    local bytes=$1
    if [ "$bytes" -lt 1024 ]; then
        echo "${bytes}B"
    elif [ "$bytes" -lt 1048576 ]; then
        echo "$((bytes / 1024))K"
    elif [ "$bytes" -lt 1073741824 ]; then
        echo "$((bytes / 1048576))M"
    else
        echo "$((bytes / 1073741824))G"
    fi
}

# Function to get directory size
get_dir_size() {
    local dir=$1
    if [ -d "$dir" ]; then
        du -sk "$dir" 2>/dev/null | awk '{print $1 * 1024}'
    else
        echo "0"
    fi
}

# Function to check disk space
check_disk_space() {
    echo -e "${BLUE}Disk Space:${NC}"
    df -h "$PROJECT_ROOT" | awk 'NR==2 {
        printf "  Available: %s\n  Used: %s / %s (%s)\n", $4, $3, $2, $5
    }'
    echo ""
}

# Function to show target directory breakdown
show_target_breakdown() {
    if [ ! -d "$TARGET_DIR" ]; then
        echo -e "${YELLOW}No target directory found${NC}"
        return
    fi

    echo -e "${BLUE}Target Directory Breakdown:${NC}"

    local total_size=$(get_dir_size "$TARGET_DIR")
    echo "  Total: $(format_bytes $total_size)"

    # Show size of each subdirectory
    for subdir in "$TARGET_DIR"/*/; do
        if [ -d "$subdir" ]; then
            local name=$(basename "$subdir")
            local size=$(get_dir_size "$subdir")
            if [ "$size" -gt 0 ]; then
                echo "    $name: $(format_bytes $size)"
            fi        fi
    done

    # Show deps directory size if it exists
    if [ -d "$TARGET_DIR/release/deps" ]; then
        local deps_size=$(get_dir_size "$TARGET_DIR/release/deps")
        echo "    release/deps: $(format_bytes $deps_size)"
    fi

    echo ""
}

# Function to check if cleanup is needed
check_cleanup_needed() {
    local total_size=$(get_dir_size "$TARGET_DIR")
    local total_gb=$((total_size / 1073741824))

    # Get available disk space in KB
    local available_kb=$(df "$PROJECT_ROOT" | awk 'NR==2 {print $4}')
    local available_gb=$((available_kb / 1048576))

    echo -e "${BLUE}Cleanup Recommendations:${NC}"

    if [ "$total_gb" -gt 2 ]; then
        echo -e "  ${YELLOW}⚠️  Target directory is large (${total_gb}G)${NC}"
        echo "     Consider: $0 --safe"
    fi

    if [ "$available_gb" -lt 5 ]; then
        echo -e "  ${RED}⚠️  LOW DISK SPACE (${available_gb}G available)${NC}"
        echo "     Run: $0 --full"
    elif [ "$available_gb" -lt 10 ]; then
        echo -e "  ${YELLOW}⚠️  Disk space getting low (${available_gb}G available)${NC}"
        echo "     Consider: $0 --safe"
    else
        echo -e "  ${GREEN}✓ Disk space OK (${available_gb}G available)${NC}"
    fi

    echo ""
}

# Show usage
usage() {
    echo "Usage: $0 [OPTION]"
    echo ""
    echo "Cargo build artifact cleanup script - prevents target directory from filling disk"
    echo ""
    echo "Options:"
    echo "  --check    Check disk space and target sizes (no cleanup)"
    echo "  --safe     Remove release artifacts only (keeps debug builds for faster iteration)"
    echo "  --full     Full clean (removes all build artifacts)"
    echo "  --help     Show this help message"
    echo ""
    echo "Examples:"
    echo "  $0 --check   # Check what needs cleanup"
    echo "  $0 --safe    # Safe cleanup (release only)"
    echo "  $0 --full    # Full cleanup"
    exit 0
}

# Parse arguments
MODE="check"
for arg in "$@"; do
    case $arg in
        --check)
            MODE="check"
            shift
            ;;
        --safe)
            MODE="safe"
            shift
            ;;
        --full)
            MODE="full"
            shift
            ;;
        --help|-h)
            usage
            ;;
        *)
            echo "Unknown option: $arg"
            usage
            ;;
    esac
done

# Main script logic
cd "$PROJECT_ROOT"

echo -e "${BLUE}=== Cargo Cleanup Script ===${NC}"
echo ""

# Show current state
check_disk_space
show_target_breakdown

# Perform cleanup based on mode
case $MODE in
    check)
        check_cleanup_needed
        echo -e "${GREEN}No cleanup performed (check mode)${NC}"
        ;;

    safe)
        echo -e "${YELLOW}Performing safe cleanup (release artifacts only)...${NC}"
        echo ""

        # Remove release directory
        if [ -d "$TARGET_DIR/release" ]; then
            before=$(get_dir_size "$TARGET_DIR")
            cargo clean --release
            after=$(get_dir_size "$TARGET_DIR")
            freed=$((before - after))

            echo -e "${GREEN}✓ Removed release artifacts${NC}"
            echo "  Freed: $(format_bytes $freed)"
        else
            echo -e "${YELLOW}No release artifacts found${NC}"
        fi

        # Clean nextest cache
        if [ -d "$TARGET_DIR/nextest" ]; then
            rm -rf "$TARGET_DIR/nextest"
            echo -e "${GREEN}✓ Removed nextest cache${NC}"
        fi

        echo ""
        check_disk_space
        show_target_breakdown
        ;;

    full)
        echo -e "${YELLOW}Performing full cleanup...${NC}"
        echo ""

        before=$(get_dir_size "$TARGET_DIR")
        cargo clean
        after=$(get_dir_size "$TARGET_DIR")
        freed=$((before - after))

        echo -e "${GREEN}✓ Full clean complete${NC}"
        echo "  Freed: $(format_bytes $freed)"
        echo ""

        check_disk_space
        show_target_breakdown
        ;;
esac

echo ""
echo -e "${GREEN}=== Done ===${NC}"
