# Session tsz-3: Control Flow Analysis & Narrowing

**Started**: 2026-02-04  
**Status**: 🟡 Active  
**Previous**: tsz-2 (Intersection Reduction and BCT - COMPLETED)

## Session Definition

**Why tsz-3?** tsz-2 successfully completed the "What" (Type Relations/Solver). tsz-3 shifts focus to the "Where" (Checker) and "Symbols" (Binder) to handle flow-sensitive typing.

**Goal**: Implement Control Flow Analysis (CFA) & Narrowing to significantly improve conformance pass rate (from 41.7% to 42.7%+ target).

## First Task: Truthiness Narrowing for Local Variables

**Problem**: Identifiers in conditional branches should be narrowed based on truthiness checks.

**Example**:
```typescript
function foo(x: string | null) {
    if (x) {
        x; // Should be 'string' (not 'string | null')
    }
}
```

### Files to Modify

1. **`src/binder/mod.rs`** - Verify CFG generation
   - Ensure `Binder` generates `FlowNodes` for `IfStatement` branches
   - Check if CFG is already robust

2. **`src/checker/mod.rs`** - Main implementation
   - Implement/expand `get_type_of_node` for identifiers
   - Add new function: `get_narrowed_type_at_node(node, symbol)`
   - Traverse `FlowNode` chain backwards to find guards

3. **`src/solver/visitor.rs`** - Type filtering
   - Implement `TypeFilter` visitor
   - Takes `Type` and `FilterKind` (Truthy/Falsy)
   - Example: `filter(String | Undefined, Truthy)` returns `String`

### Starting Point

**Specific Action**: Modify `src/checker/mod.rs` to intercept identifier resolution and check the Control Flow Graph (CFG) for truthiness guards. If a guard is found, use a new Solver visitor to subtract `null | undefined` from the type.

## Success Criteria

- **Primary Metric**: Conformance pass rate increases by at least **1%** (≈130 tests)
- **Specific Test**: `tests/conformance/controlFlow/truthinessNarrowing.ts` passes
- **Code Behavior**: Inside truthy checks, `null`/`undefined` are correctly removed from union types

## Session History

- 2026-02-04: Session defined based on Gemini recommendation after tsz-2 completion
- 2026-02-04: First task clearly defined: Truthiness Narrowing

## Complexity: HIGH

**Risk**: Changes to narrowing affect type resolution throughout the checker

**Mandatory**: Follow **Two-Question Rule** for all changes to `src/checker/mod.rs`, `src/binder/mod.rs`, and `src/solver/visitor.rs`
