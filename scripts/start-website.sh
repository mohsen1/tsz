#!/usr/bin/env bash
set -euo pipefail

# Starts local website preview using Eleventy.
# This prepares benchmark data + WASM + docs sync, then starts local preview.
#
# Usage:
#   ./scripts/start-website.sh
#
# Optional:
#   TSZ_WEBSITE_REAL_BENCH=1 ./scripts/start-website.sh
#   (runs quick real benchmarks before starting, instead of sample chart data)
#   TSZ_WEBSITE_BUILD_WASM=1 ./scripts/start-website.sh
#   (builds wasm package for playground if missing)

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WEBSITE_DIR="$ROOT/crates/tsz-website"
BENCH_JSON="$ROOT/artifacts/bench-vs-tsgo-local-sample.json"

if ! command -v cp >/dev/null 2>&1; then
  echo "error: required system command not found: cp" >&2
  exit 1
fi

if ! command -v npm >/dev/null 2>&1; then
  echo "error: npm is required but not found in PATH" >&2
  exit 1
fi

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

prepare_benchmarks() {
  mkdir -p "$ROOT/artifacts"

  latest="$(ls -t "$ROOT"/artifacts/bench-vs-tsgo-*.json 2>/dev/null | head -n 1 || true)"
  if [ -n "${latest:-}" ] && [ -f "$latest" ]; then
    echo "Benchmarks: using existing artifact $(basename "$latest")"
    return
  fi

  # For local preview, default to a deterministic sample dataset so charts always render.
  # Set TSZ_WEBSITE_REAL_BENCH=1 to run quick real benchmarks before starting the server.
  if [ "${TSZ_WEBSITE_REAL_BENCH:-0}" != "1" ]; then
    echo "Benchmarks: no benchmark artifact found; writing sample dataset for local preview."
    cat > "$BENCH_JSON" <<'JSON'
{
  "generated_at": "local-sample-default",
  "results": [
    { "name": "250 classes", "lines": 250, "kb": 6, "tsz_ms": 41, "tsgo_ms": 66, "winner": "tsz", "factor": 1.6 },
    { "name": "500 generic functions", "lines": 500, "kb": 15, "tsz_ms": 73, "tsgo_ms": 115, "winner": "tsz", "factor": 1.6 },
    { "name": "1000 union members", "lines": 1000, "kb": 24, "tsz_ms": 90, "tsgo_ms": 154, "winner": "tsz", "factor": 1.7 },
    { "name": "Shallow optional object", "lines": 320, "kb": 8, "tsz_ms": 49, "tsgo_ms": 67, "winner": "tsz", "factor": 1.4 },
    { "name": "DeepPartial object", "lines": 780, "kb": 20, "tsz_ms": 108, "tsgo_ms": 137, "winner": "tsz", "factor": 1.3 },
    { "name": "Recursive generic breadth", "lines": 520, "kb": 12, "tsz_ms": 127, "tsgo_ms": 191, "winner": "tsz", "factor": 1.5 },
    { "name": "Conditional dist chain", "lines": 410, "kb": 11, "tsz_ms": 112, "tsgo_ms": 144, "winner": "tsz", "factor": 1.3 },
    { "name": "Mapped type matrix", "lines": 640, "kb": 17, "tsz_ms": 123, "tsgo_ms": 186, "winner": "tsz", "factor": 1.5 },
    { "name": "utility-types/index.d.ts", "lines": 1200, "kb": 29, "tsz_ms": 164, "tsgo_ms": 239, "winner": "tsz", "factor": 1.5 },
    { "name": "ts-toolbelt/sources/Object/Assign.ts", "lines": 1750, "kb": 43, "tsz_ms": 221, "tsgo_ms": 337, "winner": "tsz", "factor": 1.5 },
    { "name": "ts-essentials/lib/deep-partial/index.ts", "lines": 980, "kb": 27, "tsz_ms": 148, "tsgo_ms": 198, "winner": "tsz", "factor": 1.3 },
    { "name": "nextjs", "lines": 58200, "kb": 2100, "tsz_ms": 2380, "tsgo_ms": 3140, "winner": "tsz", "factor": 1.3 }
  ]
}
JSON
    return
  fi

  if ! command -v hyperfine >/dev/null 2>&1; then
    echo "Benchmarks: hyperfine not found; writing sample benchmark dataset for local preview."
    cat > "$BENCH_JSON" <<'JSON'
{
  "generated_at": "local-sample",
  "results": [
    { "name": "250 classes", "lines": 250, "kb": 6, "tsz_ms": 41, "tsgo_ms": 66, "winner": "tsz", "factor": 1.6 },
    { "name": "500 generic functions", "lines": 500, "kb": 15, "tsz_ms": 73, "tsgo_ms": 115, "winner": "tsz", "factor": 1.6 },
    { "name": "1000 union members", "lines": 1000, "kb": 24, "tsz_ms": 90, "tsgo_ms": 154, "winner": "tsz", "factor": 1.7 },
    { "name": "Shallow optional object", "lines": 320, "kb": 8, "tsz_ms": 49, "tsgo_ms": 67, "winner": "tsz", "factor": 1.4 },
    { "name": "DeepPartial object", "lines": 780, "kb": 20, "tsz_ms": 108, "tsgo_ms": 137, "winner": "tsz", "factor": 1.3 },
    { "name": "Recursive generic breadth", "lines": 520, "kb": 12, "tsz_ms": 127, "tsgo_ms": 191, "winner": "tsz", "factor": 1.5 },
    { "name": "Conditional dist chain", "lines": 410, "kb": 11, "tsz_ms": 112, "tsgo_ms": 144, "winner": "tsz", "factor": 1.3 },
    { "name": "Mapped type matrix", "lines": 640, "kb": 17, "tsz_ms": 123, "tsgo_ms": 186, "winner": "tsz", "factor": 1.5 },
    { "name": "utility-types/index.d.ts", "lines": 1200, "kb": 29, "tsz_ms": 164, "tsgo_ms": 239, "winner": "tsz", "factor": 1.5 },
    { "name": "ts-toolbelt/sources/Object/Assign.ts", "lines": 1750, "kb": 43, "tsz_ms": 221, "tsgo_ms": 337, "winner": "tsz", "factor": 1.5 },
    { "name": "ts-essentials/lib/deep-partial/index.ts", "lines": 980, "kb": 27, "tsz_ms": 148, "tsgo_ms": 198, "winner": "tsz", "factor": 1.3 },
    { "name": "nextjs", "lines": 58200, "kb": 2100, "tsz_ms": 2380, "tsgo_ms": 3140, "winner": "tsz", "factor": 1.3 }
  ]
}
JSON
    return
  fi

  echo "Benchmarks: generating quick benchmark data..."
  (
    cd "$ROOT"
    ./scripts/bench/bench-vs-tsgo.sh --quick --json --json-file "$BENCH_JSON"
  ) || {
    echo "Benchmarks: quick generation failed; writing sample benchmark dataset for local preview."
    cat > "$BENCH_JSON" <<'JSON'
{
  "generated_at": "local-sample-after-failure",
  "results": [
    { "name": "250 classes", "lines": 250, "kb": 6, "tsz_ms": 41, "tsgo_ms": 66, "winner": "tsz", "factor": 1.6 },
    { "name": "500 generic functions", "lines": 500, "kb": 15, "tsz_ms": 73, "tsgo_ms": 115, "winner": "tsz", "factor": 1.6 },
    { "name": "1000 union members", "lines": 1000, "kb": 24, "tsz_ms": 90, "tsgo_ms": 154, "winner": "tsz", "factor": 1.7 },
    { "name": "Shallow optional object", "lines": 320, "kb": 8, "tsz_ms": 49, "tsgo_ms": 67, "winner": "tsz", "factor": 1.4 },
    { "name": "DeepPartial object", "lines": 780, "kb": 20, "tsz_ms": 108, "tsgo_ms": 137, "winner": "tsz", "factor": 1.3 },
    { "name": "Recursive generic breadth", "lines": 520, "kb": 12, "tsz_ms": 127, "tsgo_ms": 191, "winner": "tsz", "factor": 1.5 },
    { "name": "Conditional dist chain", "lines": 410, "kb": 11, "tsz_ms": 112, "tsgo_ms": 144, "winner": "tsz", "factor": 1.3 },
    { "name": "Mapped type matrix", "lines": 640, "kb": 17, "tsz_ms": 123, "tsgo_ms": 186, "winner": "tsz", "factor": 1.5 },
    { "name": "utility-types/index.d.ts", "lines": 1200, "kb": 29, "tsz_ms": 164, "tsgo_ms": 239, "winner": "tsz", "factor": 1.5 },
    { "name": "ts-toolbelt/sources/Object/Assign.ts", "lines": 1750, "kb": 43, "tsz_ms": 221, "tsgo_ms": 337, "winner": "tsz", "factor": 1.5 },
    { "name": "ts-essentials/lib/deep-partial/index.ts", "lines": 980, "kb": 27, "tsz_ms": 148, "tsgo_ms": 198, "winner": "tsz", "factor": 1.3 },
    { "name": "nextjs", "lines": 58200, "kb": 2100, "tsz_ms": 2380, "tsgo_ms": 3140, "winner": "tsz", "factor": 1.3 }
  ]
}
JSON
  }
}

prepare_benchmarks
prepare_wasm

cd "$WEBSITE_DIR"

echo "Starting website dev server..."
echo "URL: check Eleventy output below for the selected localhost port."
echo "Press Ctrl+C to stop."

if [ ! -d node_modules ]; then
  echo "Installing website dependencies..."
  npm install
fi

npm run dev
