#!/bin/bash
# Session script: conformance test improvement mission

cat >&2 <<'EOF'
IMPORTANT: Read docs/HOW_TO_CODE.md before writing any code. It covers architecture
rules, coding patterns, recursion safety, testing, and debugging conventions.

═══════════════════════════════════════════════════════════
MISSION: Pass the second 100 conformance tests (offset 100)
═══════════════════════════════════════════════════════════

Your goal is to maximize the pass rate for conformance tests 100-199.
These are the tests you get with --max=100 --offset=100.

═══════════════════════════════════════════════════════════
RUNNING CONFORMANCE TESTS
═══════════════════════════════════════════════════════════

First, build the binary:
  cargo build --profile dist-fast -p tsz-cli

Run your slice (second 100 tests):
  ./scripts/conformance.sh run --max=100 --offset=100

Run with verbose output to see per-test results:
  ./scripts/conformance.sh run --max=100 --offset=100 --verbose

Analyze failures in your slice:
  ./scripts/conformance.sh analyze --max=100 --offset=100

Analyze specific categories:
  ./scripts/conformance.sh analyze --max=100 --offset=100 --category false-positive
  ./scripts/conformance.sh analyze --max=100 --offset=100 --category all-missing
  ./scripts/conformance.sh analyze --max=100 --offset=100 --category wrong-code
  ./scripts/conformance.sh analyze --max=100 --offset=100 --category close

Filter by error code:
  ./scripts/conformance.sh run --max=100 --offset=100 --error-code 2322

═══════════════════════════════════════════════════════════
WORKFLOW
═══════════════════════════════════════════════════════════

1. Run the conformance tests for your slice to see current pass rate
2. Analyze failures to find the highest-impact error codes to fix
3. Focus on "close" category first — tests that differ by 1-2 errors
4. Pick a specific failing test, run with --verbose to see what's wrong
5. Create a minimal .ts reproduction in tmp/
6. Run: ./target/dist-fast/tsz tmp/test.ts 2>&1
7. Compare output with expected (TSC baseline in cache)
8. Fix the checker/solver/emitter code
9. Re-run conformance tests to verify improvement
10. Run cargo nextest run to ensure no unit test regressions
11. Commit with clear message
12. MANDATORY — sync after EVERY commit:
      git pull --rebase origin main && git push origin main

═══════════════════════════════════════════════════════════
STRATEGY
═══════════════════════════════════════════════════════════

Focus on fixes that are GENERAL (help many tests) rather than narrow one-offs.
The analyze command shows which error codes have the most impact.

Priority order:
  1. "close" tests (differ by 1-2 errors) — easiest wins
  2. "false-positive" errors — we emit errors TSC doesn't
  3. "all-missing" errors — we miss entire error codes TSC emits
  4. "wrong-code" errors — we emit wrong error codes

Do NOT break existing passing tests. Always verify with cargo nextest run.

═══════════════════════════════════════════════════════════
KEY CODE LOCATIONS
═══════════════════════════════════════════════════════════

  Checker entry:    crates/tsz-checker/src/checker/mod.rs
  Checker state:    crates/tsz-checker/src/checker/state.rs
  Type checking:    crates/tsz-checker/src/checker/type_checking.rs
  Declarations:     crates/tsz-checker/src/checker/declaration_checker.rs
  Solver:           crates/tsz-checker/src/solver/
  Subtype checks:   crates/tsz-checker/src/solver/subtype.rs
  Type computation: crates/tsz-checker/src/checker/type_computation_complex.rs
  Diagnostics:      crates/tsz-common/src/diagnostics.rs
  Parser:           crates/tsz-parser/src/
  Binder:           crates/tsz-binder/src/
  CLI driver:       crates/tsz-cli/src/driver.rs
EOF

exit 2
