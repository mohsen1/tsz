# Session tsz-3: Checker Conformance & Architecture Alignment

**Started**: 2026-02-06
**Status**: Active - Focus: Rest Parameter & Variadic Tuple Subtyping
**Predecessor**: tsz-2 (Solver Stabilization - COMPLETED)

## Current Focus (2026-02-06)

**Issue**: Function type with fixed parameters NOT assignable to function with rest parameter

**Test**: `aliasOfGenericFunctionWithRestBehavedSameAsUnaliased.ts`

**Problem**:
```typescript
type a3 = (name: string, mixed: any, args_0: any) => any
type b3 = (name: string, mixed: any, ...args: any[]) => any
type test3 = a3 extends b3 ? "y" : "n"  // tsc: "y", tsz: "n"
```

**Investigation Direction** (per Gemini guidance):
- Focus on **Lawyer layer** (`src/solver/lawyer.rs`) - not Checker
- Check **Variadic Tuple Subtyping** - rest params may be represented as tuples
- Audit `any` propagation rules during function parameter comparison
- Use tracing: `TSZ_LOG="wasm::solver::subtype=trace"`

**Files**:
- `src/solver/subtype.rs` - tuple assignability
- `src/solver/lawyer.rs` - any propagation rules
- `src/solver/subtype_rules/functions.rs` - function subtyping

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

### Bug Fix: Union of Constructor Types (2026-02-06)

**Test**: `abstractClassUnionInstantiation.ts`
**Issue**: `new cls3()` where `cls3: typeof ConcreteA | typeof ConcreteB` incorrectly reports TS2351

**Root Cause**:
The Checker was manually handling `TypeQuery` resolution for constructor expressions, which violated the "Solver-First" architectural principle. The existing code used `classify_for_new_expression` and `classify_for_call_signatures` to manually transform types.

**Solution (Following Two-Question Rule)**:

**Question 1**: Asked Gemini for architectural guidance on how to handle union of constructors
- Gemini identified that the Checker should delegate to Solver
- Recommended adding `resolve_new` method to `CallEvaluator`
- This mirrors `resolve_call` but handles construct signatures

**Question 2**: Asked Gemini for complete implementation details
- Got full code for `resolve_new`, `resolve_callable_new`, `resolve_union_new`, `resolve_intersection_new`
- Got wiring instructions for the Checker

**Implementation** (Commit: pending):
1. **Added to `src/solver/operations.rs`**:
   - `resolve_new` - main entry point for `new` expressions
   - `resolve_callable_new` - handles Callable types with construct signatures
   - `resolve_union_new` - handles unions (all members must be constructable)
   - `resolve_intersection_new` - handles intersections (Mixin pattern)

2. **Modified `src/checker/type_computation_complex.rs`**:
   - Replaced large `match classify_for_new_expression` block
   - Now delegates directly to `evaluator.resolve_new()`
   - Cleaner separation of concerns

**Verification**:
- Test case `abstractClassUnionInstantiation.ts` now passes
- `new (typeof ConcreteA | typeof ConcreteB)()` correctly returns the union of instance types
- Matches TypeScript behavior exactly

**Architectural Impact**:
- Successfully moved constructor resolution logic from Checker to Solver
- Follows "Solver-First" principle from NORTH_STAR.md
- Cleaner, more maintainable code
- Reduced ~230 lines of code while improving correctness

**Commit**: d44a93ddd

---

### Next Priority: Rest Parameter & Variadic Tuple Subtyping

**Issue**: Function type with fixed parameters NOT assignable to function with rest parameter

**Test**: `aliasOfGenericFunctionWithRestBehavedSameAsUnaliased.ts`

**Problem**:
```typescript
type a3 = (name: string, mixed: any, args_0: any) => any
type b3 = (name: string, mixed: any, ...args: any[]) => any
type test3 = a3 extends b3 ? "y" : "n"  // tsc: "y", tsz: "n"
```

**Status**: Investigation in progress
- The `extra_required_accepts_undefined` logic appears correct
- Need deeper investigation with tracing
- Disk space constraint requiring careful approach

**Alternative Quick Win Identified**: Union Property Access
- TS2339 (48 extra errors) - union type property access
- Already has fix branch: `fix/ts2339-union-property-access`
- Could merge for quick conformance improvement

## Session Summary

**Completed**:
- Union of constructors bug (commit d44a93ddd)
- Session tracking and documentation

