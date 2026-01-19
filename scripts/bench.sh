#!/usr/bin/env bash

# Benchmark Script for WASM Compiler (Docker-safe)
#
# This script runs Rust benchmarks inside Docker to prevent memory exhaustion.
# Running cargo bench directly on the host can use 60GB+ RAM and crash the system.
#
# Usage:
#   ./scripts/bench.sh                  # Run all benchmarks
#   ./scripts/bench.sh emitter_bench    # Run specific benchmark

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${GREEN}Running benchmarks in Docker...${NC}"

# Check if Docker is available
if ! command -v docker &> /dev/null; then
    echo -e "${RED}Error: Docker is not installed or not in PATH${NC}"
    echo "Please install Docker to run benchmarks safely."
    exit 1
fi

# Build Docker image if it doesn't exist
IMAGE_NAME="typescript-wasm-bench"
if ! docker image inspect "$IMAGE_NAME" &> /dev/null; then
    echo -e "${YELLOW}Building Docker image...${NC}"
    docker build -t "$IMAGE_NAME" -f "$PROJECT_ROOT/scripts/docker/Dockerfile.bench" "$PROJECT_ROOT"
fi

# Determine which benchmarks to run
BENCH_FILTER="${1:-}"
BENCH_ARGS=""
if [ -n "$BENCH_FILTER" ]; then
    BENCH_ARGS="--bench $BENCH_FILTER"
    echo -e "${YELLOW}Running benchmark: $BENCH_FILTER${NC}"
else
    echo -e "${YELLOW}Running all benchmarks${NC}"
fi

# Run benchmarks in Docker with resource limits
# Limit: 8GB RAM, 4 CPUs (adjust as needed)
docker run --rm \
    --memory="8g" \
    --cpus="4" \
    -v "$PROJECT_ROOT:/workspace" \
    -w /workspace/wasm \
    "$IMAGE_NAME" \
    cargo bench $BENCH_ARGS --color=always

echo -e "${GREEN}✅ Benchmarks complete!${NC}"
echo ""
echo "Benchmark results are saved in: target/criterion/"
echo "To view HTML reports, open: target/criterion/report/index.html"
