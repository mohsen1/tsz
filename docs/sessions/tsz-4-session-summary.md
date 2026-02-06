# Session tsz-4: Checker Infrastructure & Control Flow Integration

**Started**: 2026-02-06
**Status**: Starting
**Focus**: Fix checker test infrastructure and implement CFA integration

## Background

Session tsz-3 achieved **SOLVER COMPLETE** - 3544/3544 solver tests pass (100% pass rate). The Solver (the "WHAT") is now complete. The next priority is the Checker (the "WHERE") - the orchestration layer that connects the AST to the Type Engine.

Per NORTH_STAR.md Section 4, the Checker must:
1. Walk the AST and determine WHEN to check each node
2. Use the Solver's `narrow()` operation with the Binder's Flow Graph
3. Follow the "Thin Wrapper" principle - delegate all type logic to the Solver

## Priority Tasks

### Task #21: Fix Checker Test Infrastructure 🔥 (CRITICAL PATH)
**Problem**: 184 `checker_state_tests` fail because basic types (string, number, etc.) aren't properly interned in the test `TypeDatabase`.

**Evidence**:
```
Cannot find global type 'Array'.
Cannot find global type 'String'.
Cannot find global type 'Number'.
```

**Root Cause**: Tests aren't properly calling `setup_lib_contexts` to populate the global `TypeInterner` with standard library types.

**Files**: `src/checker/state.rs`, test utility files

**Goal**: 0 failing `checker_state_tests` (infrastructure only)

### Task #22: Control Flow Analysis Integration
**Problem**: The Binder produces a `FlowGraph`, but the Checker doesn't yet use the Solver's `narrow()` operation with it.

**Example**:
```typescript
function example(x: string | number) {
    if (typeof x === "string") {
        x.length; // Checker must ask Solver to narrow x to string here
    }
}
```

**Goal**: Implement the bridge between Flow Graph and Narrowing

**Files**: `src/checker/expr.rs`, `src/checker/flow_analysis.rs`

**Reference**: NORTH_STAR.md Section 4.3 (Flow Graph) and 4.5 (Checker Responsibilities)

### Task #23: Symbol Resolution Audit
**Goal**: Ensure the Checker follows the "Thin Wrapper" principle.

**Action**: Audit `src/checker/symbol_resolver.rs` to ensure it delegates all type-identity logic to the Solver.

**Constraint**: Ensure `CheckerContext` fuel counter and recursion guards are properly applied.

## Starting Point

- Solver: 3544/3544 tests pass (100%)
- Checker: ~184 tests fail due to infrastructure issues
- Overall: 8111 passing, 189 failing, 158 ignored (from end of tsz-3)

## Next Steps

1. **Task #21**: Fix `setup_lib_contexts` in checker test suite
2. **Task #22**: Implement CFA integration for typeof guards
3. **Task #23**: Audit symbol resolution for thin wrapper compliance

## Success Criteria

- 0 infrastructure-related test failures in checker_state_tests
- Basic narrowing conformance tests pass (typeof, truthiness)
- At least 5 major TS error codes (TS2322, TS2345) align with tsc
