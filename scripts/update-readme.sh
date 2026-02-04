#!/bin/bash
#
# Update README with conformance, fourslash, and/or emit test results
#
# Usage:
#   ./scripts/update-readme.sh                    # Run all and update README
#   ./scripts/update-readme.sh --commit           # Also commit and push
#   ./scripts/update-readme.sh --conformance-only # Only run conformance tests
#   ./scripts/update-readme.sh --fourslash-only   # Only run fourslash tests
#   ./scripts/update-readme.sh --emit-only        # Only run emit tests
#   ./scripts/update-readme.sh --max=500          # Limit conformance tests
#   ./scripts/update-readme.sh --fourslash-max=100 # Limit fourslash tests
#   ./scripts/update-readme.sh --emit-max=500     # Limit emit tests
#

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
README="$ROOT_DIR/README.md"

# Defaults
COMMIT=false
MAX_TESTS="--all"
FOURSLASH_MAX=""
EMIT_MAX=""
RUN_CONFORMANCE=true
RUN_FOURSLASH=true
RUN_EMIT=true

# Parse arguments
for arg in "$@"; do
    case $arg in
        --commit) COMMIT=true ;;
        --max=*) MAX_TESTS="--max=${arg#*=}" ;;
        --fourslash-max=*) FOURSLASH_MAX="--max=${arg#*=}" ;;
        --emit-max=*) EMIT_MAX="--max=${arg#*=}" ;;
        --conformance-only) RUN_FOURSLASH=false; RUN_EMIT=false ;;
        --fourslash-only) RUN_CONFORMANCE=false; RUN_EMIT=false ;;
        --emit-only) RUN_CONFORMANCE=false; RUN_FOURSLASH=false ;;
        --no-emit) RUN_EMIT=false ;;
        --help|-h)
            echo "Update README with conformance, fourslash, and emit test results"
            echo ""
            echo "Usage: ./scripts/update-readme.sh [options]"
            echo ""
            echo "Options:"
            echo "  --commit            Commit and push changes to git"
            echo "  --max=N             Limit conformance tests (default: all)"
            echo "  --fourslash-max=N   Limit fourslash tests (default: all)"
            echo "  --emit-max=N        Limit emit tests (default: 500)"
            echo "  --conformance-only  Only run conformance tests"
            echo "  --fourslash-only    Only run fourslash tests"
            echo "  --emit-only         Only run emit tests"
            echo "  --no-emit           Skip emit tests"
            echo "  --help              Show this help"
            exit 0
            ;;
    esac
done

cd "$ROOT_DIR"

# ── Helper functions ─────────────────────────────────────────────────

# Generate a progress bar: generate_bar <percent> <width>
generate_bar() {
    local percent=$1
    local width=${2:-15}
    local filled=$(echo "$percent * $width / 100" | bc)
    local empty=$((width - filled))
    local bar=""
    for ((i=0; i<filled; i++)); do bar+="█"; done
    for ((i=0; i<empty; i++)); do bar+="░"; done
    echo "$bar"
}

# Format number with commas: format_num 12345 -> 12,345
format_num() {
    printf "%'d" "$1" 2>/dev/null || echo "$1"
}

# Update a section in README between markers
# update_section <start_marker> <end_marker> <new_content>
update_section() {
    local start_marker="$1"
    local end_marker="$2"
    local new_content="$3"
    local tmpfile=$(mktemp)
    local content_file=$(mktemp)

    # Write content to temp file
    printf "%s\n" "$new_content" > "$content_file"

    awk -v start="$start_marker" -v end="$end_marker" '
        BEGIN { skip = 0 }
        $0 ~ start { print; skip = 1; next }
        $0 ~ end { skip = 0; getline; print ""; while ((getline line < "'"$content_file"'") > 0) print line; close("'$"$content_file"'"); next }
        !skip { print }
    ' "$README" > "$tmpfile"

    rm "$content_file"
    mv "$tmpfile" "$README"
}

# ── Get TypeScript version ───────────────────────────────────────────

TS_VERSION=$(node -e "const v = require('./scripts/typescript-versions.json'); const m = Object.values(v.mappings)[0]; console.log(m?.npm || v.default?.npm || 'unknown')")
echo "TypeScript version: $TS_VERSION"
echo ""

# Update TypeScript version in README
echo "Updating TypeScript version in README..."
update_section "<!-- TS_VERSION_START -->" "<!-- TS_VERSION_END -->" "Currently targeting \`TypeScript\`@\`$TS_VERSION\`"
echo ""

# Track what was updated for commit message
CONF_PASSED=""
CONF_TOTAL=""
CONF_PASS_RATE=""
FS_PASSED=""
FS_TOTAL=""
FS_PASS_RATE=""
EMIT_JS_PASSED=""
EMIT_JS_TOTAL=""
EMIT_JS_RATE=""
EMIT_DTS_PASSED=""
EMIT_DTS_TOTAL=""

