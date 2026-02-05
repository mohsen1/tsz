# Session TSZ-12: Advanced Narrowing & Type Predicates

**Started**: 2026-02-05
**Status**: ✅ TASKS 1 & 2 COMPLETE - `in` Operator & Type Guards Working

## Goal

Complete CFA narrowing parity with TypeScript by implementing:
1. The `in` operator narrowing
2. User-Defined Type Guards (`is` predicates)
3. Exhaustiveness checking (switch/if-else chains)

## Context

Previous sessions successfully implemented:
- ✅ TSZ-10: Narrowing infrastructure (truthiness, typeof, instanceof, discriminant unions, assertion functions)
- ✅ TSZ-11: Fixed instanceof narrowing by removing is_narrowable_type check

Current state: Core narrowing works, but TypeScript has more advanced features needed for full parity.

## Scope

### Task 1: `in` Operator Narrowing

Implement narrowing for property existence checks:
```typescript
function test(obj: { prop?: string } | { other: number }) {
    if ("prop" in obj) {
        obj.prop; // Should work - narrowed to first member
    }
}
```

**Files**:
- `src/solver/narrowing.rs` - Add narrow_by_in_operator logic
- `src/checker/expr.rs` - Handle binary `in` expressions

**Implementation approach** (Mandatory Gemini Question 1):
- Create new narrowing logic for property presence
- Handle unions where some members have the property
- Handle narrowing by absence (else branch)

### Task 2: User-Defined Type Guards ✅ COMPLETE

**Status**: Type guards ARE ALREADY IMPLEMENTED!

**What We Found**:

1. **`apply_type_predicate_narrowing` exists** at src/checker/control_flow_narrowing.rs:383
   - Handles both `x is T` and `asserts x is T` predicates
   - Wires up with TypeResolver for type alias support
   - Correctly narrows in true/false branches

2. **Unit tests pass**:
   - `test_user_defined_type_predicate_narrows_branches` ✅
   - `test_user_defined_type_predicate_alias_narrows` ✅
   - `test_asserts_type_predicate_narrows_true_branch` ✅

3. **Real-world verification**:
   ```typescript
   declare function isNotNullish(value: unknown): value is {};

   declare const value1: unknown;
   if (isNotNullish(value1)) {
       value1; // Correctly narrowed to {}
   }
   ```

   Both TSZ and TypeScript accept this code.

**Implementation Details**:

The type predicate narrowing works by:
- Extracting the TypePredicate from function signatures (in signature_builder.rs)
- Applying the predicate when the function is called in a condition
- Narrowing the argument to the predicate type in the true branch
- Narrowing to the exclusion of the predicate type in the false branch

**Note**: Type predicates work with union types where the predicate type is a member of the union.
For example, `x is string` works when `x: string | number` but has limited effect when `x: unknown`.

This is consistent with TypeScript's behavior.

**Bug Fix**: Fixed `PropertyAccessEvaluator::with_resolver` method to fix test compilation failures.

### Task 3: Exhaustiveness Checking

Ensure `never` is correctly inferred:
```typescript
type Shape = { kind: "circle" } | { kind: "square" };

function test(shape: Shape) {
    switch (shape.kind) {
        case "circle": return 1;
        case "square": return 2;
    }
    // Should error: shape is never here (exhaustive check)
}
```

**Files**:
- `src/checker/statements.rs` - Switch statement analysis
- `src/solver/narrowing.rs` - Never type inference

## Plan

### Phase 1: Architecture Validation (MANDATORY GEMINI)

Task 1: `in` operator approach validation
- Ask: "How should I implement `in` operator narrowing? Should I create a new NarrowingKind? How do I handle the else case (narrowing by absence)?"

Task 2: Type guard approach validation  
- Ask: "How should I implement user-defined type guards? Where do I extract the TypePredicate from function signatures?"

### Phase 2: Implementation

Task 3: Implement `in` operator narrowing
Task 4: Implement user-defined type guards
Task 5: Implement exhaustiveness checking

### Phase 3: Verification

Task 6: Test with real-world TypeScript code
Task 7: Run conformance tests for narrowing

## Risks

