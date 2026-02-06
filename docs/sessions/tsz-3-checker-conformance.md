# Session tsz-3: Checker Conformance & Architecture Alignment

**Started**: 2026-02-06
**Status**: Active
**Predecessor**: tsz-2 (Solver Stabilization - COMPLETED)

## Context

The tsz-2 session successfully stabilized the Solver unit tests (3524 tests passing, 0 failures). Now we need to verify that the Checker correctly uses the stable Solver and establish a conformance baseline.

## Goals

1. **Conformance Baseline**: Run the conformance test suite to identify failing tests
2. **Checker Architecture Audit**: Ensure Checker is a "thin wrapper" that delegates to the Solver, not implementing logic itself
3. **Control Flow Integration**: Verify Binder flow graph generation and Checker narrowing work correctly

## Priorities

### Priority A: Conformance Baseline
- Run: `./scripts/conformance/run.sh --server --max=500`
- Establish baseline of passing/failing tests
- Categorize failures: Checker misconfiguration vs. missing Solver logic

### Priority B: Checker Refactoring (North Star Alignment)
- Audit `src/checker/` for Direct TypeKey Matching (anti-pattern)
- Replace `match type_key { ... }` with calls to `self.solver.is_subtype_of(...)`
- Goal: Checker should be orchestration, not logic container

### Priority C: Control Flow Analysis Integration
- Verify `src/checker/flow_analysis.rs` integrates with `src/solver/narrowing.rs`
- Test narrowing behavior with conformance tests

## Progress

### Conformance Baseline (2026-02-06)

**500 tests**: 256/500 passed (51.2%)
- Top errors: TS2339 (48 extra), TS2322 (41 missing, 22 extra), TS2307 (15 extra)
- Time: 5.8s

### Bug Investigation: Union of Constructor Types (2026-02-06)

**Test**: `abstractClassUnionInstantiation.ts`
**Issue**: `new cls3()` where `cls3: typeof ConcreteA | typeof ConcreteB` incorrectly reports TS2351

**Investigation Progress**:
- Gemini identified root cause: `check_symbol_constructability` returns TypeQuery instead of converting to Callable
- Attempted fix: Use `construct_to_call_callable` on cached symbol type (Commit 7afc37a31)
- Status: Still not working - `construct_to_call_callable` returns None for the cached type

**Current Understanding**:
- `typeof Class` resolves to a TypeQuery
- Need to resolve TypeQuery to actual Callable before conversion
- Complexity: Type resolution requires `&mut self` but we're in `&self` context
- May need different approach or architectural change

**Requires Further Investigation**:
- How does TypeQuery resolution work in the checker?
- What is the actual cached type for a class symbol?
- Should we use tracing to see the type flow?

## Next Steps

1. Run initial conformance baseline
2. Pick a failing test and trace with TSZ_LOG=debug
3. Use Two-Question Rule for any Checker changes
