#!/bin/bash
# Conformance Test Runner
# Usage: ./scripts/conformance/conformance.sh [generate|run|all] [options]

set -euo pipefail

# Get the repository root directory
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

# Default values (relative to repo root)
TEST_DIR="$REPO_ROOT/TypeScript/tests/cases"
CACHE_FILE="$REPO_ROOT/scripts/conformance/tsc-cache-full.json"
DOMAIN_FILE="$REPO_ROOT/scripts/conformance/conformance-domain.json"

# Build profile (dist-fast = fast build + good runtime perf)
BUILD_PROFILE="dist-fast"
TARGET_DIR="$REPO_ROOT/.target"

# Binary paths (will be updated based on profile)
TSZ_BIN="$REPO_ROOT/.target/dist-fast/tsz"
SERVER_BIN="$REPO_ROOT/.target/dist-fast/tsz-server"
CACHE_GEN_BIN="$REPO_ROOT/.target/dist-fast/generate-tsc-cache"
RUNNER_BIN="$REPO_ROOT/.target/dist-fast/tsz-conformance"
BUILD_MANIFEST="$REPO_ROOT/.target/dist-fast/conformance-build-manifest.json"

WORKERS=16

# TSZ_LIB_DIR is derived from the same verified pinned native-oracle package
# used to generate the cache. Ambient overrides and alternate lib trees are not
# canonical inputs.

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

print_help() {
    cat << EOF
${YELLOW}TSZ Conformance Test Runner${NC}

Usage: ./scripts/conformance/conformance.sh [COMMAND] [OPTIONS]

Commands:
  generate    Generate TSC cache locally (if not checked in)
  run         Run conformance tests against TSC cache (auto-diffs vs baseline)
  analyze     Analyze snapshot offline: root-cause campaigns, quick wins, code families
  render-corpus
              Classify diagnostic render/fingerprint failures from the last snapshot
              and optional --print-fingerprints runner output
  areas       Analyze pass/fail rates by test directory area
  diff        Show regressions/improvements vs last snapshot baseline
  all         Generate cache (if needed) and run tests (default)
  snapshot    Run tests + analyze + areas, save structured results to
              scripts/conformance/conformance-snapshot.json, per-test detail to
              scripts/conformance/conformance-detail.json, and per-test baseline
  clean       Remove cache file

Run options:
  --verbose         Show file bodies, fingerprint deltas, and expected/actual for failures
  --filter PAT      Filter test files by pattern
  --max N           Maximum number of tests to run (default: all)
  --offset N        Skip first N tests (default: 0)
  --shard I/N       Run one round-robin shard after sorting and filtering
  --plan N          Emit JSON shard plan for N shards and exit
  --workers N       Number of parallel workers (default: 16)
  --profile NAME    Cargo build profile (default: dist-fast)
  --test-dir PATH   Override TypeScript test corpus path
  --no-cache        Force cache regeneration even if cache exists
  --force           Override snapshot regression guards (never provenance/coverage guards)

Analyze options:
  --campaigns       Show recommended root-cause campaigns
  --campaign NAME   Show one campaign in detail
  --category CAT    Legacy alias: false-positive, close, one-missing, one-extra, campaigns
  --one-missing     Show tests fixable by adding one missing code
  --one-extra       Show tests fixable by removing one extra code
  --false-positives Show codes/tests emitted incorrectly
  --code TSXXXX     Show tests involving a specific diagnostic code
  --extra-code TSX  Show tests where a code is emitted as extra
  --close N         Show tests within diff <= N of passing
  --paths-only      Output only test paths for code queries
  --top N           Show top N rows in detailed views (default: 20)

Render-corpus options:
  --fingerprint-log PATH
                    Verbose run output from --print-fingerprints
  --category CAT    Filter render buckets, e.g. location-only, under-count,
                    over-count, message-only
  --code TSXXXX     Filter render records by diagnostic code
  --paths-only      Output only matching test paths

Areas options:
  --depth N         Grouping depth: 1=top-level, 2=sub-areas (default: 1)
  --min-tests N     Minimum tests in area to display (default: 5)
  --drilldown AREA  Drill into a specific area (e.g., "types", "statements")

Examples:
  ./scripts/conformance/conformance.sh run --max 100              # Quick smoke test
  ./scripts/conformance/conformance.sh run --max 20 --verbose     # Verbose with file bodies
  ./scripts/conformance/conformance.sh run --filter "strict"      # Run tests matching "strict"
  ./scripts/conformance/conformance.sh analyze                    # Offline strategy overview
  ./scripts/conformance/conformance.sh analyze --campaigns        # Ranked root-cause campaigns
  ./scripts/conformance/conformance.sh analyze --campaign big3    # Deep dive one campaign
  ./scripts/conformance/conformance.sh render-corpus              # Render failure buckets
  ./scripts/conformance/conformance.sh render-corpus --category location-only --paths-only
  ./scripts/conformance/conformance.sh areas --depth 2            # Sub-area breakdown

Note: Fingerprint comparison (code + location + message) is always enabled.
      Binaries are automatically built if not found.
      Cache/domain: scripts/conformance/tsc-cache-full.json and
                    scripts/conformance/conformance-domain.json
      Offline analysis reads scripts/conformance/conformance-detail.json from the last snapshot.
EOF
}

binaries_are_fresh() {
    python3 "$REPO_ROOT/scripts/conformance/build-manifest.py" verify \
        --repo "$REPO_ROOT" \
        --manifest "$BUILD_MANIFEST" \
        --binary "tsz=$TSZ_BIN" \
        --binary "tsz-server=$SERVER_BIN" \
        --binary "generate-tsc-cache=$CACHE_GEN_BIN" \
        --binary "tsz-conformance=$RUNNER_BIN"
}