1. **Property presence checking**: Requires analyzing type shapes to determine if a property exists
2. **Type predicate extraction**: Need to correctly parse function signatures for `is` predicates
3. **Performance**: `in` operator checks could be expensive if not cached properly

## Success Criteria

1. ✅ `in` operator narrowing works (VERIFIED)
2. ✅ User-defined type guards work (VERIFIED)
3. ⏳ Exhaustiveness checking detects when types are narrowed to `never` (IN PROGRESS)
4. ⏳ All features match TypeScript behavior
5. ✅ No regressions in existing narrowing

## References

- Previous Sessions: docs/sessions/history/tsz-10.md, tsz-11.md
- North Star: docs/architecture/NORTH_STAR.md
- Narrowing Logic: src/solver/narrowing.rs
- Expression Checking: src/checker/expr.rs

---
**AGENTS.md Reminder**: All solver/checker changes require two-question Gemini consultation.

## Investigation Update

**CRITICAL DISCOVERY**: `in` operator narrowing ALREADY WORKS!

### What We Found

1. **`narrow_by_property_presence` exists** at src/solver/narrowing.rs:1042
   - Implements union filtering based on property presence
   - Handles optional vs required property checking
   - Handles unknown type narrowing

2. **`narrow_by_in_operator` exists** at src/checker/control_flow_narrowing.rs:513
   - Called from `narrow_by_binary_expr` in src/checker/control_flow.rs:2273
   - Properly integrates with flow analysis

3. **Testing confirms it works**:
   ```typescript
   type A = { prop: string };
   type B = { other: number };

   function testInOperator(obj: A | B) {
       if ("prop" in obj) {
           const s: string = obj.prop; // ✅ Works!
           return s;
       } else {
           const n: number = obj.other; // ✅ Works!
           return n;
       }
   }
   ```

   Both TSZ and TypeScript compile this without errors.

### Task 1 Status: ✅ COMPLETE

The `in` operator narrowing is fully functional and matches TypeScript behavior.

### Task 2 Status: ✅ COMPLETE

User-defined type guards and assertion predicates are fully functional.

**What Works**:
- Type guards with union types: `(x: string | number) => x is string`
- Assertion predicates: `(x: unknown) => asserts x is string`
- Bare assertions: `(x: unknown) => asserts x`
- Type guards with type aliases

**Limitations** (Consistent with TypeScript):
- Type predicates have limited effect on `unknown` type
- Work best with union types where predicate type is a member

**Next Task**: Exhaustiveness Checking (Task 3)

### Task 3: Exhaustiveness Checking (IN PROGRESS)

**Status**: Mandatory Gemini consultation complete - implementation approach defined.

**Problem**: After exhaustive switch/if-else, variables should narrow to `never`.

**Gemini Guidance (Question 1 - Approach Validation)**:

The solution is to model the "implicit default" path in the control flow graph. When a switch statement has no `default` clause, execution falls through to the end if no case matches. On this path, the type is narrowed by excluding all case values. If all possible values are covered, the result becomes `never`.

**Implementation Plan**:

1. **Flow Graph Construction** (`src/checker/flow_graph_builder.rs`):
   - Modify `build_switch_statement` to create an implicit default flow node when no default clause exists
   - Connect this node to the end label

2. **Control Flow Analysis** (`src/checker/control_flow.rs`):
   - Update `handle_switch_clause_iterative` to recognize implicit default nodes
   - Apply `narrow_by_default_switch_clause` for implicit defaults

3. **Narrowing Logic** (`src/checker/flow_narrowing.rs`):
   - Already exists - `narrow_by_default_switch_clause` handles the type algebra
   - `narrow_excluding_types` in solver automatically produces `never` when all union members are excluded

**Key Insight**: Don't explicitly check "is exhaustive". Instead, model the "else" path. If the switch is exhaustive, the type on the "else" path becomes `never` automatically.

### Implementation Status: IN PROCEEDING - NOT YET WORKING

**What Was Implemented** (commit 2f0c908f0):

1. **FlowGraphBuilder changes**:
   - Added `has_default_clause` tracking in `build_switch_statement`
   - Create implicit default flow node when no default clause exists
   - Use `switch_data.case_block` as node marker for implicit default
   - Connect implicit default to `end_label` with `FlowNodeId::NONE` (no fallthrough)

