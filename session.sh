#!/bin/bash
# Session script: type relation/inference engine parity mission

cat >&2 <<'EOF'
IMPORTANT: Read docs/HOW_TO_CODE.md before writing any code. It covers architecture
rules, coding patterns, recursion safety, testing, and debugging conventions.

═══════════════════════════════════════════════════════════
MISSION: Type Relation / Inference Engine Parity with TSC
═══════════════════════════════════════════════════════════

Your goal is to close the gap between tsz and tsc in the core type system:
  - Generics (inference, constraints, defaults)
  - Contextual typing (callback params, function expressions, return types)
  - Mapped types (homomorphic, key remapping, array preservation)
  - Conditional types (evaluation, distribution, infer patterns)
  - Recursive types (depth limits, coinductive semantics)

═══════════════════════════════════════════════════════════
EXAMPLE FAILING TESTS (representative of each category)
═══════════════════════════════════════════════════════════

Generics:
  TypeScript/tests/cases/compiler/genericFunctionInference1.ts

Mapped types + recursion:
  TypeScript/tests/cases/compiler/mappedTypeRecursiveInference.ts

Contextual typing:
  TypeScript/tests/cases/compiler/contextualTypingOfLambdaWithMultipleSignatures2.ts

Conditional types:
  TypeScript/tests/cases/compiler/conditionalTypeDoesntSpinForever.ts

Run any single test:
  cargo build --profile dist-fast -p tsz-cli && \
  .target/dist-fast/tsz TypeScript/tests/cases/compiler/TEST_NAME.ts 2>&1

Compare with TSC baseline (cached):
  cat TypeScript/tests/baselines/reference/TEST_NAME.errors.txt

═══════════════════════════════════════════════════════════
RUNNING CONFORMANCE TESTS
═══════════════════════════════════════════════════════════

Build the binary:
  cargo build --profile dist-fast -p tsz-cli

Run full conformance suite:
  ./scripts/conformance.sh run

Run a specific slice:
  ./scripts/conformance.sh run --max=100 --offset=0

Analyze failures:
  ./scripts/conformance.sh analyze
  ./scripts/conformance.sh analyze --category false-positive
  ./scripts/conformance.sh analyze --category all-missing
  ./scripts/conformance.sh analyze --category wrong-code
  ./scripts/conformance.sh analyze --category close

Filter by error code:
  ./scripts/conformance.sh run --error-code 2322

═══════════════════════════════════════════════════════════
WORKFLOW
═══════════════════════════════════════════════════════════

1. Pick a failing test from the example list or from conformance analysis
2. Compare tsz output with TSC baseline to understand the gap
3. Identify the root cause (missing feature, wrong inference, etc.)
4. Write a minimal reproduction in tmp/
5. Run with tracing: TSZ_LOG=debug TSZ_LOG_FORMAT=tree .target/dist-fast/tsz tmp/test.ts 2>&1
6. Fix the solver/checker code
7. Verify: run the original test + cargo nextest run
8. Run conformance to check for regressions
9. Commit with clear message
10. MANDATORY — sync after EVERY commit:
      git pull --rebase origin main && git push origin main

═══════════════════════════════════════════════════════════
STRATEGY
═══════════════════════════════════════════════════════════

Focus on CORE TYPE SYSTEM fixes that affect many tests:
  - Conditional type evaluation (blocks ~200 tests)
  - Contextual typing for function expressions (TS7006 false positives)
  - Generic inference edge cases (multi-signature, higher-order)
  - Mapped type completeness (array/tuple preservation)

Prioritize by impact:
  1. Fixes that unblock the most conformance tests
  2. Fixes that improve inference accuracy broadly
  3. Edge cases in specific type system features

Do NOT break existing passing tests. Always verify with cargo nextest run.

═══════════════════════════════════════════════════════════
KEY CODE LOCATIONS
═══════════════════════════════════════════════════════════

  Type inference:       crates/tsz-solver/src/infer.rs
  Contextual typing:    crates/tsz-solver/src/contextual.rs
  Conditional types:    crates/tsz-solver/src/evaluate_rules/conditional.rs
  Mapped types:         crates/tsz-solver/src/evaluate_rules/mapped.rs
  Infer patterns:       crates/tsz-solver/src/evaluate_rules/infer_pattern.rs
  Recursive types:      crates/tsz-solver/src/recursion.rs
  Type instantiation:   crates/tsz-solver/src/instantiate.rs
  Type application:     crates/tsz-solver/src/application.rs
  Subtype checks:       crates/tsz-solver/src/subtype.rs
  Call checking:        crates/tsz-checker/src/call_checker.rs
  Type computation:     crates/tsz-checker/src/type_computation_complex.rs
  State/checker:        crates/tsz-checker/src/state.rs
  Diagnostics:          crates/tsz-common/src/diagnostics.rs
EOF

exit 2
