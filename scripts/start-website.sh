#!/usr/bin/env bash
set -euo pipefail

# Starts local website preview using Eleventy.
# This prepares benchmark data + WASM + docs sync, then starts local preview.
#
# Usage:
#   ./scripts/start-website.sh
#   ./scripts/start-website.sh --prepare-only
#
# Benchmark data priority (first match wins):
#   1. Latest GitHub Pages benchmark snapshot    — refreshed with plain curl
#   2. Latest public GCS benchmark data          — fallback for older deployments
#   3. Existing CI artifact                      — reused when refresh is unavailable
#
# Other env vars:
#   TSZ_WEBSITE_BUILD_WASM=1  — build WASM package for playground if missing
#   TSZ_WEBSITE_BENCH_REFRESH=0 — skip CI benchmark refresh
#   TSZ_WEBSITE_BENCH_URL=https://... — override benchmark data URL

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WEBSITE_DIR="$ROOT/crates/tsz-website"
GITHUB_PAGES_BENCH_URL="https://tsz.dev/benchmark-data/latest.json"
GITHUB_PAGES_FALLBACK_BENCH_URL="https://mohsen1.github.io/tsz/benchmark-data/latest.json"
GCS_BENCH_URL="https://storage.googleapis.com/storage/v1/b/thirdface-ai-oauth_cloudbuild/o/tsz-ci-cache%2Fbench-runs%2Flatest.json?alt=media"

if ! command -v npm >/dev/null 2>&1; then
  echo "error: npm is required but not found in PATH" >&2
  exit 1
fi

try_download_benchmark_url() {
  local label="$1"
  local url="$2"
  local dest="$3"
  local tmp
  tmp="$(mktemp "${TMPDIR:-/tmp}/tsz-bench-latest.XXXXXX")"

  if [ -z "$url" ] || ! command -v curl >/dev/null 2>&1; then
    rm -f "$tmp"
    return 1
  fi

  local http_status
  http_status="$(curl -fsSL -w "%{http_code}" -o "$tmp" "$url" 2>/dev/null || true)"
  if [ "$http_status" = "200" ] && validate_benchmark_json "$tmp"; then
    mv "$tmp" "$dest"
    echo "Benchmarks: got latest CI data from ${label}."
    return 0
  fi

  rm -f "$tmp"
  return 1
}

try_download_latest_benchmarks() {
  if [ "${TSZ_WEBSITE_BENCH_REFRESH:-1}" = "0" ]; then
    return 1
  fi

  echo "Benchmarks: refreshing latest CI benchmark data..."

  local github_dest="$ROOT/artifacts/bench-vs-tsgo-github-latest.json"
  local gcs_dest="$ROOT/artifacts/bench-vs-tsgo-gcs-latest.json"

  try_download_benchmark_url "GitHub Pages" "${TSZ_WEBSITE_BENCH_URL:-$GITHUB_PAGES_BENCH_URL}" "$github_dest" && return 0
  try_download_benchmark_url "GitHub Pages fallback" "$GITHUB_PAGES_FALLBACK_BENCH_URL" "$github_dest" && return 0
  try_download_benchmark_url "public GCS" "$GCS_BENCH_URL" "$gcs_dest" && return 0

  return 1
}

validate_benchmark_json() {
  node -e '
    const fs = require("node:fs");
    const file = process.argv[1];
    let data;
    try {
      data = JSON.parse(fs.readFileSync(file, "utf8"));
    } catch {
      process.exit(1);
    }
    const count = Array.isArray(data.results)
      ? data.results.filter((row) => Number(row?.tsz_ms) > 0 && Number(row?.tsgo_ms) > 0).length
      : 0;
    process.exit(count > 0 ? 0 : 1);
  ' "$1"
}

prepare_benchmarks() {
  mkdir -p "$ROOT/artifacts"
  rm -f "$ROOT/artifacts/bench-vs-tsgo-local-sample.json" "$ROOT/artifacts/bench-vs-tsgo-local.json"

  # 1. Pull latest CI benchmark data first, so local preview does not stay stale.
  if try_download_latest_benchmarks; then
    return
  fi

  # 2. Reuse only CI-backed artifacts already on disk.
  for ci_latest in \
    "$ROOT/artifacts/bench-vs-tsgo-github-latest.json" \
    "$ROOT/artifacts/bench-vs-tsgo-gcs-latest.json" \
    "$ROOT/artifacts/bench-vs-tsgo-latest.json"; do
    if [ -f "$ci_latest" ] && validate_benchmark_json "$ci_latest"; then
      echo "Benchmarks: using existing CI artifact $(basename "$ci_latest")"
      return
    fi
  done

  echo "Benchmarks: no CI benchmark data available; charts will stay empty."
  echo "  (expected artifact: artifacts/bench-vs-tsgo-github-latest.json)"
}

prepare_wasm() {
  if ! command -v wasm-pack >/dev/null 2>&1; then
    echo "error: wasm-pack is required to build playground WASM." >&2
    echo "Install: curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh" >&2
    exit 1
  fi

  echo "WASM: building web target for playground..."
  cp "$ROOT/LICENSE.txt" "$ROOT/crates/tsz-wasm/LICENSE.txt"
  (
    cd "$ROOT"
    # Keep local preview aligned with production: wasm-opt currently breaks
    # the browser-facing build during wasm-bindgen externref table init.
    wasm-pack build crates/tsz-wasm --target web --out-dir ../../pkg/web --no-opt
  )
}

prepare_benchmarks

if [ "${1:-}" = "--prepare-only" ]; then
  exit 0
fi

if [ "${TSZ_WEBSITE_BUILD_WASM:-0}" = "1" ]; then
  prepare_wasm
fi

cd "$WEBSITE_DIR"

echo "Starting website dev server..."
echo "URL: check Eleventy output below for the selected localhost port."
echo "Press Ctrl+C to stop."

if [ ! -d node_modules ]; then
  echo "Installing website dependencies..."
  npm install
fi

TSZ_WEBSITE_SKIP_BENCH_PREPARE=1 npm run dev