2. **FlowAnalyzer changes**:
   - Detect implicit default by checking `clause_idx.kind == BLOCK`
   - Get parent switch via `arena.get_extended(clause_idx).parent`
   - Apply `narrow_by_default_switch_clause` for implicit defaults

**Current Problem**: Exhaustiveness checking not working. Test case:
```typescript
type Shape = { kind: "circle" } | { kind: "square" };
function test(shape: Shape) {
    switch (shape.kind) {
        case "circle": return 1;
        case "square": return 2;
    }
    const x: never = shape; // Should work - shape is never
}
```
- TypeScript: Accepts (correct - exhaustive)
- TSZ: Reports error "Shape is not assignable to never" (incorrect)

**Investigation Needed**:
The implicit default flow node appears to not be created or processed during normal type checking. Debug messages showed:
- Only 2 CASE_CLAUSE nodes (297) being processed
- No BLOCK nodes (242) which would indicate the implicit default
- No flow_graph debug output, suggesting `build_switch_statement` may not be called or case_block is None

**Possible Issues**:
1. Flow graph may be built at a different phase than expected
2. The case_block may be None in some code path
3. The implicit default creation code may not be reached
4. Flow graph construction might happen in binder, not checker

**Next Steps**:
1. Investigate when and where flow graph is built in the type checking pipeline
2. Trace through a simple switch statement to understand flow graph construction
3. Verify the implicit default node is actually created in the flow graph
4. If created, verify it's being processed by FlowAnalyzer

**Session Status**: ROOT CAUSE IDENTIFIED - Architectural disconnect between two CFGs.

### 🚨 CRITICAL DISCOVERY from Gemini (Question 2)

**The Problem**: There are TWO Control Flow Graphs in the codebase:

1. **Binder's CFG** (`src/binder/state.rs`):
   - Basic control flow graph built during binding phase
   - Contains CASE_CLAUSE nodes (297) that we see being processed
   - Does NOT have implicit default logic

2. **Checker's CFG** (`src/checker/flow_graph_builder.rs`):
   - Refined CFG for type narrowing
   - This is where we implemented implicit default logic
   - **NOT being used during type checking!**

**The Smoking Gun** (`src/checker/control_flow.rs:135`):
```rust
pub fn new(arena: &'a NodeArena, binder: &'a BinderState, interner: &'a dyn QueryDatabase) -> Self {
    // HARDCODED to use Binder's flow nodes - this is the problem!
    let flow_graph = Some(FlowGraph::new(&binder.flow_nodes));
    Self { ... }
}
```

The `FlowAnalyzer` defaults to the Binder's CFG, which doesn't contain our implicit default logic. The `FlowGraphBuilder` isn't even being executed during type checking!

### Solution Path (from Gemini)

**Step 1**: Update `FlowAnalyzer` to accept custom flow graph arena
- Add `with_flow_nodes(&arena)` method to override default Binder graph
- Location: `src/checker/control_flow.rs`

**Step 2**: Wire `FlowGraphBuilder` into the Checker
- Find where Checker processes function bodies (`src/checker/state.rs` or `declarations.rs`)
- Run `FlowGraphBuilder::new().build_function_body(body)` for each function
- Pass resulting arena to `FlowAnalyzer` via `with_flow_nodes()`

**Step 3**: Verify wiring works
- Debug statements in `build_switch_statement` should appear
- BLOCK nodes (242) should be processed by FlowAnalyzer
- Test case should work (exhaustive switch narrows to never)

### Investigation Tasks (Next Steps)

1. **Search** for `FlowAnalyzer::new` calls in `src/checker/state.rs` and `expr.rs`
2. **Check** if `FlowGraphBuilder` is imported/used anywhere in `src/checker/`
3. **Modify** `FlowAnalyzer::with_flow_nodes()` to accept builder's arena
4. **Inject** `FlowGraphBuilder` pass into function checking pipeline
5. **Ask Gemini** to review the implementation (MANDATORY per AGENTS.md rule)

This is a critical architectural fix that aligns with the North Star Architecture (Section 4.3, 4.5): Binder handles basic symbol flow, Checker/Solver handles refined flow analysis for narrowing.