# Build binaries if source has changed (cargo handles incremental compilation)
ensure_binaries() {
    export RUST_BACKTRACE=1

    # Fast path: check if binaries are already fresh
    if binaries_are_fresh; then
        echo -e "${GREEN}Binaries are up-to-date (profile: $BUILD_PROFILE)${NC}"
        return 0
    fi
    
    echo -e "${YELLOW}Building tsz and conformance runner (profile: $BUILD_PROFILE)...${NC}"
    cd "$REPO_ROOT"
    
    # For dev profile, optimize for fast build (link time not important)
    # For release/dist, LTO is already configured in Cargo.toml
    # NOTE: On macOS, ThinLTO + incremental can intermittently fail at link-time
    # with undefined llvm internal symbols. Disable incremental for dist profiles
    # in this script to keep conformance runs stable.
    local cargo_incremental="${CARGO_INCREMENTAL:-1}"
    if [[ "$BUILD_PROFILE" == "dist" || "$BUILD_PROFILE" == "dist-fast" ]]; then
        cargo_incremental="0"
    fi
    CARGO_TARGET_DIR="$TARGET_DIR" \
    CARGO_INCREMENTAL="$cargo_incremental" \
    cargo build --target-dir "$TARGET_DIR" --profile "$BUILD_PROFILE" -p tsz-cli -p tsz-conformance

    python3 "$REPO_ROOT/scripts/conformance/build-manifest.py" write \
        --repo "$REPO_ROOT" \
        --manifest "$BUILD_MANIFEST" \
        --binary "tsz=$TSZ_BIN" \
        --binary "tsz-server=$SERVER_BIN" \
        --binary "generate-tsc-cache=$CACHE_GEN_BIN" \
        --binary "tsz-conformance=$RUNNER_BIN"
    binaries_are_fresh
    
    echo ""
}

# Ensure the exact pinned compiler and its platform standard libraries exist.
ensure_scripts_deps() {
    if ! "$REPO_ROOT/scripts/setup/ensure-pinned-typescript.sh" "$REPO_ROOT/scripts"; then
        echo -e "${YELLOW}Pinned TypeScript compiler or standard libraries are unavailable.${NC}" >&2
        exit 1
    fi
}

resolve_tsz_lib_dir() {
    ensure_scripts_deps
    local oracle_json
    local resolved_lib_dir
    oracle_json="$(node --experimental-strip-types \
        "$REPO_ROOT/scripts/emit/resolve-oracle.mjs" --root "$REPO_ROOT")" \
        || { echo "error: conformance could not verify the pinned native TypeScript oracle." >&2; return 1; }
    resolved_lib_dir="$(python3 -c \
        'import json,pathlib,sys; print(pathlib.Path(json.loads(sys.argv[1])["binaryPath"]).resolve(strict=True).parent)' \
        "$oracle_json")" \
        || { echo "error: verified TypeScript oracle returned no usable library directory." >&2; return 1; }
    if [ ! -f "$resolved_lib_dir/lib.d.ts" ] || [ ! -f "$resolved_lib_dir/lib.es5.d.ts" ]; then
        echo "error: verified native TypeScript package has no complete standard-library tree: $resolved_lib_dir" >&2
        return 1
    fi
    export TSZ_LIB_DIR="$resolved_lib_dir"
    echo "Lib dir: ${TSZ_LIB_DIR}"
}

generate_cache() {
    local force_regenerate="${1:-false}"

    # Ensure scripts dependencies (TypeScript + emit runner deps) are installed
    ensure_scripts_deps

    if [ "$force_regenerate" != "true" ] && [ -f "$CACHE_FILE" ] && [ -f "$DOMAIN_FILE" ]; then
        echo -e "${YELLOW}Cache and domain already exist:${NC}"
        echo "  $CACHE_FILE"
        echo "  $DOMAIN_FILE"
        echo "Skipping cache/domain generation."
        echo ""
        return
    fi

    if [ "$force_regenerate" = "true" ] && [ -f "$CACHE_FILE" ]; then
        echo -e "${YELLOW}Cache exists but --no-cache flag set, regenerating...${NC}"
        echo ""
    fi

    # Always use the Rust cache generator (spawns tsc --project per test).
    # This matches the runner's invocation method exactly, ensuring tsc-vs-tsc = 100%.
    # The binary auto-caps concurrent node processes at min(workers, 8) to avoid OOM.
    echo -e "${GREEN}Generating TSC cache (tsc --project per test)...${NC}"
    echo "Test directory: $TEST_DIR"
    echo "Workers: $WORKERS"
    echo ""

    cd "$REPO_ROOT"
    $CACHE_GEN_BIN \
        --repo-root "$REPO_ROOT" \
        --test-dir "$TEST_DIR" \
        --output "$CACHE_FILE" \
        --domain-output "$DOMAIN_FILE" \
        --workers "$WORKERS"

    python3 "$REPO_ROOT/scripts/conformance/validate-cache-domain.py" \
        --cache "$CACHE_FILE" \
        --domain "$DOMAIN_FILE" \
        --versions "$REPO_ROOT/scripts/conformance/typescript-versions.json"

    echo ""
    echo -e "${GREEN}Cache generated: $CACHE_FILE${NC}"
    echo -e "${GREEN}Domain generated: $DOMAIN_FILE${NC}"
}

