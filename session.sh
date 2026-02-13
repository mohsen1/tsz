#!/bin/bash
# Session script: control-flow narrowing tsc-equivalence mission

cat >&2 <<'EOF'
IMPORTANT: Read docs/HOW_TO_CODE.md before writing any code. It covers architecture
rules, coding patterns, recursion safety, testing, and debugging conventions.

═══════════════════════════════════════════════════════════
MISSION: Fix control-flow narrowing to match tsc behavior
═══════════════════════════════════════════════════════════

Control-flow narrowing is not tsc-equivalent. Several categories diverge:
  - Aliased discriminant narrowing
  - Assertion-function narrowing
  - Destructuring-aware flow analysis
  - CFA (Control Flow Analysis) edge cases

Key failing test files:
  TypeScript/tests/cases/compiler/controlFlowAliasedDiscriminants.ts
  TypeScript/tests/cases/compiler/assertionFunctionsCanNarrowByDiscriminant.ts
  TypeScript/tests/cases/compiler/destructuringTypeGuardFlow.ts
  TypeScript/tests/cases/conformance/controlFlow/assertionTypePredicates1.ts

═══════════════════════════════════════════════════════════
INVESTIGATION WORKFLOW
═══════════════════════════════════════════════════════════

1. Run the key failing tests through conformance to see exact differences:
     cargo build --profile dist-fast -p tsz-cli
     ./scripts/conformance.sh run --filter controlFlow
     ./scripts/conformance.sh run --filter assertionFunction
     ./scripts/conformance.sh run --filter discriminant

2. For each failing test, create a minimal reproduction in tmp/:
     cp TypeScript/tests/cases/compiler/controlFlowAliasedDiscriminants.ts tmp/
     .target/dist-fast/tsz tmp/controlFlowAliasedDiscriminants.ts --pretty false 2>&1

3. Compare with tsc expected output (in conformance cache):
     node -e "const c=require('./crates/conformance/tsc-cache-full.json'); console.log(JSON.stringify(c['controlFlowAliasedDiscriminants.ts'],null,2))"

4. Use tracing to debug narrowing behavior:
     TSZ_LOG="wasm::solver::narrowing=trace" TSZ_LOG_FORMAT=tree \
       cargo run -p tsz-cli --bin tsz -- tmp/test.ts 2>&1 | head -200

5. Fix the narrowing/CFA code
6. Run cargo nextest run to ensure no regressions
7. Re-run conformance to verify improvement
8. Commit with clear message
9. MANDATORY — sync after EVERY commit:
     git pull --rebase origin main && git push origin main

═══════════════════════════════════════════════════════════
NARROWING CATEGORIES TO FIX
═══════════════════════════════════════════════════════════

1. ALIASED DISCRIMINANTS
   When a discriminant property is assigned to a local variable,
   narrowing the alias should narrow the original object:
     const kind = obj.kind;
     if (kind === "a") { obj.value /* should be narrowed */ }
   tsc tracks the alias relationship through CFA.

2. ASSERTION FUNCTIONS
   Functions declared with `asserts x is T` or `asserts x` should
   narrow the type of x in subsequent code:
     function assertIsString(x: unknown): asserts x is string { ... }
     assertIsString(val);
     val // should be string here
   Also: assertion functions narrowing by discriminant property.

3. DESTRUCTURING-AWARE FLOW
   When destructuring, narrowing should flow through:
     const { kind, value } = obj;
     if (kind === "a") { value /* should be narrowed */ }
   The destructured bindings should be linked to the original object's CFA.

4. CFA EDGE CASES
   Various edge cases in control flow analysis:
   - Narrowing after throw/return in if branches
   - Narrowing in switch/case with fallthrough
   - Narrowing across function boundaries (closures)
   - Truthiness narrowing for optional properties

═══════════════════════════════════════════════════════════
KEY CODE LOCATIONS
═══════════════════════════════════════════════════════════

  Narrowing:        crates/tsz-checker/src/narrowing.rs (or solver/narrowing.rs)
  Control flow:     crates/tsz-checker/src/control_flow.rs
  Type guards:      crates/tsz-checker/src/ (search for type_guard, type_predicate)
  Assertion funcs:  search for "asserts" in checker/solver
  Solver:           crates/tsz-checker/src/solver/
  Subtype checks:   crates/tsz-checker/src/solver/subtype.rs
  Type computation: crates/tsz-checker/src/type_computation_complex.rs
  Checker state:    crates/tsz-checker/src/state.rs
  Binder CFA:       crates/tsz-binder/src/ (search for control_flow, flow_node)
  Diagnostics:      crates/tsz-common/src/diagnostics.rs
  CLI driver:       crates/tsz-cli/src/driver.rs

═══════════════════════════════════════════════════════════
STRATEGY
═══════════════════════════════════════════════════════════

Start by understanding the CURRENT narrowing architecture:
  1. Read the narrowing code to understand what's implemented
  2. Run the 4 key test files to see exact error differences
  3. Categorize failures: missing narrowing vs wrong narrowing vs crash
  4. Fix in order of impact (most tests affected first)

Focus on fixes that are GENERAL (help many CFA tests) rather than narrow one-offs.
Do NOT break existing passing tests. Always verify with cargo nextest run.
EOF

exit 2
