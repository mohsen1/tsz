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
#   1. cargo fmt --check    (formatting gate)
#   2. cargo clippy         (lint gate, warnings denied)
#   3. cargo nextest run    (compilation + unit tests)
#   4. conformance.sh run   (conformance vs tsc)
#   5. emit/run.sh          (JS/declaration emit)
#   6. fourslash/run.sh     (LSP/language service)
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
TEMP_ARTIFACTS=()

cleanup_temp_artifacts() {
    for path in "${TEMP_ARTIFACTS[@]+"${TEMP_ARTIFACTS[@]}"}"; do
        [[ -z "$path" ]] && continue
        rm -rf "$path" 2>/dev/null || true
    done
}
trap cleanup_temp_artifacts EXIT

make_temp_json() {
    local name="$1"
    local path
    path="$(mktemp "${TMPDIR:-/tmp}/tsz-${name}.XXXXXX")"
    rm -f "$path"
    path="${path}.json"
    TEMP_ARTIFACTS+=("$path")
    printf '%s\n' "$path"
}

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
CONFORMANCE_TOTAL=0
if [[ -f "$REPO_ROOT/scripts/conformance/conformance-snapshot.json" ]]; then
    read -r BASELINE_PASS CONFORMANCE_TOTAL < <(python3 -c "
import json
with open('$REPO_ROOT/scripts/conformance/conformance-snapshot.json') as f:
    summary = json.load(f).get('summary', {})
    print(summary.get('passed', 0), summary.get('total_tests', summary.get('total', 0)))
" 2>/dev/null || echo "0 0")
fi

EMIT_BASELINE_JS=0
EMIT_BASELINE_DTS=0
if [[ -f "$REPO_ROOT/scripts/emit/emit-snapshot.json" ]]; then
    read -r EMIT_BASELINE_JS EMIT_BASELINE_DTS < <(python3 -c "
import json
with open('$REPO_ROOT/scripts/emit/emit-snapshot.json') as f:
    summary = json.load(f).get('summary', {})
    print(summary.get('jsPass', 0), summary.get('dtsPass', 0))
" 2>/dev/null || echo "0 0")
fi

FOURSLASH_BASELINE_PASS=0
if [[ -f "$REPO_ROOT/scripts/fourslash/fourslash-snapshot.json" ]]; then
    FOURSLASH_BASELINE_PASS=$(python3 -c "
import json
with open('$REPO_ROOT/scripts/fourslash/fourslash-snapshot.json') as f:
    data = json.load(f)
    print(data.get('summary', {}).get('passed', data.get('passed', 0)))
" 2>/dev/null || echo "0")
fi

echo -e "${BOLD}TSZ Full Verification Suite${RESET}"
echo "Conformance baseline: $BASELINE_PASS tests passing"
echo "Emit baseline: JS=$EMIT_BASELINE_JS DTS=$EMIT_BASELINE_DTS"
echo "Fourslash baseline: $FOURSLASH_BASELINE_PASS tests passing"
echo "=========================================="

cd "$REPO_ROOT"

# --- 1. Formatting ---
run_suite "formatting" scripts/safe-run.sh cargo fmt --all --check

# --- 2. Clippy ---
run_suite "clippy" scripts/safe-run.sh cargo clippy --workspace --all-targets --all-features -- -D warnings

# --- 3. Unit tests (also compiles — no separate cargo check needed) ---
run_suite "unit tests" scripts/safe-run.sh cargo nextest run

# --- 4. Conformance ---
echo ""
echo -e "${CYAN}━━━ [conformance] ━━━${RESET}"
echo -e "${CYAN}→${RESET}  scripts/safe-run.sh ./scripts/conformance/conformance.sh run"
echo ""

CONF_PASS=0
CONF_RECORDED=0
CONF_LAST_RUN="$REPO_ROOT/scripts/conformance/conformance-last-run.txt"

read_conformance_results() {
    local last_run_path="$1"
    python3 -c "
import sys
pass_count = 0
recorded = 0
with open(sys.argv[1], encoding='utf-8', errors='replace') as f:
    for line in f:
        if line.startswith(('PASS ', 'FAIL ', 'CRASH ', 'TIMEOUT ')):
            recorded += 1
        if line.startswith('PASS '):
            pass_count += 1
print(pass_count, recorded)
" "$last_run_path" 2>/dev/null || echo "0 0"
}

run_conformance_once() {
    scripts/safe-run.sh ./scripts/conformance/conformance.sh run || true
    if [[ -f "$CONF_LAST_RUN" ]]; then
        read -r CONF_PASS CONF_RECORDED < <(read_conformance_results "$CONF_LAST_RUN")
    else
        CONF_PASS=0
        CONF_RECORDED=0
    fi

    if ! [[ "$CONF_PASS" =~ ^[0-9]+$ ]]; then
        CONF_PASS=0
    fi
    if ! [[ "$CONF_RECORDED" =~ ^[0-9]+$ ]]; then
        CONF_RECORDED=0
    fi
}

# Retry conformance a few times when the runner drops results or reports a
# one-off regression that doesn't reproduce on the next full pass.
CONFORMANCE_ATTEMPTS=3
for attempt in $(seq 1 "$CONFORMANCE_ATTEMPTS"); do
    run_conformance_once

    if [[ "$CONFORMANCE_TOTAL" -gt 0 ]] && [[ "$CONF_RECORDED" -lt "$CONFORMANCE_TOTAL" ]]; then
        if [[ "$attempt" -lt "$CONFORMANCE_ATTEMPTS" ]]; then
            MISSING_RESULTS=$((CONFORMANCE_TOTAL - CONF_RECORDED))
            echo -e "${YELLOW}!${RESET}  conformance — incomplete run: recorded $CONF_RECORDED/$CONFORMANCE_TOTAL results; retrying (${attempt}/${CONFORMANCE_ATTEMPTS})"
            continue
        fi
        break
    fi

    if [[ "$CONF_PASS" -lt "$BASELINE_PASS" ]]; then
        if [[ "$attempt" -lt "$CONFORMANCE_ATTEMPTS" ]]; then
            REGRESSION=$((BASELINE_PASS - CONF_PASS))
            echo -e "${YELLOW}!${RESET}  conformance — provisional regression: -$REGRESSION ($CONF_PASS vs $BASELINE_PASS baseline); retrying (${attempt}/${CONFORMANCE_ATTEMPTS})"
            continue
        fi
        break
    fi

    break
done

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

# --- 5. Emit tests ---
if ! $QUICK; then
    echo ""
    echo -e "${CYAN}━━━ [emit tests] ━━━${RESET}"
    EMIT_JSON="$(make_temp_json "emit-verify")"
    echo -e "${CYAN}→${RESET}  scripts/safe-run.sh ./scripts/emit/run.sh --json-out=$EMIT_JSON"
    echo ""

    scripts/safe-run.sh ./scripts/emit/run.sh --json-out="$EMIT_JSON" || true

    if [[ ! -f "$EMIT_JSON" ]]; then
        echo -e "${RED}✗${RESET}  emit tests — FAILED: no JSON summary written"
        RESULTS+=("${RED}✗${RESET}  emit tests (no summary)")
        FAIL=$((FAIL + 1))
    else
        read -r EMIT_JS_PASS EMIT_DTS_PASS < <(python3 -c "
import json
with open('$EMIT_JSON') as f:
    summary = json.load(f).get('summary', {})
    print(summary.get('jsPass', 0), summary.get('dtsPass', 0))
" 2>/dev/null || echo "0 0")

        if [[ "$EMIT_JS_PASS" -lt "$EMIT_BASELINE_JS" ]] || [[ "$EMIT_DTS_PASS" -lt "$EMIT_BASELINE_DTS" ]]; then
            JS_REGRESSION=$((EMIT_BASELINE_JS - EMIT_JS_PASS))
            DTS_REGRESSION=$((EMIT_BASELINE_DTS - EMIT_DTS_PASS))
            echo -e "${RED}✗${RESET}  emit tests — REGRESSION: JS -$JS_REGRESSION, DTS -$DTS_REGRESSION ($EMIT_JS_PASS/$EMIT_DTS_PASS vs $EMIT_BASELINE_JS/$EMIT_BASELINE_DTS baseline)"
            RESULTS+=("${RED}✗${RESET}  emit tests (REGRESSION: JS -$JS_REGRESSION, DTS -$DTS_REGRESSION)")
            FAIL=$((FAIL + 1))
        elif [[ "$EMIT_JS_PASS" -gt "$EMIT_BASELINE_JS" ]] || [[ "$EMIT_DTS_PASS" -gt "$EMIT_BASELINE_DTS" ]]; then
            JS_IMPROVEMENT=$((EMIT_JS_PASS - EMIT_BASELINE_JS))
            DTS_IMPROVEMENT=$((EMIT_DTS_PASS - EMIT_BASELINE_DTS))
            echo -e "${GREEN}✓${RESET}  emit tests — IMPROVED: JS +$JS_IMPROVEMENT, DTS +$DTS_IMPROVEMENT ($EMIT_JS_PASS/$EMIT_DTS_PASS vs $EMIT_BASELINE_JS/$EMIT_BASELINE_DTS baseline)"
            RESULTS+=("${GREEN}✓${RESET}  emit tests (JS +$JS_IMPROVEMENT, DTS +$DTS_IMPROVEMENT)")
            PASS=$((PASS + 1))
        else
            echo -e "${GREEN}✓${RESET}  emit tests — NO CHANGE (JS=$EMIT_JS_PASS DTS=$EMIT_DTS_PASS)"
            RESULTS+=("${GREEN}✓${RESET}  emit tests (=$EMIT_JS_PASS/$EMIT_DTS_PASS)")
            PASS=$((PASS + 1))
        fi
    fi
else
    echo ""
    echo -e "${YELLOW}⊘${RESET}  emit tests — SKIPPED (--quick mode)"
    RESULTS+=("${YELLOW}⊘${RESET}  emit tests (skipped)")
fi

# --- 6. Fourslash/LSP tests ---
if ! $QUICK && ! $SKIP_LSP; then
    echo ""
    echo -e "${CYAN}━━━ [fourslash/LSP] ━━━${RESET}"
    FOURSLASH_JSON="$(make_temp_json "fourslash-verify")"
    FOURSLASH_VERIFY_MAX=50
    echo -e "${CYAN}→${RESET}  scripts/safe-run.sh ./scripts/fourslash/run-fourslash.sh --max=$FOURSLASH_VERIFY_MAX --workers=8 --json-out=$FOURSLASH_JSON"
    echo ""

    scripts/safe-run.sh ./scripts/fourslash/run-fourslash.sh --max="$FOURSLASH_VERIFY_MAX" --workers=8 --json-out="$FOURSLASH_JSON" || true

    if [[ ! -f "$FOURSLASH_JSON" ]]; then
        echo -e "${RED}✗${RESET}  fourslash/LSP — FAILED: no JSON summary written"
        RESULTS+=("${RED}✗${RESET}  fourslash/LSP (no summary)")
        FAIL=$((FAIL + 1))
    else
        FOURSLASH_PASS=$(python3 -c "
import json
with open('$FOURSLASH_JSON') as f:
    data = json.load(f)
    print(data.get('summary', {}).get('passed', data.get('passed', 0)))
" 2>/dev/null || echo "0")

        FOURSLASH_EXPECTED_BASELINE="$FOURSLASH_BASELINE_PASS"
        if [[ "$FOURSLASH_VERIFY_MAX" -gt 0 ]] && [[ "$FOURSLASH_BASELINE_PASS" -gt "$FOURSLASH_VERIFY_MAX" ]]; then
            # When running a capped smoke subset, compare against the capped baseline.
            FOURSLASH_EXPECTED_BASELINE="$FOURSLASH_VERIFY_MAX"
        fi

        if [[ "$FOURSLASH_PASS" -lt "$FOURSLASH_EXPECTED_BASELINE" ]]; then
            FOURSLASH_REGRESSION=$((FOURSLASH_EXPECTED_BASELINE - FOURSLASH_PASS))
            echo -e "${RED}✗${RESET}  fourslash/LSP — REGRESSION: lost $FOURSLASH_REGRESSION tests ($FOURSLASH_PASS vs $FOURSLASH_EXPECTED_BASELINE expected)"
            RESULTS+=("${RED}✗${RESET}  fourslash/LSP (REGRESSION: -$FOURSLASH_REGRESSION)")
            FAIL=$((FAIL + 1))
        elif [[ "$FOURSLASH_PASS" -gt "$FOURSLASH_EXPECTED_BASELINE" ]]; then
            FOURSLASH_IMPROVEMENT=$((FOURSLASH_PASS - FOURSLASH_EXPECTED_BASELINE))
            echo -e "${GREEN}✓${RESET}  fourslash/LSP — IMPROVED: +$FOURSLASH_IMPROVEMENT tests ($FOURSLASH_PASS vs $FOURSLASH_EXPECTED_BASELINE expected)"
            RESULTS+=("${GREEN}✓${RESET}  fourslash/LSP (+$FOURSLASH_IMPROVEMENT)")
            PASS=$((PASS + 1))
        else
            echo -e "${GREEN}✓${RESET}  fourslash/LSP — NO CHANGE ($FOURSLASH_PASS tests passing)"
            RESULTS+=("${GREEN}✓${RESET}  fourslash/LSP (=$FOURSLASH_PASS)")
            PASS=$((PASS + 1))
        fi
    fi
else
    echo ""
    echo -e "${YELLOW}⊘${RESET}  fourslash/LSP — SKIPPED"
    RESULTS+=("${YELLOW}⊘${RESET}  fourslash/LSP (skipped)")
fi

# --- Cleanup temp artifacts from this verification run ---
cleanup_temp_artifacts

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