# Ensure cache exists - generate if not checked in
ensure_cache() {
    if [ ! -f "$CACHE_FILE" ]; then
        echo -e "${YELLOW}Cache not found, generating locally (this may take 10-15 minutes)...${NC}"
        ensure_binaries
        generate_cache
        return
    fi

    local pinned_version=""
    if ! pinned_version="$(node -e "const fs = require('fs'); const cfg = JSON.parse(fs.readFileSync('$REPO_ROOT/scripts/conformance/typescript-versions.json', 'utf8')); const current = cfg.current; const mapping = current && cfg.mappings && cfg.mappings[current] && cfg.mappings[current].npm; process.stdout.write(mapping || '');")"; then
        echo -e "${YELLOW}ERROR: Failed to read pinned TypeScript version from scripts/conformance/typescript-versions.json${NC}" >&2
        return 1
    fi
    if [ -z "$pinned_version" ]; then
        echo -e "${YELLOW}ERROR: Could not resolve pinned TypeScript version from scripts/conformance/typescript-versions.json${NC}" >&2
        return 1
    fi

    local cache_report=""
    if ! cache_report="$(node - "$CACHE_FILE" "$pinned_version" <<'EOF'
const fs = require('fs');
const cachePath = process.argv[2];
const pinnedVersion = process.argv[3];
const cache = JSON.parse(fs.readFileSync(cachePath, 'utf8'));

let missing = 0;
let mismatch = 0;
let samplePath = '';
let sampleVersion = '';
let checked = 0;
let incompleteDiagnosticEvidence = 0;
let evidenceSample = '';

for (const [path, entry] of Object.entries(cache)) {
  checked += 1;
  const exits = entry && entry.ordinary_exit_statuses;
  if (
    !entry ||
    entry.diagnostic_blocks_complete !== true ||
    !Array.isArray(exits) ||
    exits.length === 0 ||
    exits.some(status => !Number.isInteger(status) || status < 0 || status > 2)
  ) {
    incompleteDiagnosticEvidence += 1;
    if (!evidenceSample) evidenceSample = path;
  }
  const actual = entry && entry.metadata && entry.metadata.typescript_version;
  if (!actual) {
    missing += 1;
    if (!samplePath) {
      samplePath = path;
      sampleVersion = '<missing>';
    }
    continue;
  }
  if (actual !== pinnedVersion) {
    mismatch += 1;
    if (!samplePath) {
      samplePath = path;
      sampleVersion = actual;
    }
  }
}

if (checked === 0) {
  console.log('EMPTY');
  process.exit(1);
}

if (missing > 0 || mismatch > 0) {
  console.log(`VERSION:missing=${missing},mismatch=${mismatch},sample=${samplePath},sampleVersion=${sampleVersion}`);
  process.exit(1);
}

if (incompleteDiagnosticEvidence > 0) {
  console.log(`EVIDENCE:incomplete=${incompleteDiagnosticEvidence},sample=${evidenceSample}`);
  process.exit(1);
}

console.log('ok');
process.exit(0);
EOF
)"; then
        # Non-zero exit here means cache metadata is missing/mismatched or cache is invalid.
        # Preserve cache_report for actionable diagnostics below.
        :
    fi

    if [ "$cache_report" != "ok" ]; then
        if [[ "$cache_report" == EVIDENCE:* ]]; then
            echo -e "${YELLOW}ERROR: TypeScript cache lacks complete grouped diagnostic-block evidence:${NC}" >&2
            echo "  Cache check: $cache_report" >&2
            echo -e "${YELLOW}Regenerate once with: scripts/safe-run.sh --limit 75% -- ./scripts/conformance/conformance.sh generate --no-cache --workers $WORKERS${NC}" >&2
        else
            echo -e "${YELLOW}ERROR: TypeScript cache does not match the pinned TypeScript version:${NC}" >&2
            echo "  Pinned version: $pinned_version" >&2
            echo "  Cache check: ${cache_report:-unknown}" >&2
            echo -e "${YELLOW}Re-run with --no-cache to regenerate cache, or update the cache file to match pinned tsc.${NC}" >&2
        fi
        return 1
    fi

    if ! python3 "$REPO_ROOT/scripts/conformance/validate-cache-domain.py" \
        --cache "$CACHE_FILE" \
        --domain "$DOMAIN_FILE" \
        --versions "$REPO_ROOT/scripts/conformance/typescript-versions.json"; then
        echo -e "${YELLOW}ERROR: TypeScript cache/domain validation failed.${NC}" >&2
        return 1
    fi

    echo -e "${GREEN}TypeScript cache version matches pinned version: $pinned_version${NC}"
    return 0
}

run_tests() {
    # Pin TSZ to the exact native-oracle library tree for every invocation;
    # ambient TSZ_LIB_DIR values never select canonical inputs.
    resolve_tsz_lib_dir

    echo -e "${GREEN}Running conformance tests...${NC}"
    echo "Cache file: $CACHE_FILE"
    echo "Workers: $WORKERS"
    echo ""

    cd "$REPO_ROOT"
    # Filter out flags already handled at the top level
    local extra_args=()
    local verbose=false
    local skip_next=false
    for arg in "$@"; do
        if [ "$skip_next" = true ]; then
            skip_next=false
            continue
        fi
        if [ "$arg" = "--workers" ]; then
            skip_next=true
            continue
        fi
        if [[ "$arg" == --workers=* ]]; then
            continue
        fi
        if [ "$arg" = "--no-cache" ]; then
            continue
        fi
        if [[ "$arg" == --verbose ]]; then
            verbose=true
            # Don't add --verbose here; we build the runner flags below
            continue
        fi
        extra_args+=("$arg")
    done

    # Build runner flags based on mode
    #   quiet (default): FAIL lines + summary only
    #   verbose: FAIL lines with expected/actual, file bodies, fingerprint deltas
    local runner_flags=()
    if [ "$verbose" = true ]; then
        runner_flags+=(--print-test-files --print-fingerprints --verbose)
    fi

    # Always capture per-test results for diffing against baseline.
    # Use --print-test and tee to both show output and save results.
    local last_run="$REPO_ROOT/scripts/conformance/conformance-last-run.txt"
    local tmpout
    tmpout=$(mktemp)
    # Run with --print-test to get PASS/FAIL per test line. Preserve the
    # runner's status across tee so fatal/incomplete runs cannot become green.
    local runner_status=0
    set +e
    $RUNNER_BIN \
        --test-dir "$TEST_DIR" \
        --cache-file "$CACHE_FILE" \
        --domain-file "$DOMAIN_FILE" \
        --tsz-binary "$TSZ_BIN" \
        --workers $WORKERS \
        --print-test \
        "${runner_flags[@]+"${runner_flags[@]}"}" \
        "${extra_args[@]+"${extra_args[@]}"}" 2>&1 | tee "$tmpout"
    runner_status=${PIPESTATUS[0]}
    set -e

    if ! python3 "$REPO_ROOT/scripts/conformance/validate-runner-output.py" \
        "$tmpout" --runner-status "$runner_status" >/dev/null; then
        rm -f "$tmpout"
        echo "ERROR: conformance runner output failed canonical accounting validation" >&2
        return 1
    fi

    # Never overwrite the last complete observation with an extraction failure.
    local last_run_tmp
    last_run_tmp=$(mktemp)
    if ! python3 "$REPO_ROOT/scripts/conformance/extract-baseline.py" "$tmpout" > "$last_run_tmp"; then
        rm -f "$tmpout" "$last_run_tmp"
        echo "ERROR: failed to extract conformance baseline" >&2
        return 1
    fi
    mv "$last_run_tmp" "$last_run"
    rm -f "$tmpout"

    # Auto-diff against baseline if it exists and this was an unfiltered run
    local baseline="$REPO_ROOT/scripts/conformance/conformance-baseline.txt"
    local has_filter=false
    for arg in "${extra_args[@]+"${extra_args[@]}"}"; do
        if [[ "$arg" == "--filter" ]] || [[ "$arg" == --filter=* ]]; then
            has_filter=true
            break
        fi
    done

    if [ "$has_filter" = false ] && [ -f "$baseline" ] && [ -s "$last_run" ]; then
        echo ""
        diff_results "$baseline" "$last_run"
    fi
    return "$runner_status"
}