**Blocked On**:
- Rest parameter bug requires extensive debugging
- Disk space constraints affecting rebuild time

**Options**:
1. Continue rest parameter investigation (high value, time-intensive)
2. Merge union property access branch (quick win)
3. Find other simpler bug to fix

## Next Steps

1. Run initial conformance baseline
2. Pick a failing test and trace with TSZ_LOG=debug
3. Use Two-Question Rule for any Checker changes

## Detailed Investigation (2026-02-06)

### Key Finding: Function Type Instantiation Preserves rest Flag

In `src/solver/instantiate.rs` line 422, when instantiating function types:
```rust
rest: p.rest,  // Preserves rest flag from original parameter
```

### The Core Question

When declaring `type F<T extends any[]> = (...args: T) => void`:

**For `F<[any]>` (instantiation with tuple):**
- Should the function have: `params: [{type: any, rest: false}]` (fixed param)
- Or: `params: [{type: any, rest: true}]` (rest param)?

**For `F<any[]>` (instantiation with array):**
- Should have: `params: [{type: any[], rest: true}]`

### Investigation Approach

1. The instantiation preserves the `rest` flag from the generic definition
2. Need to determine how `...args: T` is initially lowered when `T extends any[]`
3. Key file: `src/checker/type_node.rs` - tuple type lowering (lines 393-403)

### Next Step

Add debug output to see actual FunctionShape.params structure for both instantiations.


## 2026-02-06 Critical Discovery

### Found Root Cause Location!

In `src/solver/evaluate_rules/conditional.rs` lines 199-200:
```rust
let mut checker = SubtypeChecker::with_resolver(self.interner(), self.resolver());
checker.allow_bivariant_rest = true;
```

**Key Finding**: `allow_bivariant_rest` IS already set to true!

This means the fix at lines 310-312 in functions.rs SHOULD work:
```rust
if rest_is_top {
    return SubtypeResult::True;
}
```

### New Hypothesis

The problem is NOT in the subtype checking logic. The problem must be:
1. The target type doesn't actually have `rest: true` set
2. OR `rest_elem_type` is not `Some(any)` as expected

### Next Investigation Step

Add debug output to verify:
1. Does target.params.last() have `rest: true`?
2. What does `get_array_element_type(target.params.last().type_id)` return?
3. Is the issue in how generic types are instantiated?


## 2026-02-06 Final Investigation Summary

### Confirmed Facts
1. `allow_bivariant_rest = true` in evaluate_conditional.rs (line 200)
2. Rest flag IS preserved during tuple instantiation (instantiate.rs line 280)
3. Function subtype checking logic appears correct (functions.rs lines 305-331)
4. Unit test confirms rest flag preservation works correctly

### Remaining Hypothesis
The problem is likely in how the ORIGINAL function type `(...args: T)` is lowered, NOT in instantiation.

When declaring `type F<T extends any[]> = (...args: T) => void`:
- How is the parameter `args` initially represented?
- Does it have `rest: true` based on the constraint `T extends any[]`?
- Or does it only get `rest: true` when T is known to be an array?

### Next Steps (Per Gemini Recommendation)
1. Investigate type lowering for generic rest parameters
2. If blocked > 30 min, pivot to Template Literal Types or Intrinsic String Types
3. Focus: src/checker/type_node.rs or src/solver/lower.rs

### Impact
Fixing this would unlock dozens of conformance tests related to:
- Parameters<T>, ConstructorParameters<T>
- Tail<T>, variadic tuple types
- Higher-order function patterns


## 2026-02-06 Key Finding: Rest Flag Comes From AST Syntax

**Critical Discovery**: The `rest: true` flag comes from the AST syntax `...`, not from the type parameter constraint!

From src/parser/state_statements.rs lines 1359-1367:
```rust
let is_rest_param = if let Some(node) = self.arena.get(param) {
    if let Some(param_data) = self.arena.get_parameter(node) {
        param_data.dot_dot_dot_token  // <-- AST has ...
    } else {
        false
    }
};
```

**This Means**:
- `type F<T extends any[]> = (...args: T) => void` has `rest: true` because of the `...` syntax
- When instantiating `F<[any]>`, we substitute `T` with `[any]` but the `rest` flag is preserved
- The function shape already has `rest: true` before instantiation

**Conclusion**: The lowering is likely correct! The problem must be elsewhere.