# ── Conformance tests ─────────────────────────────────────────────
if [ "$RUN_CONFORMANCE" = true ]; then
    echo "============================================================"
    echo "         Conformance Tests"
    echo "============================================================"
    echo ""

    echo "Running conformance tests..."
    OUTPUT=$(./scripts/conformance.sh run $MAX_TESTS 2>&1) || true
    echo "$OUTPUT"

    # Parse new format: "FINAL RESULTS: N/N passed (N%)"
    if echo "$OUTPUT" | grep -q "FINAL RESULTS:"; then
        # Strip ANSI color codes before parsing
        RESULTS_LINE=$(echo "$OUTPUT" | grep "FINAL RESULTS:" | head -1 | sed 's/\x1b\[[0-9;]*m//g')
        # Format: "FINAL RESULTS: 568/1200 passed (47.3%)"
        CONF_PASSED=$(echo "$RESULTS_LINE" | sed -E 's/.*FINAL RESULTS:[[:space:]]*([0-9]+)\/([0-9]+).*/\1/')
        CONF_TOTAL=$(echo "$RESULTS_LINE" | sed -E 's/.*FINAL RESULTS:[[:space:]]*([0-9]+)\/([0-9]+).*/\2/')
        CONF_PASS_RATE=$(echo "$RESULTS_LINE" | sed -E 's/.*\(([0-9.]+)%\).*/\1/')

        # Validate parsed values are numbers
        if ! [[ "$CONF_PASS_RATE" =~ ^[0-9]+\.?[0-9]*$ ]] || ! [[ "$CONF_PASSED" =~ ^[0-9]+$ ]] || ! [[ "$CONF_TOTAL" =~ ^[0-9]+$ ]]; then
            echo "Failed to parse conformance test output (invalid values: rate=$CONF_PASS_RATE, passed=$CONF_PASSED, total=$CONF_TOTAL)"
            if [ "$RUN_FOURSLASH" = false ]; then
                exit 1
            fi
        else
            echo ""
            echo "Conformance: $CONF_PASSED/$CONF_TOTAL tests passed ($CONF_PASS_RATE%)"

            # Parse time from output: "Time: 14.7s"
            TIME_LINE=$(echo "$OUTPUT" | grep "Time:" | head -1 | sed 's/\x1b\[[0-9;]*m//g')
            CONF_TIME=$(echo "$TIME_LINE" | sed -E 's/.*Time:[[:space:]]*([0-9.]+)s.*/\1/')
            
            # Calculate tests/sec
            if [[ "$CONF_TIME" =~ ^[0-9.]+$ ]] && [ "$(echo "$CONF_TIME > 0" | bc)" -eq 1 ]; then
                TESTS_PER_SEC=$(echo "scale=0; $CONF_TOTAL / $CONF_TIME" | bc)
            else
                TESTS_PER_SEC="N/A"
                CONF_TIME="N/A"
            fi

            # Generate progress bar and update README
            BAR=$(generate_bar "$CONF_PASS_RATE")
            PASSED_FMT=$(format_num "$CONF_PASSED")
            TOTAL_FMT=$(format_num "$CONF_TOTAL")
            
            CONF_CONTENT="\`\`\`