analyze_tests() {
    echo -e "${GREEN}Analyzing saved conformance snapshot...${NC}"
    echo "Source: scripts/conformance/conformance-detail.json"
    echo "Method: root-cause campaigns first, quick wins second"
    echo ""

    cd "$REPO_ROOT"
    python3 "$REPO_ROOT/scripts/conformance/query-conformance.py" "$@"
}

areas_analysis() {
    resolve_tsz_lib_dir
    local depth=""
    local min_tests=""
    local drilldown=""
    local extra_args=()

    # Parse areas-specific args
    local args=("$@")
    local i=0
    while [ $i -lt ${#args[@]} ]; do
        case "${args[$i]}" in
            --depth)
                i=$((i + 1))
                depth="${args[$i]}"
                ;;
            --min-tests)
                i=$((i + 1))
                min_tests="${args[$i]}"
                ;;
            --drilldown)
                i=$((i + 1))
                drilldown="${args[$i]}"
                ;;
            *)
                extra_args+=("${args[$i]}")
                ;;
        esac
        i=$((i + 1))
    done

    echo -e "${GREEN}Running conformance tests for area analysis...${NC}"

    cd "$REPO_ROOT"

    # Run with --print-test to get PASS/FAIL per test
    local tmpfile
    tmpfile=$(mktemp)
    trap "rm -f '$tmpfile'" EXIT
    $RUNNER_BIN \
        --test-dir "$TEST_DIR" \
        --cache-file "$CACHE_FILE" \
        --domain-file "$DOMAIN_FILE" \
        --tsz-binary "$TSZ_BIN" \
        --workers $WORKERS \
        --print-test \
        "${extra_args[@]+"${extra_args[@]}"}" > "$tmpfile" 2>&1

    # Use python to analyze by area
    python3 "$REPO_ROOT/scripts/conformance/analyze-conformance-areas.py" "$tmpfile" \
        ${depth:+--depth "$depth"} \
        ${min_tests:+--min-tests "$min_tests"} \
        ${drilldown:+--drilldown "$drilldown"}
}

diff_results() {
    # Compare two per-test result files and show regressions/improvements.
    # Usage: diff_results <baseline_file> <current_file>
    # Format: "PASS path" or "FAIL path | expected:[...] actual:[...]"
    local baseline_file="$1"
    local current_file="$2"

    python3 -c "
import sys

def parse_result_file(path):
    \"\"\"Parse a result file into {test_path: status} dict.
    Handles PASS/FAIL/XFAIL/CRASH/TIMEOUT rows and detailed failure rows.
    Every terminal non-pass counts as FAIL for regression math.\"\"\"
    results = {}
    with open(path) as f:
        for line in f:
            line = line.strip()
            parts = line.split(' ', 1)
            if len(parts) == 2 and parts[0] in ('PASS', 'FAIL', 'XFAIL', 'CRASH', 'TIMEOUT'):
                # Strip ' | expected:... actual:...' suffix if present
                test_path = parts[1].split(' | ')[0]
                results[test_path] = 'PASS' if parts[0] == 'PASS' else 'FAIL'
    return results

baseline = parse_result_file(sys.argv[1])
current = parse_result_file(sys.argv[2])

regressions = sorted(t for t in baseline if baseline[t] == 'PASS' and current.get(t) == 'FAIL')
improvements = sorted(t for t in current if current[t] == 'PASS' and baseline.get(t) == 'FAIL')
new_tests = sorted(t for t in current if t not in baseline)
removed_tests = sorted(t for t in baseline if t not in current)

b_pass = sum(1 for v in baseline.values() if v == 'PASS')
c_pass = sum(1 for v in current.values() if v == 'PASS')
delta = c_pass - b_pass

if not regressions and not improvements:
    print(f'No regressions or improvements vs baseline ({b_pass} -> {c_pass}, delta={delta:+d})')
else:
    if improvements:
        print(f'✓ {len(improvements)} improvements (FAIL -> PASS):')
        for t in improvements:
            print(f'  + {t}')
    if regressions:
        print(f'✗ {len(regressions)} regressions (PASS -> FAIL):')
        for t in regressions:
            print(f'  - {t}')
    print(f'Net: {b_pass} -> {c_pass} ({delta:+d})')
" "$baseline_file" "$current_file"
}

clean_cache() {
    echo "Removing cache/domain files:"
    echo "  $CACHE_FILE"
    echo "  $DOMAIN_FILE"
    rm -f "$CACHE_FILE" "$DOMAIN_FILE"
    echo -e "${GREEN}Cache and domain cleaned${NC}"
}