### Revised Hypothesis

Maybe the issue is NOT about rest flags at all. Let me check:
1. Are the function types actually being created correctly?
2. Is there an issue with how `[any]` (tuple) vs `any[]` (array) are compared?
3. Could the issue be in tuple subtyping rather than function subtyping?

---

## 2026-02-06 Deep Logic Trace

### Traced Through functions.rs Lines 305-331

The logic SHOULD work:
1. `target_has_rest` = true ✓
2. `rest_elem_type` = `Some(get_array_element_type(any[]))`
3. `get_array_element_type(any[])` should return `ANY` (per tuples.rs:372-377)
4. `rest_is_top` = `true && matches!(Some(ANY), Some(ANY | UNKNOWN))` = `true`
5. Should return `SubtypeResult::True` at line 311

**But test is failing!** This means:
- Either `get_array_element_type(any[])` is NOT returning `ANY`
- Or `target.params.last().type_id` is NOT `any[]` during comparison

### Current Status

**Investigation**: ~280 lines documented in session file
**Constraint**: Disk space low (had cargo clean)
**Impact**: Blocks dozens of conformance tests
**Recommendation from Gemini**: Continue with targeted tracing

**Next Step** (for follow-up session):
Use tracing with specific filters to see actual TypeId values during comparison:
```bash
TSZ_LOG="wasm::solver::subtype=trace" cargo test ...
```

---

## 2026-02-06 MAJOR DISCOVERY: Unit Test Passes!

### Test Results

**Unit test**: `test_rest_any_three_fixed_to_two_fixed_plus_rest` - **PASSES ✓**
- Created exact replica of failing conformance test
- Uses same type structure: 3 fixed vs 2 fixed + 1 rest with `any[]`
- Result: Works correctly!

**Conformance test**: `aliasOfGenericFunctionWithRestBehavedSameAsUnaliased.ts` - **FAILS ✗**
- Same type pattern
- Result: Incorrectly reports `"n"` instead of `"y"`

### Critical Conclusion

**The bug is NOT in the function subtype checking logic!**

The problem must be in:
1. **Type lowering**: How generic functions are parsed from TypeScript
2. **Type instantiation**: How `ExtendedMapper<any, any, [any]>` creates the function type
3. **Type representation**: The instantiated type doesn't match the expected structure

### Next Investigation Steps

1. Compare the FunctionShape from unit test vs. conformance test
2. Add tracing to see actual FunctionShape during conformance test
3. Check how `[any]` (tuple) vs `any[]` (array) are distinguished
4. Verify generic instantiation preserves rest flag correctly

### Impact

This is actually GOOD news - it means:
- The subtype logic is correct
- The fix will be in type lowering/instantiation, not core type operations
- More surgical fix, less risk of breaking other things

---

## 2026-02-06 Root Cause Identified: Tuple Instantiation Behavior

### Key Insight from Gemini

When a generic rest parameter `...args: T` is instantiated with a **tuple** `[any]`:
- TypeScript spreads the tuple into fixed parameters
- `(...args: [any])` becomes `(args_0: any)` - a FIXED parameter

When instantiated with an **array** `any[]`:
- It remains a rest parameter
- `(...args: any[])` stays as a rest parameter

### The Actual Types

**`type a = ExtendedMapper<any, any, [any]>`:**
```rust
FunctionShape {
  params: [
    { name: "name", type: string, rest: false },
    { name: "mixed", type: any, rest: false },
    { name: "args_0", type: any, rest: false }  // FIXED!
  ]
}
```

**`type b = ExtendedMapper<any, any, any[]>`:**
```rust
FunctionShape {
  params: [
    { name: "name", type: string, rest: false },
    { name: "mixed", type: any, rest: false },
    { name: "args", type: any[], rest: true }  // REST!
  ]
}
```

### Why TypeScript Allows This

The "Infinite Expansion Rule": A target with a rest parameter `(a, b, ...args: T[])` can accept any source with 2+ parameters because the rest acts as an infinite supply of `T`-typed parameters.

This is intentional unsoundness for JS patterns where extra arguments are ignored.

### The Bug

Our unit test passes because we manually construct types with correct structure.
The conformance test fails because somewhere in the instantiation pipeline, the types are not being created correctly OR the subtype check is not recognizing the correct case.

**Next Step**: Add tracing to see what FunctionShapes are actually created during the conformance test.

