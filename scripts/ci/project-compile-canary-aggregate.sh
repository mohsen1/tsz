#!/usr/bin/env bash
#
# Recombine the per-shard project-compile-canary artifacts into the single
# project-compile-canary-logs payload the downstream consumers expect.
#
# The canary job is sharded N-way (see ci.yml project-compile-canary matrix and
# scripts/ci/project-compile-guard.sh run_canary_projects). Each shard runs a
# disjoint, ordered subset of TSZ_COMPILE_GUARD_CANARY_ROWS and uploads its own
# project-compile-canary-logs-<shard> artifact. This script merges those shard
# artifacts back into a single .target/project-compile-guard tree:
#
#   * the per-shard project-compatibility.jsonl files are concatenated (in
#     shard order) into one combined project-compatibility.jsonl, exactly the
#     row set the pre-shard single job produced;
#   * project-compatibility-summary.json is regenerated from the combined jsonl
#     via project-compatibility.mjs summary, so it is byte-shape-identical to
#     the single-job summary (the aggregator is a pure function of the rows);
#   * the remaining log/manifest files from every shard are copied through so
#     the re-uploaded artifact still carries them.
#
# The downloaded shard artifacts live under .canary-shards/<artifact-name>/...
# (actions/download-artifact@v4 with merge-multiple: false).

set -Eeuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

SHARDS_DIR="${TSZ_CANARY_SHARDS_DIR:-$ROOT_DIR/.canary-shards}"
FIXTURE_ROOT="${TSZ_PROJECT_COMPILE_FIXTURE_ROOT:-$ROOT_DIR/.target/project-compile-guard}"
COMBINED_JSONL="$FIXTURE_ROOT/project-compatibility.jsonl"
COMBINED_SUMMARY="$FIXTURE_ROOT/project-compatibility-summary.json"
PROJECT_SET="${TSZ_PROJECT_COMPILE_SET:-canary}"
PROJECT_FILTER="${TSZ_PROJECT_COMPILE_FILTER:-}"
ALLOW_FAILURES="${TSZ_PROJECT_COMPILE_ALLOW_FAILURES:-1}"

mkdir -p "$FIXTURE_ROOT"

if [[ ! -d "$SHARDS_DIR" ]]; then
  echo "error: canary shards directory does not exist: $SHARDS_DIR" >&2
  exit 1
fi

# Discover the downloaded shard artifact directories, sorted so the combined
# jsonl row order is deterministic across runs (shard 0, then 1, then 2 ...).
shard_dirs=()
while IFS= read -r dir; do
  shard_dirs+=("$dir")
done < <(find "$SHARDS_DIR" -mindepth 1 -maxdepth 1 -type d -name 'project-compile-canary-logs-*' | sort)

if [[ "${#shard_dirs[@]}" -eq 0 ]]; then
  echo "error: no project-compile-canary-logs-* shard directories found under $SHARDS_DIR" >&2
  find "$SHARDS_DIR" -maxdepth 2 -print >&2 || true
  exit 1
fi

echo "Found ${#shard_dirs[@]} canary shard artifact directories:"
printf '  %s\n' "${shard_dirs[@]}"

# Copy every shard's files into the combined fixture root first (logs,
# manifests, type-challenges trees, etc.), preserving relative structure. Later
# shards win on path collisions for non-jsonl files; the jsonl/summary are
# rebuilt below so any copied copies of them are intentionally overwritten.
for dir in "${shard_dirs[@]}"; do
  # `cp -R <dir>/. <dest>` merges the shard tree into the fixture root.
  cp -R "$dir/." "$FIXTURE_ROOT/" 2>/dev/null || true
done

# Concatenate the per-shard jsonl rows in shard order into the combined file.
: > "$COMBINED_JSONL"
total_rows=0
for dir in "${shard_dirs[@]}"; do
  shard_jsonl="$dir/project-compatibility.jsonl"
  if [[ -f "$shard_jsonl" ]]; then
    shard_rows="$(grep -c . "$shard_jsonl" 2>/dev/null || echo 0)"
    total_rows=$((total_rows + shard_rows))
    cat "$shard_jsonl" >> "$COMBINED_JSONL"
    echo "  merged ${shard_rows} rows from $(basename "$dir")"
  else
    echo "  warning: shard $(basename "$dir") has no project-compatibility.jsonl" >&2
  fi
done
echo "Combined ${total_rows} canary rows into $COMBINED_JSONL"

# Sum the per-shard summary failure counters so the combined summary's
# `failures` field equals what a single serial run would have reported.
combined_failures=0
for dir in "${shard_dirs[@]}"; do
  shard_summary="$dir/project-compatibility-summary.json"
  if [[ -f "$shard_summary" ]]; then
    shard_failures="$(node -e '
      const fs = require("node:fs");
      try {
        const data = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
        const n = Number(data && data.failures);
        process.stdout.write(String(Number.isFinite(n) ? n : 0));
      } catch {
        process.stdout.write("0");
      }
    ' "$shard_summary" 2>/dev/null || echo 0)"
    combined_failures=$((combined_failures + shard_failures))
  fi
done
echo "Combined canary failure count: ${combined_failures}"

# Regenerate the summary from the combined jsonl. project-compatibility.mjs
# summary is a pure function of the rows + these env knobs, so the output is
# shape-identical to the single-job summary (same keys, same aggregation).
SUMMARY_JSONL_FILE="$COMBINED_JSONL" \
SUMMARY_OUTPUT_FILE="$COMBINED_SUMMARY" \
SUMMARY_OUTPUT_ROOT="$FIXTURE_ROOT" \
SUMMARY_PROJECT_SET="$PROJECT_SET" \
SUMMARY_PROJECT_FILTER="$PROJECT_FILTER" \
SUMMARY_ALLOW_FAILURES="$ALLOW_FAILURES" \
SUMMARY_FAILURES="$combined_failures" \
node scripts/ci/project-compatibility.mjs summary

echo "Wrote combined canary summary to $COMBINED_SUMMARY"