# Ensure the standalone TypeScript corpus is pinned and pristine before tests.
# tsc can emit ignored .d.ts/.js files next to test cases, polluting the corpus
# and causing cache misses (extra .js files get picked up as test inputs).
# Keep cleanup scoped to tests/cases so dependency and harness caches survive.
check_submodule_clean() {
    local ts_dir="$REPO_ROOT/TypeScript"
    local reset_helper="$REPO_ROOT/scripts/setup/reset-ts-submodule.sh"
    if ! "$reset_helper" --sparse; then
        echo -e "${YELLOW}ERROR: Could not materialize the pinned TypeScript corpus.${NC}" >&2
        return 1
    fi

    local expected_sha
    expected_sha=$(tr -d '[:space:]' < "$REPO_ROOT/scripts/ci/typescript-submodule-ref")
    local actual_sha
    if ! actual_sha=$(cd "$ts_dir" && git rev-parse HEAD 2>/dev/null); then
        echo -e "${YELLOW}ERROR: Could not read TypeScript corpus HEAD.${NC}" >&2
        return 1
    fi
    if [ "$expected_sha" != "$actual_sha" ]; then
        echo -e "${YELLOW}ERROR: TypeScript corpus SHA mismatch after reset.${NC}" >&2
        echo "  Expected: $expected_sha" >&2
        echo "  Actual: ${actual_sha:-<no HEAD>}" >&2
        return 1
    fi

    # A worktree symlink points at a shared corpus. The helper verified its SHA
    # and cleanliness; never mutate it from this checkout.
    if [ -L "$ts_dir" ]; then
        echo -e "${GREEN}✓ Shared TypeScript corpus verified${NC}"
        echo ""
        return 0
    fi

    echo -e "${YELLOW}Cleaning ignored outputs under TypeScript/tests/cases...${NC}"
    if ! (cd "$ts_dir" && git clean -xfd -- tests/cases >/dev/null 2>&1); then
        echo -e "${YELLOW}ERROR: Could not clean generated TypeScript test-case outputs.${NC}" >&2
        return 1
    fi
    echo -e "${GREEN}✓ TypeScript corpus clean${NC}"
    echo ""
}

validate_snapshot_selection() {
    # Tracked snapshots are full-domain evidence, never subset observations.
    if [ "$NO_CACHE" = "true" ]; then
        echo -e "${YELLOW}ERROR: snapshot --no-cache is not atomic; generate and review the oracle cache separately.${NC}" >&2
        return 1
    fi
    if [ "$CUSTOM_TEST_DIR" = "true" ]; then
        echo -e "${YELLOW}ERROR: tracked snapshots require the pinned default test directory.${NC}" >&2
        return 1
    fi
    local arg
    for arg in "${REMAINING_ARGS[@]+"${REMAINING_ARGS[@]}"}"; do
        case "$arg" in
            -v)
                # The only safe short option in tracked snapshot mode.
                ;;
            --filter|--filter=*|--max|--max=*|-m|-m=*|-m[0-9]*|--offset|--offset=*|-o|-o=*|-o[0-9]*|--shard|--shard=*|--plan|--plan=*|--error-code|--error-code=*|--cache-file|--cache-file=*|--domain-file|--domain-file=*|--tsz-binary|--tsz-binary=*|--mode|--mode=*|--cache-clear|--cache-status|--timings-file|--timings-file=*|--write-diff-artifacts|--diff-artifacts-dir|--diff-artifacts-dir=*)
                echo -e "${YELLOW}ERROR: tracked snapshots reject subset/custom runner argument: $arg.${NC}" >&2
                return 1
                ;;
            -*)
                echo -e "${YELLOW}ERROR: tracked snapshots reject clustered/unknown short runner argument: $arg.${NC}" >&2
                return 1
                ;;
        esac
    done
}

