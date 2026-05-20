#!/usr/bin/env bash
# Architecture Health Check
#
# Runs architecture contract tests and outputs a summary report.
# Designed to be fast (< 5 seconds) using grep for static analysis.
#
# Usage:
#   scripts/architecture-check.sh          # full check (tests + static analysis)
#   scripts/architecture-check.sh --quick  # static analysis only (no cargo test)

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CHECKER_SRC="$REPO_ROOT/crates/tsz-checker/src"
BINDER_SRC="$REPO_ROOT/crates/tsz-binder/src"
EMITTER_SRC="$REPO_ROOT/crates/tsz-emitter/src"
SCANNER_SRC="$REPO_ROOT/crates/tsz-scanner/src"
PARSER_SRC="$REPO_ROOT/crates/tsz-parser/src"

QUICK_MODE=false
if [[ "${1:-}" == "--quick" ]]; then
    QUICK_MODE=true
fi

echo "Architecture Health Report"
echo "=========================="
echo ""

# --- Static Analysis (fast, grep-based) ---

# Helper: count grep matches, returning 0 if no matches
count_grep() {
    grep "$@" 2>/dev/null | wc -l | tr -d ' '
}

# 1. LOC violations: checker files over 2000 non-empty, non-comment lines
loc_violations=0
while IFS= read -r file; do
    bname=$(basename "$file")
    case "$bname" in
        mod.rs|*_tests.rs|test_utils.rs) continue ;;
    esac
    case "$file" in
        */tests/*) continue ;;
    esac

    loc=$(grep -cvE '^\s*$|^\s*//' "$file" 2>/dev/null; true)
    if [ "$loc" -gt 2000 ]; then
        loc_violations=$((loc_violations + 1))
        echo "  LOC: ${file#$REPO_ROOT/} ($loc lines)"
    fi
done <<EOF
$(find "$CHECKER_SRC" -name '*.rs' -type f)
EOF

# 2. Boundary bypasses: direct tsz_solver imports outside query_boundaries
direct_imports_file=$(mktemp)
grep -rl 'use tsz_solver' "$CHECKER_SRC" --include='*.rs' 2>/dev/null | grep -v '/tests/' | grep -v '/query_boundaries/' > "$direct_imports_file" || true
direct_imports=$(wc -l < "$direct_imports_file" | tr -d ' ')
rm -f "$direct_imports_file"

boundary_file=$(mktemp)
grep -rl 'query_boundaries::' "$CHECKER_SRC" --include='*.rs' 2>/dev/null | grep -v '/tests/' > "$boundary_file" || true
boundary_users=$(wc -l < "$boundary_file" | tr -d ' ')
rm -f "$boundary_file"

# 3. Diagnostic leaks: push_diagnostic outside error_reporter
leak_file=$(mktemp)
grep -rn 'push_diagnostic(' "$CHECKER_SRC" --include='*.rs' 2>/dev/null | grep -v '/error_reporter/' | grep -v '/tests/' | grep -v 'context/core.rs' | grep -v '^\s*//' > "$leak_file" || true
diagnostic_leaks=$(wc -l < "$leak_file" | tr -d ' ')
rm -f "$leak_file"

# 4. Code smells: TODO/FIXME/HACK markers
smells_file=$(mktemp)
grep -rn 'TODO\|FIXME\|HACK' "$CHECKER_SRC" --include='*.rs' 2>/dev/null | grep -v '/tests/' > "$smells_file" || true
code_smells=$(wc -l < "$smells_file" | tr -d ' ')
rm -f "$smells_file"

# 5. Cross-layer violations (exclude comment-only references: lines starting with // or ///)
count_non_comment_grep() {
    local pattern="$1"
    local dir="$2"
    # Find files containing the pattern on non-comment lines
    local count=0
    while IFS= read -r file; do
        # Check if the pattern appears on any non-comment line
        if grep -v '^\s*//' "$file" 2>/dev/null | grep -q "$pattern"; then
            count=$((count + 1))
        fi
    done < <(grep -rl "$pattern" "$dir" --include='*.rs' 2>/dev/null || true)
    echo "$count"
}
binder_solver=$(count_non_comment_grep 'tsz_solver' "$BINDER_SRC")
emitter_checker=$(count_non_comment_grep 'tsz_checker' "$EMITTER_SRC")
scanner_downstream=$(count_non_comment_grep 'tsz_\(parser\|binder\|checker\|solver\|emitter\)' "$SCANNER_SRC")
parser_downstream=$(count_non_comment_grep 'tsz_\(binder\|checker\|solver\|emitter\)' "$PARSER_SRC")

cross_layer=$((binder_solver + emitter_checker + scanner_downstream + parser_downstream))

# --- Print Report ---

printf "LOC violations:       %3d files over 2000 LOC (target: 0)\n" "$loc_violations"
printf "Boundary bypasses:    %3d direct solver imports (boundary users: %d)\n" "$direct_imports" "$boundary_users"
printf "Diagnostic leaks:     %3d calls outside error_reporter (target: 0)\n" "$diagnostic_leaks"
printf "Cross-layer imports:  %3d violations (target: 0)\n" "$cross_layer"
printf "Code smells:          %3d TODO/FIXME/HACK markers\n" "$code_smells"

# --- Run cargo tests (unless --quick) ---

test_result="SKIPPED"
test_exit=0
if [ "$QUICK_MODE" = false ]; then
    echo ""
    echo "Running architecture contract tests..."
    if cargo test -p tsz-checker -- architecture 2>&1 | tail -5; then
        test_result="PASS"
    else
        test_result="FAIL"
        test_exit=1
    fi
fi

echo ""
printf "Test result:          %s\n" "$test_result"
echo ""

# --- Exit code ---

if [ "$cross_layer" -gt 0 ]; then
    echo "FAIL: Cross-layer import violations detected."
    exit 1
fi

if [ "$test_exit" -ne 0 ]; then
    echo "FAIL: Architecture contract tests failed."
    exit 1
fi

if [ "$loc_violations" -gt 0 ]; then
    echo "WARNING: $loc_violations file(s) exceed 2000 LOC (grandfathered files are tracked in tests)."
fi

if [ "$diagnostic_leaks" -gt 0 ]; then
    echo "WARNING: $diagnostic_leaks diagnostic leak(s) detected."
fi

exit 0
