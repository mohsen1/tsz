#!/bin/bash
# Conformance Test Runner
# Usage: ./scripts/conformance/conformance.sh [generate|run|all] [options]

set -e

# Get the repository root directory
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

# Default values (relative to repo root)
TEST_DIR="$REPO_ROOT/TypeScript/tests/cases"
CACHE_FILE="$REPO_ROOT/scripts/conformance/tsc-cache-full.json"

# Build profile (dist-fast = fast build + good runtime perf)
BUILD_PROFILE="dist-fast"

# Binary paths (will be updated based on profile)
TSZ_BIN="$REPO_ROOT/.target/dist-fast/tsz"
SERVER_BIN="$REPO_ROOT/.target/dist-fast/tsz-server"
CACHE_GEN_BIN="$REPO_ROOT/.target/dist-fast/generate-tsc-cache"
RUNNER_BIN="$REPO_ROOT/.target/dist-fast/tsz-conformance"

WORKERS=16

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
  --workers N       Number of parallel workers (default: 16)
  --profile NAME    Cargo build profile (default: dist-fast)
  --test-dir PATH   Override TypeScript test corpus path
  --no-cache        Force cache regeneration even if cache exists
  --force           Override snapshot safety guards (dirty-tree + regression check)

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
  ./scripts/conformance/conformance.sh areas --depth 2            # Sub-area breakdown

Note: Fingerprint comparison (code + location + message) is always enabled.
      Binaries are automatically built if not found.
      Cache: scripts/conformance/tsc-cache-full.json
      Offline analysis reads scripts/conformance/conformance-detail.json from the last snapshot.
EOF
}

# Cross-platform file modification time (seconds since epoch)
# Linux: stat -c %Y, macOS: stat -f %m
file_mtime() {
    if stat -c %Y /dev/null >/dev/null 2>&1; then
        stat -c %Y "$1" 2>/dev/null
    else
        stat -f %m "$1" 2>/dev/null
    fi
}

# Check if binaries are up-to-date with source code
# Returns 0 if binaries are fresh (up-to-date), 1 if they need rebuilding
binaries_are_fresh() {
    local binary_dir="$REPO_ROOT/.target/$BUILD_PROFILE"
    local tsz_bin="$binary_dir/tsz"
    local conformance_bin="$binary_dir/tsz-conformance"
    local cache_gen_bin="$binary_dir/generate-tsc-cache"
    
    # Check if all binaries exist
    if [ ! -f "$tsz_bin" ] || [ ! -f "$conformance_bin" ] || [ ! -f "$cache_gen_bin" ]; then
        return 1
    fi
    
    # Find the newest binary modification time
    local newest_binary_mtime=$(for f in "$tsz_bin" "$conformance_bin" "$cache_gen_bin"; do file_mtime "$f"; done | sort -n | tail -1)
    
    # Check if any Rust source file in the relevant crates is newer than the binaries
    # These are all the workspace crates that tsz-cli and tsz-conformance depend on
    local crates_to_check=(
        "tsz-cli"
        "conformance"
        "tsz-common"
        "tsz-scanner"
        "tsz-parser"
        "tsz-binder"
        "tsz-solver"
        "tsz-checker"
        "tsz-emitter"
        "tsz-lsp"
        "tsz-wasm"
    )
    
    local crates_dir="$REPO_ROOT/crates"
    
    for crate_name in "${crates_to_check[@]}"; do
        local crate_dir="$crates_dir/$crate_name"
        
        # Check source files
        if [ -d "$crate_dir/src" ]; then
            while IFS= read -r -d '' src_file; do
                local src_mtime=$(file_mtime "$src_file")
                if [ "$src_mtime" -gt "$newest_binary_mtime" ]; then
                    return 1
                fi
            done < <(find "$crate_dir/src" -name "*.rs" -print0 2>/dev/null)
        fi
        
        # Check Cargo.toml
        if [ -f "$crate_dir/Cargo.toml" ]; then
            local toml_mtime=$(file_mtime "$crate_dir/Cargo.toml")
            if [ "$toml_mtime" -gt "$newest_binary_mtime" ]; then
                return 1
            fi
        fi
    done
    
    # Check root workspace src/ directory (tsz-core crate)
    if [ -d "$REPO_ROOT/src" ]; then
        while IFS= read -r -d '' src_file; do
            local src_mtime=$(file_mtime "$src_file")
            if [ "$src_mtime" -gt "$newest_binary_mtime" ]; then
                return 1
            fi
        done < <(find "$REPO_ROOT/src" -name "*.rs" -print0 2>/dev/null)
    fi

    # Check root Cargo.toml and Cargo.lock
    if [ -f "$REPO_ROOT/Cargo.toml" ]; then
        local root_toml_mtime=$(file_mtime "$REPO_ROOT/Cargo.toml")
        if [ "$root_toml_mtime" -gt "$newest_binary_mtime" ]; then
            return 1
        fi
    fi
    if [ -f "$REPO_ROOT/Cargo.lock" ]; then
        local lock_mtime=$(file_mtime "$REPO_ROOT/Cargo.lock")
        if [ "$lock_mtime" -gt "$newest_binary_mtime" ]; then
            return 1
        fi
    fi
    
    return 0
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
    CARGO_INCREMENTAL="$cargo_incremental" cargo build --profile "$BUILD_PROFILE" -p tsz-cli -p tsz-conformance
    
    echo ""
}