Progress: [$BAR] ${CONF_PASS_RATE}% ($PASSED_FMT / $TOTAL_FMT tests)
Performance: $TESTS_PER_SEC tests/sec (${CONF_TIME}s for full suite)
\`\`\`

**Quick Start:**
\`\`\`bash
# Generate TSC cache (one-time setup)
./scripts/conformance.sh generate

# Run conformance tests
./scripts/conformance.sh run

# See details
./scripts/conformance.sh run --verbose --max 100
\`\`\`

**Implementation:** High-performance Rust runner with parallel execution
**Documentation:** [conformance-rust/README.md](conformance-rust/README.md) | [docs/CONFORMANCE_DEEP_DIVE.md](docs/CONFORMANCE_DEEP_DIVE.md)"

            update_section "<!-- CONFORMANCE_START -->" "<!-- CONFORMANCE_END -->" "$CONF_CONTENT"
        fi
    else
        echo "Failed to parse conformance test output"
        if [ "$RUN_FOURSLASH" = false ]; then
            exit 1
        fi
    fi
    echo ""
fi

# ── Fourslash / LSP tests ─────────────────────────────────────────
if [ "$RUN_FOURSLASH" = true ]; then
    echo "============================================================"
    echo "         Fourslash / Language Service Tests"
    echo "============================================================"
    echo ""

    echo "Running fourslash tests..."
    FS_OUTPUT=$(./scripts/run-fourslash.sh --skip-build $FOURSLASH_MAX 2>&1) || true
    echo "$FS_OUTPUT"

    # Parse: "Results: N passed, N failed out of N (Ns)"
    if echo "$FS_OUTPUT" | grep -q "^Results:"; then
        FS_PASSED=$(echo "$FS_OUTPUT" | grep "^Results:" | tail -1 | sed -E 's/Results:[[:space:]]*([0-9]+) passed.*/\1/')
        FS_RAN=$(echo "$FS_OUTPUT" | grep "^Results:" | tail -1 | sed -E 's/.*out of ([0-9]+).*/\1/')
        # Use total available if reported, otherwise use tests run
        if echo "$FS_OUTPUT" | grep -q "total available"; then
            FS_TOTAL=$(echo "$FS_OUTPUT" | grep "total available" | tail -1 | sed -E 's/.*([0-9]+) total available.*/\1/')
        else
            FS_TOTAL="$FS_RAN"
        fi
        FS_PASS_RATE=$(echo "$FS_OUTPUT" | grep "Pass rate:" | tail -1 | sed -E 's/.*Pass rate:[[:space:]]*([0-9.]+)%.*/\1/')

        echo ""
        echo "Fourslash: $FS_PASSED/$FS_TOTAL tests ($FS_PASS_RATE%)"

        # Generate progress bar and update README
        BAR=$(generate_bar "$FS_PASS_RATE" 20)
        PASSED_FMT=$(format_num "$FS_PASSED")
        TOTAL_FMT=$(format_num "$FS_TOTAL")
        
        FS_CONTENT="\`\`\`
Progress: [$BAR] ${FS_PASS_RATE}% ($PASSED_FMT / $TOTAL_FMT tests)
\`\`\`"

        update_section "<!-- FOURSLASH_START -->" "<!-- FOURSLASH_END -->" "$FS_CONTENT"
    else
        echo "Failed to parse fourslash test output"
        if [ "$RUN_CONFORMANCE" = false ]; then
            exit 1
        fi
    fi
    echo ""
fi

# ── Emit tests ─────────────────────────────────────────────────────
if [ "$RUN_EMIT" = true ]; then
    echo "============================================================"
    echo "         Emit Tests (JavaScript + Declaration)"
    echo "============================================================"
    echo ""

    echo "Running emit tests..."
    EMIT_OUTPUT=$(./scripts/emit/run.sh $EMIT_MAX --js-only 2>&1) || true
    echo "$EMIT_OUTPUT"

    # Parse: "Pass Rate: N% (N/N)"
    if echo "$EMIT_OUTPUT" | grep -q "Pass Rate:"; then
        EMIT_JS_PASSED=$(echo "$EMIT_OUTPUT" | grep "Pass Rate:" | head -1 | sed -E 's/.*\(([0-9]+)\/([0-9]+)\).*/\1/')
        EMIT_JS_TOTAL=$(echo "$EMIT_OUTPUT" | grep "Pass Rate:" | head -1 | sed -E 's/.*\(([0-9]+)\/([0-9]+)\).*/\2/')
        EMIT_JS_RATE=$(echo "$EMIT_OUTPUT" | grep "Pass Rate:" | head -1 | sed -E 's/.*Pass Rate:[[:space:]]*([0-9.]+)%.*/\1/')

        # DTS not implemented yet, set to 0
        EMIT_DTS_PASSED=0
        EMIT_DTS_TOTAL=0

        echo ""
        echo "Emit JS: $EMIT_JS_PASSED/$EMIT_JS_TOTAL ($EMIT_JS_RATE%)"

        # Generate progress bars and update README
        JS_BAR=$(generate_bar "$EMIT_JS_RATE" 20)
        JS_PASSED_FMT=$(format_num "$EMIT_JS_PASSED")
        JS_TOTAL_FMT=$(format_num "$EMIT_JS_TOTAL")
        
        EMIT_CONTENT="\`\`\`
JavaScript:  [$JS_BAR] ${EMIT_JS_RATE}% ($JS_PASSED_FMT / $JS_TOTAL_FMT tests)
Declaration: [░░░░░░░░░░░░░░░░░░░░] 0.0% (0 / 0 tests)
\`\`\`"

        update_section "<!-- EMIT_START -->" "<!-- EMIT_END -->" "$EMIT_CONTENT"
    else
        echo "Failed to parse emit test output"
    fi
    echo ""
fi

# ── Commit ─────────────────────────────────────────────────────────
if git diff --quiet README.md; then
    echo "README.md is already up to date"
else
    echo "README.md updated"

    if [ "$COMMIT" = true ]; then
        # Build commit message
        MSG="docs: update README progress"
        DETAILS=""
        if [ -n "$CONF_PASSED" ]; then
            DETAILS="${DETAILS}Conformance: ${CONF_PASS_RATE}% (${CONF_PASSED}/${CONF_TOTAL})\n"
        fi
        if [ -n "$FS_PASSED" ]; then
            DETAILS="${DETAILS}Fourslash: ${FS_PASS_RATE}% (${FS_PASSED}/${FS_TOTAL})\n"
        fi
        if [ -n "$EMIT_JS_PASSED" ]; then
            DETAILS="${DETAILS}Emit JS: ${EMIT_JS_RATE}% (${EMIT_JS_PASSED}/${EMIT_JS_TOTAL})\n"
        fi
        DETAILS="${DETAILS}TypeScript: ${TS_VERSION}"

        echo ""
        echo "Committing and pushing..."
        git add README.md
        git commit -m "$MSG

$(echo -e "$DETAILS")"
        git push
        echo "Pushed to remote"
    else
        echo ""
        echo "Run with --commit to commit and push changes"
        echo "Or manually: git add README.md && git commit -m 'docs: update progress'"
    fi
fi

echo ""
echo "Done!"