snapshot_tests() {
    local snapshot_file="$REPO_ROOT/scripts/conformance/conformance-snapshot.json"
    local prev_pass=0

    validate_snapshot_selection || return 1
    resolve_tsz_lib_dir || return 1
    local provenance_json
    provenance_json=$(mktemp)
    capture_snapshot_provenance() {
        local output_path="$1"
        local provenance_args=(
            --repo "$REPO_ROOT"
            --test-dir "$TEST_DIR"
            --cache "$CACHE_FILE"
            --domain "$DOMAIN_FILE"
            --build-manifest "$BUILD_MANIFEST"
            --binary "tsz=$TSZ_BIN"
            --binary "tsz-server=$SERVER_BIN"
            --binary "generate-tsc-cache=$CACHE_GEN_BIN"
            --binary "tsz-conformance=$RUNNER_BIN"
            --workers "$WORKERS"
            --output "$output_path"
        )
        local runner_arg
        for runner_arg in "${REMAINING_ARGS[@]+"${REMAINING_ARGS[@]}"}"; do
            provenance_args+=("--runner-arg=$runner_arg")
        done
        python3 "$REPO_ROOT/scripts/conformance/snapshot-provenance.py" \
            "${provenance_args[@]}"
    }
    capture_snapshot_provenance "$provenance_json" || return 1
    local git_sha
    git_sha=$(python3 -c \
        "import json,sys; print(json.load(open(sys.argv[1]))['git']['commit'])" \
        "$provenance_json")

    echo -e "${GREEN}Running full conformance snapshot (run + analyze + areas)...${NC}"

    cd "$REPO_ROOT"

    if [ -f "$snapshot_file" ]; then
        prev_pass=$(python3 -c "
import json, sys
d = json.load(open(sys.argv[1]))
print(d['summary']['passed'])
" "$snapshot_file")
    fi

    # 1) Run tests with --print-test to get per-test results
    local tmpfile
    tmpfile=$(mktemp)
    local summary_json
    summary_json=$(mktemp)
    trap "rm -f '$tmpfile' '$summary_json' '$provenance_json'" RETURN
    run_snapshot_once() {
        rm -f "$tmpfile"
        tmpfile=$(mktemp)
        local runner_status=0

        # Runner exits non-zero when tests fail, so capture status explicitly
        # and validate snapshot completeness ourselves.
        set +e
        $RUNNER_BIN \
            --test-dir "$TEST_DIR" \
            --cache-file "$CACHE_FILE" \
            --domain-file "$DOMAIN_FILE" \
            --tsz-binary "$TSZ_BIN" \
            --workers $WORKERS \
            --print-test \
            "${REMAINING_ARGS[@]+"${REMAINING_ARGS[@]}"}" > "$tmpfile" 2>&1
        runner_status=$?
        set -e

        # Verify runner produced output
        if [ ! -s "$tmpfile" ]; then
            echo -e "${YELLOW}ERROR: conformance runner produced no output${NC}"
            return 1
        fi

        # 2) Validate the exact same identity/accounting contract used by `run`.
        python3 "$REPO_ROOT/scripts/conformance/validate-runner-output.py" \
            "$tmpfile" --runner-status "$runner_status" --output "$summary_json"
    }

    local candidate_tests total_tests unsupported_tests skipped_tests
    local passed failed crashed timeout pass_rate recorded_results recorded_runnable
    local has_final_results partition_valid runner_status
    run_snapshot_once || return 1

    # Read values from the first and only canonical invocation (no retry/election).
    total_tests=$(python3 -c "import json,sys; d=json.load(open(sys.argv[1])); print(d['total'])" "$summary_json")
    candidate_tests=$(python3 -c "import json,sys; d=json.load(open(sys.argv[1])); print(d['candidates'])" "$summary_json")
    unsupported_tests=$(python3 -c "import json,sys; d=json.load(open(sys.argv[1])); print(d['unsupported'])" "$summary_json")
    skipped_tests=$(python3 -c "import json,sys; d=json.load(open(sys.argv[1])); print(d['skipped'])" "$summary_json")
    passed=$(python3 -c "import json,sys; d=json.load(open(sys.argv[1])); print(d['passed'])" "$summary_json")
    failed=$(python3 -c "import json,sys; d=json.load(open(sys.argv[1])); print(d['failed'])" "$summary_json")
    crashed=$(python3 -c "import json,sys; d=json.load(open(sys.argv[1])); print(d.get('crashed', 0))" "$summary_json")
    timeout=$(python3 -c "import json,sys; d=json.load(open(sys.argv[1])); print(d.get('timeout', 0))" "$summary_json")
    pass_rate=$(python3 -c "import json,sys; d=json.load(open(sys.argv[1])); print(d['rate'])" "$summary_json")
    recorded_results=$(python3 -c "import json,sys; d=json.load(open(sys.argv[1])); print(d.get('recorded_candidates', 0))" "$summary_json")
    recorded_runnable=$(python3 -c "import json,sys; d=json.load(open(sys.argv[1])); print(d.get('recorded_runnable', 0))" "$summary_json")
    has_final_results=$(python3 -c "import json,sys; d=json.load(open(sys.argv[1])); print('true' if d.get('has_final_results') else 'false')" "$summary_json")
    partition_valid=$(python3 -c "import json,sys; d=json.load(open(sys.argv[1])); print('true' if d.get('partition_valid') else 'false')" "$summary_json")
    runner_status=$(python3 -c "import json,sys; d=json.load(open(sys.argv[1])); print(d.get('runner_status', 0))" "$summary_json")

    if [ "$has_final_results" != "true" ]; then
        echo -e "${YELLOW}ERROR: Snapshot run missing FINAL RESULTS summary (runner exit: $runner_status).${NC}"
        return 1
    fi

    if [ "$partition_valid" != "true" ]; then
        echo -e "${YELLOW}ERROR: Snapshot candidate partition is inconsistent: ${candidate_tests} != ${total_tests} runnable + ${unsupported_tests} unsupported + ${skipped_tests} skipped.${NC}"
        return 1
    fi

    if [ "$recorded_results" -ne "$candidate_tests" ] || [ "$recorded_runnable" -ne "$total_tests" ]; then
        echo -e "${YELLOW}ERROR: Snapshot run was incomplete (${recorded_results}/${candidate_tests} candidates, ${recorded_runnable}/${total_tests} runnable).${NC}"
        echo -e "${YELLOW}Incomplete candidate coverage cannot be saved, including with --force.${NC}"
        return 1
    fi

    if [ "$FORCE_SNAPSHOT" != "true" ] && [ "$prev_pass" -gt 0 ] && [ "$passed" -lt "$prev_pass" ]; then
        echo -e "${YELLOW}ERROR: Snapshot run regressed vs previous snapshot ($passed vs $prev_pass passes).${NC}"
        echo -e "${YELLOW}Investigate before saving, or use --force to override.${NC}"
        return 1
    fi

    # Guard 2: Regression check — abort if score dropped >5% from previous snapshot
    if [ "$FORCE_SNAPSHOT" != "true" ] && [ -f "$snapshot_file" ]; then
        local prev_rate
        prev_rate=$(python3 -c "
import json, sys
d = json.load(open(sys.argv[1]))
print(d['summary']['pass_rate'])
" "$snapshot_file")
        local drop
        drop=$(python3 -c "
prev, curr = float('$prev_rate'), float('$pass_rate')
print(f'{prev - curr:.1f}')
")
        local is_regression
        is_regression=$(python3 -c "print('yes' if float('$drop') > 5.0 else 'no')")
        if [ "$is_regression" = "yes" ]; then
            echo -e "${YELLOW}ERROR: Snapshot score dropped ${drop}% (${prev_rate}% -> ${pass_rate}%).${NC}"
            echo -e "${YELLOW}This likely indicates a stale build or broken binary.${NC}"
            echo -e "${YELLOW}Use --force to save the snapshot anyway.${NC}"
            return 1
        fi
    fi

    # 3) Build per-test detail snapshot (compact JSON with all failure data)
    local detail_file="$REPO_ROOT/scripts/conformance/conformance-detail.json"
    local detail_tmp
    detail_tmp=$(mktemp)
    python3 "$REPO_ROOT/scripts/conformance/build-snapshot-detail.py" "$tmpfile" \
        --output "$detail_tmp" \
        --git-sha "$git_sha" \
        --provenance "$provenance_json" \
        || { echo "ERROR: failed to build conformance detail snapshot"; return 1; }

    # 4) Run analyze with JSON output
    local analyze_json
    analyze_json=$(mktemp)
    python3 "$REPO_ROOT/scripts/conformance/analyze-conformance.py" "$tmpfile" \
        --json-output "$analyze_json" \
        || { echo "ERROR: failed to analyze conformance snapshot"; return 1; }

    # 5) Run areas with JSON output (depth 2, min 10 tests)
    local areas_json
    areas_json=$(mktemp)
    python3 "$REPO_ROOT/scripts/conformance/analyze-conformance-areas.py" "$tmpfile" \
        --depth 2 --min-tests 10 --json-output "$areas_json" \
        || { echo "ERROR: failed to analyze conformance areas"; return 1; }

    # 6) Assemble snapshot JSON (all data passed as arguments, not interpolated)
    local timestamp
    timestamp="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
    local snapshot_tmp
    snapshot_tmp=$(mktemp)

    python3 -c "
import json, sys

timestamp, git_sha = sys.argv[1], sys.argv[2]
candidates, runnable = int(sys.argv[3]), int(sys.argv[4])
passed, failed = int(sys.argv[5]), int(sys.argv[6])
unsupported, skipped = int(sys.argv[7]), int(sys.argv[8])
crashed, timeout = int(sys.argv[9]), int(sys.argv[10])
rate = float(sys.argv[11])
analyze_path, areas_path, detail_path, provenance_path, out_path = sys.argv[12:17]

analyze, areas, detail = {}, {}, {}
try:
    with open(analyze_path) as f: analyze = json.load(f)
except Exception as error: raise SystemExit(f'cannot load analysis artifact: {error}')
try:
    with open(areas_path) as f: areas = json.load(f)
except Exception as error: raise SystemExit(f'cannot load areas artifact: {error}')
try:
    with open(detail_path) as f: detail = json.load(f)
except Exception as error: raise SystemExit(f'cannot load detail artifact: {error}')
try:
    with open(provenance_path) as f: provenance = json.load(f)
except Exception as error: raise SystemExit(f'cannot load provenance artifact: {error}')

# Pull richer aggregates from the detail file when available
aggregates = detail.get('aggregates', {})
detail_summary = detail.get('summary', {})
expected_detail = {
    'candidates': candidates,
    'runnable': runnable,
    'passed': passed,
    'failed': failed,
    'unsupported': unsupported,
    'skipped': skipped,
    'crashed': crashed,
    'timeout': timeout,
}
actual_detail = {
    'candidates': int(detail_summary.get('candidates', -1)),
    'runnable': int(detail_summary.get('runnable', detail_summary.get('total', -1))),
    'passed': int(detail_summary.get('passed', -1)),
    'failed': int(detail_summary.get('failed', -1)),
    'unsupported': int(detail_summary.get('unsupported', -1)),
    'skipped': int(detail_summary.get('skipped', -1)),
    'crashed': int(detail_summary.get('crashed', -1)),
    'timeout': int(detail_summary.get('timeout', -1)),
}
if actual_detail != expected_detail:
    raise SystemExit(
        f'detail/runner accounting mismatch: detail={actual_detail}, runner={expected_detail}'
    )
if candidates != runnable + unsupported + skipped:
    raise SystemExit(
        f'invalid candidate partition: {candidates} != {runnable} + {unsupported} + {skipped}'
    )

snapshot = {
    'timestamp': timestamp,
    'git_sha': git_sha,
    'provenance': provenance,
    'summary': {
        'candidates': candidates,
        'total_tests': runnable,
        'runnable': runnable,
        'passed': passed,
        'failed': failed,
        'unsupported': unsupported,
        'skipped': skipped,
        'crashed': actual_detail['crashed'],
        'timeout': actual_detail['timeout'],
        'pass_rate': rate,
    },
    'areas_by_pass_rate': areas.get('areas', []),
    'top_failures': analyze.get('quick_wins', []),
    'not_implemented_codes': aggregates.get('not_implemented_codes', analyze.get('not_implemented_codes', [])),
    'partial_codes': aggregates.get('partial_codes', analyze.get('partial_codes', [])),
    'one_missing_zero_extra': aggregates.get('one_missing_zero_extra', []),
    'one_extra_zero_missing': aggregates.get('one_extra_zero_missing', []),
    'false_positive_codes': aggregates.get('false_positive_codes', []),
    'top_missing_codes': aggregates.get('top_missing_codes', []),
    'top_extra_codes': aggregates.get('top_extra_codes', []),
    'categories': aggregates.get('categories', {}),
    'terminal_failures': detail.get('terminal_failures', {}),
}

with open(out_path, 'w') as f:
    json.dump(snapshot, f, indent=2)

print(
    f'Snapshot saved: {candidates} candidates, {runnable} runnable, '
    f'{passed} passed, {unsupported} unsupported, {skipped} skipped ({rate}%)'
)
print(f'Git SHA: {git_sha}')
print(f'Areas ranked: {len(snapshot[\"areas_by_pass_rate\"])}')
" "$timestamp" "$git_sha" "$candidate_tests" "$total_tests" "$passed" "$failed" \
  "$unsupported_tests" "$skipped_tests" "$crashed" "$timeout" "$pass_rate" "$analyze_json" "$areas_json" \
  "$detail_tmp" "$provenance_json" "$snapshot_tmp" \
  || { echo "ERROR: failed to assemble snapshot JSON"; return 1; }

    rm -f "$summary_json" "$analyze_json" "$areas_json"

    # Verify snapshot is valid JSON
    python3 -m json.tool "$snapshot_tmp" > /dev/null || { echo "ERROR: snapshot is not valid JSON"; return 1; }

    # 6) Save per-test baseline for regression diffing (with expected/actual codes)
    local baseline_file="$REPO_ROOT/scripts/conformance/conformance-baseline.txt"
    local baseline_tmp
    baseline_tmp=$(mktemp)
    if ! python3 "$REPO_ROOT/scripts/conformance/extract-baseline.py" "$tmpfile" > "$baseline_tmp"; then
        echo "ERROR: failed to extract snapshot baseline" >&2
        return 1
    fi

    # Re-capture every immutable input after the compiler run and artifact
    # assembly. Nothing is published if HEAD, cleanliness, build inputs,
    # binaries, corpus, oracle, cache, domain, or CLI selection changed.
    local provenance_after
    provenance_after=$(mktemp)
    capture_snapshot_provenance "$provenance_after" || return 1
    if ! cmp -s "$provenance_json" "$provenance_after"; then
        echo "ERROR: snapshot provenance changed during the canonical run" >&2
        return 1
    fi
    rm -f "$provenance_after"

    mv "$detail_tmp" "$detail_file"
    mv "$snapshot_tmp" "$snapshot_file"
    mv "$baseline_tmp" "$baseline_file"
    local baseline_count
    baseline_count=$(wc -l < "$baseline_file" | tr -d ' ')
    echo -e "${GREEN}Baseline saved: $baseline_file ($baseline_count tests)${NC}"

    echo -e "${GREEN}Detail written to: $detail_file${NC}"
    echo -e "${GREEN}Snapshot written to: $snapshot_file${NC}"
    echo -e "${GREEN}Query offline: python3 scripts/conformance/query-conformance.py${NC}"
    return "$runner_status"
}

# Parse arguments
# Check for help flags first
if [[ "${1:-}" == "help" ]] || [[ "${1:-}" == "--help" ]] || [[ "${1:-}" == "-h" ]]; then
    COMMAND="help"
    shift
# If first argument starts with --, assume user meant 'run' command
elif [[ "${1:-}" == --* ]]; then
    COMMAND="run"
else
    COMMAND="${1:-all}"
    if [ "$#" -gt 0 ]; then
        shift
    fi
fi

# Check for flags
NO_CACHE=false
FORCE_SNAPSHOT=false
CUSTOM_TEST_DIR=false
REMAINING_ARGS=()
i=0
while [ $i -lt ${#@} ]; do
    arg="${@:$((i+1)):1}"
    if [ "$arg" = "--no-cache" ]; then
        NO_CACHE=true
    elif [ "$arg" = "--force" ]; then
        FORCE_SNAPSHOT=true
    elif [ "$arg" = "--workers" ]; then
        i=$((i + 1))
        WORKERS="${@:$((i+1)):1}"
    elif [ "$arg" = "--test-dir" ]; then
        i=$((i + 1))
        TEST_DIR="${@:$((i+1)):1}"
        CUSTOM_TEST_DIR=true
    elif [[ "$arg" == --test-dir=* ]]; then
        TEST_DIR="${arg#--test-dir=}"
        CUSTOM_TEST_DIR=true
    elif [ "$arg" = "--profile" ]; then
        i=$((i + 1))
        BUILD_PROFILE="${@:$((i+1)):1}"
        TSZ_BIN="$REPO_ROOT/.target/$BUILD_PROFILE/tsz"
        SERVER_BIN="$REPO_ROOT/.target/$BUILD_PROFILE/tsz-server"
        CACHE_GEN_BIN="$REPO_ROOT/.target/$BUILD_PROFILE/generate-tsc-cache"
        RUNNER_BIN="$REPO_ROOT/.target/$BUILD_PROFILE/tsz-conformance"
        BUILD_MANIFEST="$REPO_ROOT/.target/$BUILD_PROFILE/conformance-build-manifest.json"
    else
        REMAINING_ARGS+=("$arg")
    fi
    i=$((i + 1))
done

# Reject subset snapshot requests before cache generation or any tracked write.
if [ "$COMMAND" = "snapshot" ]; then
    validate_snapshot_selection || exit 1
fi

case "$COMMAND" in
    generate)
        check_submodule_clean
        ensure_binaries
        if [ "$NO_CACHE" = "true" ]; then
            generate_cache "true"
        else
            generate_cache
        fi
        ;;
    run)
        check_submodule_clean
        if [ "$NO_CACHE" = "true" ]; then
            ensure_binaries
            echo -e "${YELLOW}--no-cache flag set, regenerating cache...${NC}"
            generate_cache "true"
            echo ""
        else
            ensure_cache
            ensure_binaries
        fi
        run_tests "${REMAINING_ARGS[@]+"${REMAINING_ARGS[@]}"}"
        ;;
    analyze)
        analyze_tests "${REMAINING_ARGS[@]+"${REMAINING_ARGS[@]}"}"
        ;;
    render-corpus)
        python3 "$REPO_ROOT/scripts/conformance/classify-render-corpus.py" \
            "${REMAINING_ARGS[@]+"${REMAINING_ARGS[@]}"}"
        ;;
    areas)
        check_submodule_clean
        if [ "$NO_CACHE" = "true" ]; then
            ensure_binaries
            generate_cache "true"
        else
            ensure_cache
            ensure_binaries
        fi
        areas_analysis "${REMAINING_ARGS[@]+"${REMAINING_ARGS[@]}"}"
        ;;
    diff)
        # Diff last run against baseline (no need to re-run tests)
        baseline="$REPO_ROOT/scripts/conformance/conformance-baseline.txt"
        last_run="$REPO_ROOT/scripts/conformance/conformance-last-run.txt"
        if [ ! -f "$baseline" ]; then
            echo "No baseline found. Run './scripts/conformance/conformance.sh snapshot' first."
            exit 1
        fi
        if [ ! -f "$last_run" ]; then
            echo "No last-run results. Run './scripts/conformance/conformance.sh run' first."
            exit 1
        fi
        diff_results "$baseline" "$last_run"
        ;;
    all)
        check_submodule_clean
        if [ "$NO_CACHE" = "true" ]; then
            ensure_binaries
            generate_cache "true"
        else
            ensure_cache
            ensure_binaries
        fi
        echo ""
        run_tests "${REMAINING_ARGS[@]+"${REMAINING_ARGS[@]}"}"
        ;;
    snapshot)
        check_submodule_clean
        if [ "$NO_CACHE" = "true" ]; then
            ensure_binaries
            generate_cache "true"
        else
            ensure_cache
            ensure_binaries
        fi
        snapshot_tests
        ;;
    clean)
        clean_cache
        ;;
    help|--help|-h)
        print_help
        ;;
    *)
        echo "Error: Unknown command '$COMMAND'"
        echo ""
        print_help
        exit 1
        ;;
esac
