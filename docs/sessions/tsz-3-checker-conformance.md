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

**Investigation Plan** (per Gemini guidance):
1. Reproduce with tracing: `TSZ_LOG="wasm::solver::subtype=trace"`
2. Focus on **Lawyer layer** (`src/solver/lawyer.rs`) - not Checker
3. Check **Variadic Tuple Subtyping** - rest params may be represented as tuples
4. Audit `any` propagation rules during function parameter comparison
5. Use Two-Question Rule before implementing fix

**Files**:
- `src/solver/subtype_rules/functions.rs` - function subtyping logic
- `src/solver/lawyer.rs` - any propagation rules
- `src/solver/subtype.rs` - tuple assignability

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

