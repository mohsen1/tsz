#!/usr/bin/env bash
# =============================================================================
# verify-all.sh — Run ALL test suites and verify zero regressions
# =============================================================================
#
# Usage:
#   scripts/session/verify-all.sh              # Run all suites
#   scripts/session/verify-all.sh --quick      # Skip emit and fourslash
#   scripts/session/verify-all.sh --skip-lsp   # Skip fourslash only
#
# Runs in order:
#   1. cargo nextest run    (compilation + unit tests)
#   2. conformance.sh run   (conformance vs tsc)
#   3. emit/run.sh          (JS/declaration emit)
#   4. fourslash/run.sh     (LSP/language service)
#
# Exits non-zero if ANY suite fails or conformance regresses.
#
# =============================================================================
set -uo pipefail
# Note: NOT using set -e so we can run all suites and report aggregate results.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"

if [[ -t 1 ]]; then
    RED='\033[0;31m' GREEN='\033[0;32m' YELLOW='\033[0;33m'
    CYAN='\033[0;36m' BOLD='\033[1m' RESET='\033[0m'
else
    RED='' GREEN='' YELLOW='' CYAN='' BOLD='' RESET=''
fi

QUICK=false
SKIP_LSP=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --quick) QUICK=true; shift ;;
        --skip-lsp) SKIP_LSP=true; shift ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
done

PASS=0
FAIL=0
RESULTS=()

run_suite() {
    local name="$1"
    shift
    echo ""
    echo -e "${CYAN}━━━ [$name] ━━━${RESET}"
    echo -e "${CYAN}→${RESET}  $*"
    echo ""

    if "$@"; then
        echo -e "${GREEN}✓${RESET}  $name — PASSED"
        RESULTS+=("${GREEN}✓${RESET}  $name")
        PASS=$((PASS + 1))
    else
        echo -e "${RED}✗${RESET}  $name — FAILED"
        RESULTS+=("${RED}✗${RESET}  $name")
        FAIL=$((FAIL + 1))
    fi
}

# --- Get conformance baseline ---
BASELINE_PASS=0
if [[ -f "$REPO_ROOT/scripts/conformance/conformance-snapshot.json" ]]; then
    BASELINE_PASS=$(python3 -c "
import json
with open('$REPO_ROOT/scripts/conformance/conformance-snapshot.json') as f:
    print(json.load(f).get('summary', {}).get('passed', 0))
" 2>/dev/null || echo "0")
fi

echo -e "${BOLD}TSZ Full Verification Suite${RESET}"
echo "Conformance baseline: $BASELINE_PASS tests passing"
echo "=========================================="

cd "$REPO_ROOT"

# --- 1. Unit tests (also compiles — no separate cargo check needed) ---
run_suite "unit tests" scripts/safe-run.sh cargo nextest run

# --- 2. Conformance ---
echo ""
echo -e "${CYAN}━━━ [conformance] ━━━${RESET}"
echo -e "${CYAN}→${RESET}  scripts/safe-run.sh ./scripts/conformance/conformance.sh run"
echo ""

# Capture stdout only (stderr flows to terminal naturally)
CONF_PASS=0
CONF_OUTPUT=$(scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>/dev/null) || true
CONF_PASS=$(echo "$CONF_OUTPUT" | sed 's/\x1b\[[0-9;]*m//g' | grep -Eo '[0-9]+/[0-9]+ passed' | grep -Eo '^[0-9]+' || echo "0")

# Validate it's a number
if ! [[ "$CONF_PASS" =~ ^[0-9]+$ ]]; then
    CONF_PASS=0
fi

if [[ "$CONF_PASS" -lt "$BASELINE_PASS" ]]; then
    REGRESSION=$((BASELINE_PASS - CONF_PASS))
    echo -e "${RED}✗${RESET}  conformance — REGRESSION: lost $REGRESSION tests ($CONF_PASS vs $BASELINE_PASS baseline)"
    RESULTS+=("${RED}✗${RESET}  conformance (REGRESSION: -$REGRESSION)")
    FAIL=$((FAIL + 1))
elif [[ "$CONF_PASS" -gt "$BASELINE_PASS" ]]; then
    IMPROVEMENT=$((CONF_PASS - BASELINE_PASS))
    echo -e "${GREEN}✓${RESET}  conformance — IMPROVED: +$IMPROVEMENT tests ($CONF_PASS vs $BASELINE_PASS baseline)"
    RESULTS+=("${GREEN}✓${RESET}  conformance (+$IMPROVEMENT)")
    PASS=$((PASS + 1))
else
    echo -e "${GREEN}✓${RESET}  conformance — NO CHANGE ($CONF_PASS tests passing)"
    RESULTS+=("${GREEN}✓${RESET}  conformance (=$CONF_PASS)")
    PASS=$((PASS + 1))
fi

# --- 3. Emit tests ---
if ! $QUICK; then
    run_suite "emit tests" scripts/safe-run.sh ./scripts/emit/run.sh
else
    echo ""
    echo -e "${YELLOW}⊘${RESET}  emit tests — SKIPPED (--quick mode)"
    RESULTS+=("${YELLOW}⊘${RESET}  emit tests (skipped)")
fi

# --- 4. Fourslash/LSP tests ---
if ! $QUICK && ! $SKIP_LSP; then
    run_suite "fourslash/LSP" scripts/safe-run.sh ./scripts/fourslash/run-fourslash.sh --max=50
else
    echo ""
    echo -e "${YELLOW}⊘${RESET}  fourslash/LSP — SKIPPED"
    RESULTS+=("${YELLOW}⊘${RESET}  fourslash/LSP (skipped)")
fi

# --- Cleanup temp artifacts from test runs ---
rm -rf /tmp/tsz-* /tmp/tmp.* 2>/dev/null || true

# --- Summary ---
echo ""
echo "=========================================="
echo -e "${BOLD}Verification Summary${RESET}"
echo "=========================================="
for r in "${RESULTS[@]}"; do
    echo -e "  $r"
done
echo ""
echo "  Passed: $PASS  Failed: $FAIL"
echo "  Conformance: $CONF_PASS (baseline: $BASELINE_PASS)"
echo "=========================================="

if [[ $FAIL -gt 0 ]]; then
    echo ""
    echo -e "${RED}${BOLD}VERIFICATION FAILED — DO NOT PUSH${RESET}"
    exit 1
else
    echo ""
    echo -e "${GREEN}${BOLD}ALL SUITES PASSED — safe to push${RESET}"
    exit 0
fi