# Ensure scripts/node_modules is installed (provides TypeScript lib files for type checking)
ensure_scripts_deps() {
    if [ ! -d "$REPO_ROOT/scripts/node_modules/typescript" ]; then
        echo -e "${YELLOW}Installing scripts dependencies (TypeScript libs)...${NC}"
        (cd "$REPO_ROOT/scripts" && npm install --silent 2>/dev/null || npm install)
    fi
}

generate_cache() {
    local force_regenerate="${1:-false}"

    # Ensure scripts dependencies (TypeScript + emit runner deps) are installed
    ensure_scripts_deps

    if [ "$force_regenerate" != "true" ] && [ -f "$CACHE_FILE" ]; then
        echo -e "${YELLOW}Cache already exists: $CACHE_FILE${NC}"
        echo "Skipping cache generation."
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
        --test-dir "$TEST_DIR" \
        --output "$CACHE_FILE" \
        --workers "$WORKERS"

    echo ""
    echo -e "${GREEN}Cache generated: $CACHE_FILE${NC}"
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
    if ! pinned_version="$(node -e "const fs = require('fs'); const cfg = JSON.parse(fs.readFileSync('$REPO_ROOT/scripts/conformance/typescript-versions.json', 'utf8')); const current = cfg.current; const mapping = current && cfg.mappings && cfg.mappings[current] && cfg.mappings[current].npm; const fallback = cfg.default && cfg.default.npm; process.stdout.write(mapping || fallback || '');")"; then
        echo -e "${YELLOW}Failed to read pinned TypeScript version from scripts/conformance/typescript-versions.json${NC}"
        echo -e "${YELLOW}Proceeding without cache-version validation${NC}"
        return
    fi
    if [ -z "$pinned_version" ]; then
        echo -e "${YELLOW}Could not resolve pinned TypeScript version from scripts/conformance/typescript-versions.json${NC}"
        echo -e "${YELLOW}Proceeding without cache-version validation${NC}"
        return
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

for (const [path, entry] of Object.entries(cache)) {
  checked += 1;
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
  console.log(`missing=${missing},mismatch=${mismatch},sample=${samplePath},sampleVersion=${sampleVersion}`);
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
        echo -e "${YELLOW}TypeScript cache was generated with a different TypeScript version than pinned:${NC}"
        echo "  Pinned version: $pinned_version"
        echo "  Cache check: ${cache_report:-unknown}"
        echo -e "${YELLOW}Re-run with --no-cache to regenerate cache, or update cache file to match pinned version.${NC}"
        echo -e "${YELLOW}Proceeding with stale cache (results may differ from pinned tsc)${NC}"
        return 0
    fi

    echo -e "${GREEN}TypeScript cache version matches pinned version: $pinned_version${NC}"
    return 0
}

run_tests() {
    # TypeScript lib files are needed for type checking (resolved via scripts/node_modules/typescript/lib)
    ensure_scripts_deps

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

    # Run with --print-test to get PASS/FAIL per test line
    $RUNNER_BIN \
        --test-dir "$TEST_DIR" \
        --cache-file "$CACHE_FILE" \
        --tsz-binary "$TSZ_BIN" \
        --server-binary "$SERVER_BIN" \
        --workers $WORKERS \
        --print-test \
        "${runner_flags[@]}" \
        "${extra_args[@]}" 2>/dev/null | tee "$tmpout" || true

    # Extract sorted PASS/FAIL lines with expected/actual codes for diffing
    python3 "$REPO_ROOT/scripts/conformance/extract-baseline.py" "$tmpout" > "$last_run" 2>/dev/null || true
    rm -f "$tmpout"

    # Auto-diff against baseline if it exists and this was an unfiltered run
    local baseline="$REPO_ROOT/scripts/conformance/conformance-baseline.txt"
    local has_filter=false
    for arg in "${extra_args[@]}"; do
        if [[ "$arg" == "--filter" ]] || [[ "$arg" == --filter=* ]]; then
            has_filter=true
            break
        fi
    done

    if [ "$has_filter" = false ] && [ -f "$baseline" ] && [ -s "$last_run" ]; then
        echo ""
        diff_results "$baseline" "$last_run"
    fi
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
        --tsz-binary "$TSZ_BIN" \
        --server-binary "$SERVER_BIN" \
        --workers $WORKERS \
        --print-test \
        "${extra_args[@]}" > "$tmpfile" 2>/dev/null || true

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
    Handles both old format (PASS/FAIL path) and new format
    (FAIL path | expected:[...] actual:[...]).\"\"\"
    results = {}
    with open(path) as f:
        for line in f:
            line = line.strip()
            parts = line.split(' ', 1)
            if len(parts) == 2 and parts[0] in ('PASS', 'FAIL'):
                # Strip ' | expected:... actual:...' suffix if present
                test_path = parts[1].split(' | ')[0]
                results[test_path] = parts[0]
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
    echo "Removing cache file: $CACHE_FILE"
    rm -f "$CACHE_FILE"
    echo -e "${GREEN}Cache cleaned${NC}"
}

# Ensure the TypeScript submodule is pristine before running tests.
# tsc can emit .d.ts/.js files next to test files, polluting the submodule
# and causing cache misses (extra .js files get picked up as test inputs).
# Always run git clean -xf to guarantee a clean state.
check_submodule_clean() {
    local ts_dir="$REPO_ROOT/TypeScript"
    if [ ! -d "$ts_dir/.git" ] && [ ! -f "$ts_dir/.git" ]; then
        return 0  # Not a git repo/submodule, skip check
    fi

    # Verify the submodule SHA matches what's committed in the parent repo.
    # This catches accidental `cd TypeScript && git checkout <other>` or detached HEAD drift.
    local expected_sha
    expected_sha=$(cd "$REPO_ROOT" && git ls-tree HEAD TypeScript 2>/dev/null | awk '{print $3}')

    # Prefer repository pinned TypeScript SHA so local workflow can proceed with
    # the intended submodule version tracked in scripts/conformance/typescript-versions.json
    # even before the superproject commit is updated.
    local pinned_sha
    pinned_sha=$(node -e "const fs = require('fs'); const p = 'scripts/conformance/typescript-versions.json'; try { const v = JSON.parse(fs.readFileSync(p, 'utf8')); process.stdout.write(v.current || ''); } catch {}" | tr -d '\n')
    if [ -n "$pinned_sha" ]; then
        expected_sha="$pinned_sha"
    fi
    local actual_sha
    actual_sha=$(cd "$ts_dir" && git rev-parse HEAD 2>/dev/null || true)

    # Fresh worktrees can leave the submodule on an unborn/invalid HEAD
    # (for example "ref: refs/heads/.invalid"), which breaks rev-parse and
    # later checkout/clean steps. Recover to the pinned SHA before running.
    if [ -z "$actual_sha" ] || [ "$actual_sha" = "HEAD" ]; then
        echo -e "${YELLOW}⚠ TypeScript submodule HEAD is not checked out; resetting to pinned SHA...${NC}"
        if ! (cd "$REPO_ROOT" && bash scripts/setup/reset-ts-submodule.sh 2>/dev/null); then
            echo -e "${YELLOW}⚠ Automatic submodule reset failed; continuing with current state.${NC}"
        fi
        actual_sha=$(cd "$ts_dir" && git rev-parse HEAD 2>/dev/null || true)
    fi

    if [ -n "$expected_sha" ] && [ -n "$actual_sha" ] && [ "$expected_sha" != "$actual_sha" ]; then
        echo -e "${YELLOW}⚠ TypeScript submodule SHA mismatch!${NC}"
        echo "  Expected (committed): $expected_sha"
        echo "  Actual (checked out): $actual_sha"
        echo -e "${YELLOW}Resetting to committed SHA...${NC}"
        (cd "$REPO_ROOT" && git submodule update --init TypeScript 2>/dev/null)
    fi

    echo -e "${YELLOW}Cleaning TypeScript submodule (git checkout + clean -xfd)...${NC}"
    if ! (cd "$ts_dir" && git checkout -- . >/dev/null 2>&1 && git clean -xfd >/dev/null 2>&1); then
        echo -e "${YELLOW}⚠ Could not fully clean TypeScript submodule; continuing.${NC}"
    else
        echo -e "${GREEN}✓ TypeScript submodule clean${NC}"
    fi
    echo ""
}

snapshot_tests() {
    local snapshot_file="$REPO_ROOT/scripts/conformance/conformance-snapshot.json"
    local git_sha
    git_sha="$(cd "$REPO_ROOT" && git rev-parse HEAD 2>/dev/null || echo 'unknown')"

    # Guard 1: Dirty-tree check — prevent snapshots from uncommitted or worktree builds
    if [ "$FORCE_SNAPSHOT" != "true" ]; then
        local dirty
        dirty="$(cd "$REPO_ROOT" && git status --porcelain -- crates/ src/ Cargo.toml Cargo.lock 2>/dev/null | head -1)"
        if [ -n "$dirty" ]; then
            echo -e "${YELLOW}ERROR: Working tree has uncommitted changes to source files.${NC}"
            echo -e "${YELLOW}Snapshot would record a score that doesn't match any commit.${NC}"
            echo -e "${YELLOW}Commit or stash your changes first, or use --force to override.${NC}"
            return 1
        fi
    fi

    echo -e "${GREEN}Running full conformance snapshot (run + analyze + areas)...${NC}"

    cd "$REPO_ROOT"

    # 1) Run tests with --print-test to get per-test results
    local tmpfile
    tmpfile=$(mktemp)
    trap "rm -f '$tmpfile'" RETURN

    # Runner exits non-zero when any tests fail, which is expected
    $RUNNER_BIN \
        --test-dir "$TEST_DIR" \
        --cache-file "$CACHE_FILE" \
        --tsz-binary "$TSZ_BIN" \
        --server-binary "$SERVER_BIN" \
        --workers $WORKERS \
        --print-test \
        "${REMAINING_ARGS[@]}" > "$tmpfile" 2>/dev/null || true

    # Verify runner produced output
    if [ ! -s "$tmpfile" ]; then
        echo -e "${YELLOW}ERROR: conformance runner produced no output${NC}"
        return 1
    fi

    # 2) Extract summary values via Python -> JSON (no eval)
    #    Runner output format: "FINAL RESULTS: N/M passed (X.X%)"
    local summary_json
    summary_json=$(mktemp)
    python3 -c "
import re, sys, json
text = open(sys.argv[1]).read()
m = re.search(r'FINAL RESULTS:\s+(\d+)/(\d+)\s+passed\s+\(([0-9.]+)%\)', text)
passed, total, rate = (int(m.group(1)), int(m.group(2)), float(m.group(3))) if m else (0, 0, 0.0)
json.dump({'total': total, 'passed': passed, 'failed': total - passed, 'rate': rate}, sys.stdout)
" "$tmpfile" > "$summary_json"

    # Read values from JSON (no eval)
    local total_tests passed failed pass_rate
    total_tests=$(python3 -c "import json,sys; d=json.load(open(sys.argv[1])); print(d['total'])" "$summary_json")
    passed=$(python3 -c "import json,sys; d=json.load(open(sys.argv[1])); print(d['passed'])" "$summary_json")
    failed=$(python3 -c "import json,sys; d=json.load(open(sys.argv[1])); print(d['failed'])" "$summary_json")
    pass_rate=$(python3 -c "import json,sys; d=json.load(open(sys.argv[1])); print(d['rate'])" "$summary_json")

    # Guard 2: Regression check — abort if score dropped >5% from previous snapshot
    if [ "$FORCE_SNAPSHOT" != "true" ] && [ -f "$snapshot_file" ]; then
        local prev_rate
        prev_rate=$(python3 -c "
import json, sys
try:
    d = json.load(open(sys.argv[1]))
    print(d['summary']['pass_rate'])
except: print(0)
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
    python3 "$REPO_ROOT/scripts/conformance/build-snapshot-detail.py" "$tmpfile" \
        --output "$detail_file" || true

    # 4) Run analyze with JSON output
    local analyze_json
    analyze_json=$(mktemp)
    python3 "$REPO_ROOT/scripts/conformance/analyze-conformance.py" "$tmpfile" \
        --json-output "$analyze_json" || true

    # 5) Run areas with JSON output (depth 2, min 10 tests)
    local areas_json
    areas_json=$(mktemp)
    python3 "$REPO_ROOT/scripts/conformance/analyze-conformance-areas.py" "$tmpfile" \
        --depth 2 --min-tests 10 --json-output "$areas_json" || true

    # 6) Assemble snapshot JSON (all data passed as arguments, not interpolated)
    local timestamp
    timestamp="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"

    python3 -c "
import json, sys

timestamp, git_sha = sys.argv[1], sys.argv[2]
total, passed, failed = int(sys.argv[3]), int(sys.argv[4]), int(sys.argv[5])
rate = float(sys.argv[6])
analyze_path, areas_path, detail_path, out_path = sys.argv[7], sys.argv[8], sys.argv[9], sys.argv[10]

analyze, areas, detail = {}, {}, {}
try:
    with open(analyze_path) as f: analyze = json.load(f)
except: pass
try:
    with open(areas_path) as f: areas = json.load(f)
except: pass
try:
    with open(detail_path) as f: detail = json.load(f)
except: pass

# Pull richer aggregates from the detail file when available
aggregates = detail.get('aggregates', {})

snapshot = {
    'timestamp': timestamp,
    'git_sha': git_sha,
    'summary': {
        'total_tests': total, 'passed': passed, 'failed': failed,
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
}

with open(out_path, 'w') as f:
    json.dump(snapshot, f, indent=2)

print(f'Snapshot saved: {total} tests, {passed} passed ({rate}%)')
print(f'Git SHA: {git_sha}')
print(f'Areas ranked: {len(snapshot[\"areas_by_pass_rate\"])}')
" "$timestamp" "$git_sha" "$total_tests" "$passed" "$failed" \
  "$pass_rate" "$analyze_json" "$areas_json" "$detail_file" "$snapshot_file" \
  || { echo "ERROR: failed to assemble snapshot JSON"; return 1; }

    rm -f "$summary_json" "$analyze_json" "$areas_json"

    # Verify snapshot is valid JSON
    python3 -m json.tool "$snapshot_file" > /dev/null || { echo "ERROR: snapshot is not valid JSON"; return 1; }

    # 6) Save per-test baseline for regression diffing (with expected/actual codes)
    local baseline_file="$REPO_ROOT/scripts/conformance/conformance-baseline.txt"
    python3 "$REPO_ROOT/scripts/conformance/extract-baseline.py" "$tmpfile" > "$baseline_file" 2>/dev/null || true
    local baseline_count
    baseline_count=$(wc -l < "$baseline_file" | tr -d ' ')
    echo -e "${GREEN}Baseline saved: $baseline_file ($baseline_count tests)${NC}"

    echo -e "${GREEN}Detail written to: $detail_file${NC}"
    echo -e "${GREEN}Snapshot written to: $snapshot_file${NC}"
    echo -e "${GREEN}Query offline: python3 scripts/conformance/query-conformance.py${NC}"
}

# Parse arguments
# Check for help flags first
if [[ "${1:-}" == "help" ]] || [[ "${1:-}" == "--help" ]] || [[ "${1:-}" == "-h" ]]; then
    COMMAND="help"
    shift || true
# If first argument starts with --, assume user meant 'run' command
elif [[ "${1:-}" == --* ]]; then
    COMMAND="run"
else
    COMMAND="${1:-all}"
    shift || true
fi

# Check for flags
NO_CACHE=false
FORCE_SNAPSHOT=false
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
    elif [[ "$arg" == --test-dir=* ]]; then
        TEST_DIR="${arg#--test-dir=}"
    elif [ "$arg" = "--profile" ]; then
        i=$((i + 1))
        BUILD_PROFILE="${@:$((i+1)):1}"
        TSZ_BIN="$REPO_ROOT/.target/$BUILD_PROFILE/tsz"
        SERVER_BIN="$REPO_ROOT/.target/$BUILD_PROFILE/tsz-server"
        CACHE_GEN_BIN="$REPO_ROOT/.target/$BUILD_PROFILE/generate-tsc-cache"
        RUNNER_BIN="$REPO_ROOT/.target/$BUILD_PROFILE/tsz-conformance"
    else
        REMAINING_ARGS+=("$arg")
    fi
    i=$((i + 1))
done

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
        ensure_binaries
        if [ "$NO_CACHE" = "true" ]; then
            echo -e "${YELLOW}--no-cache flag set, regenerating cache...${NC}"
            generate_cache "true"
            echo ""
        else
            ensure_cache
        fi
        run_tests "${REMAINING_ARGS[@]}"
        ;;
    analyze)
        analyze_tests "${REMAINING_ARGS[@]}"
        ;;
    areas)
        check_submodule_clean
        ensure_binaries
        if [ "$NO_CACHE" = "true" ]; then
            generate_cache "true"
        else
            ensure_cache
        fi
        areas_analysis "${REMAINING_ARGS[@]}"
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
        ensure_binaries
        if [ "$NO_CACHE" = "true" ]; then
            generate_cache "true"
        else
            ensure_cache
        fi
        echo ""
        run_tests "${REMAINING_ARGS[@]}"
        ;;
    snapshot)
        check_submodule_clean
        ensure_binaries
        if [ "$NO_CACHE" = "true" ]; then
            generate_cache "true"
        else
            ensure_cache
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
