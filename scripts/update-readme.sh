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

# Defaults
COMMIT=false
MAX_TESTS="--all"
FOURSLASH_MAX=""
EMIT_MAX="--max=500"
WORKERS=8
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
        --workers=*) WORKERS="${arg#*=}" ;;
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
            echo "  --workers=N         Number of conformance workers (default: 8)"
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

# Get TypeScript version
TS_VERSION=$(node -e "const v = require('./scripts/conformance/typescript-versions.json'); const m = Object.values(v.mappings)[0]; console.log(m?.npm || v.default?.npm || 'unknown')")
echo "TypeScript version: $TS_VERSION"
echo ""

# Build update-readme tool
echo "Building update-readme tool..."
cd scripts/conformance
npm run build --silent 2>/dev/null || npm run build
cd "$ROOT_DIR"

# Track what was updated for commit message
CONF_PASSED=""
CONF_TOTAL=""
CONF_PASS_RATE=""
FS_PASSED=""
FS_TOTAL=""
FS_PASS_RATE=""
EMIT_JS_PASSED=""
EMIT_JS_TOTAL=""
EMIT_DTS_PASSED=""
EMIT_DTS_TOTAL=""

# ── Conformance tests ─────────────────────────────────────────────
if [ "$RUN_CONFORMANCE" = true ]; then
    echo "============================================================"
    echo "         Conformance Tests"
    echo "============================================================"
    echo ""

    echo "Running conformance tests..."
    OUTPUT=$(./scripts/conformance/run.sh $MAX_TESTS --workers=$WORKERS 2>&1) || true
    echo "$OUTPUT"

    if echo "$OUTPUT" | grep -q "Pass Rate:"; then
        CONF_PASS_RATE=$(echo "$OUTPUT" | grep "Pass Rate:" | tail -1 | sed -E 's/.*Pass Rate:[[:space:]]*([0-9.]+)%.*/\1/')
        CONF_PASSED=$(echo "$OUTPUT" | grep "Pass Rate:" | tail -1 | sed -E 's/.*\(([0-9,]+)\/([0-9,]+)\).*/\1/' | tr -d ',')
        CONF_TOTAL=$(echo "$OUTPUT" | grep "Pass Rate:" | tail -1 | sed -E 's/.*\(([0-9,]+)\/([0-9,]+)\).*/\2/' | tr -d ',')

        echo ""
        echo "Conformance: $CONF_PASSED/$CONF_TOTAL tests passed ($CONF_PASS_RATE%)"

        cd scripts/conformance
        node dist/update-readme.js \
            --passed="$CONF_PASSED" \
            --total="$CONF_TOTAL" \
            --pass-rate="$CONF_PASS_RATE" \
            --ts-version="$TS_VERSION"
        cd "$ROOT_DIR"
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

        cd scripts/conformance
        node dist/update-readme.js \
            --fourslash \
            --passed="$FS_PASSED" \
            --total="$FS_TOTAL" \
            --pass-rate="$FS_PASS_RATE" \
            --ts-version="$TS_VERSION"
        cd "$ROOT_DIR"
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

        cd scripts/conformance
        node dist/update-readme.js \
            --emit \
            --js-passed="$EMIT_JS_PASSED" \
            --js-total="$EMIT_JS_TOTAL" \
            --dts-passed="$EMIT_DTS_PASSED" \
            --dts-total="$EMIT_DTS_TOTAL"
        cd "$ROOT_DIR"
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
