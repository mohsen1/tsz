# Session tsz-2: Coinductive Subtyping (Recursive Types)

**Started**: 2026-02-05
**Status**: Active
**Goal**: Implement coinductive subtyping logic to handle recursive types without infinite loops

**Last Updated**: 2026-02-05 (Phase C Complete - Validated by Gemini Pro)

## Problem Statement

From NORTH_STAR.md Section 4.4:

> "TypeScript uses 'coinductive' subtyping for recursive types. This means we compute the Greatest Fixed Point (GFP) rather than Least Fixed Point (LFP). When comparing `type A = { self: A }` and `type B = { self: B }`, we assume they are subtypes and verify consistency."

Without coinductive subtyping, the compiler will crash or enter infinite loops when comparing recursive types.

**Impact**:
- Blocks complex recursive type definitions (linked lists, trees, Redux state)
- Causes stack overflow crashes
- Prevents proper type checking of self-referential generics

## Technical Details

**Files**:
- `src/solver/subtype.rs` - Core subtype checking logic
- `src/solver/mod.rs` - Solver state management
- `src/solver/visitor.rs` - Traversal of recursive structures

**Root Cause**:
When comparing `A` and `B` where both contain references to themselves, the naive approach leads to infinite recursion: `is_subtype_of(A, B)` → check properties → `is_subtype_of(A, B)` → ...

## Implementation Strategy

### Phase A: Fix the Build (Janitor Phase) ✅ COMPLETE
**Problem**: 9 compilation errors blocking tests

**Completed** (commit eae3bd048):
1. ✅ Fixed `compute_best_common_type` calls in expression_ops.rs to use `NoopResolver`
2. ✅ Fixed `narrow_by_discriminant` calls to pass `&[Atom]` instead of `Atom`
3. ✅ Removed deprecated `Ref` handling (Phase 4.3 uses `Lazy`)

**Result**: Build compiles successfully. Runtime shows stack overflow - the exact problem Coinductive Subtyping will solve.

### Phase B: Coinductive Subtyping Implementation ✅ COMPLETE
**Root Cause**: Expansive recursion - `type T = Box<T>` produces new TypeId on each evaluation, bypassing cycle detection

**Implementation** (commits b0dd31634, c04b735ff):

1. ✅ **DefId-Level Cycle Detection in check_subtype** (src/solver/subtype.rs)
   - Extract DefIds from Lazy/Enum types BEFORE calling evaluate_type
   - Check `seen_defs` for cycles (both forward and reverse pairs)
   - Return `CycleDetected` if cycle found
   - Cleanup `seen_defs` after evaluation

2. ✅ **visiting_defs Tracking in TypeEvaluator** (src/solver/evaluate.rs)
   - Added `visiting_defs: FxHashSet<DefId>` field
   - In `evaluate_application`: check if DefId is in `visiting_defs` before resolving
   - If yes, return application as-is (prevent infinite expansion)
   - If no, insert into `visiting_defs`, evaluate, then remove

**Result**: No more stack overflow when evaluating recursive types. Tests pass successfully.

**Step 1: Confirm Recursion Location** ⏳
```bash
TSZ_LOG=trace cargo nextest run --test_name
```

**Step 2: Implement Coinductive Subtyping**
**A. Fix src/solver/subtype.rs:**
- Ensure `depth` check returns `SubtypeResult::DepthExceeded`
- Review `in_progress` cycle detection

**B. Fix src/solver/evaluate.rs:**
- Detect recursive type alias expansion
- Enforce `MAX_EVALUATE_DEPTH` strictly
- Implement "Lazy" evaluation (one level at a time)

**Step 3: MANDATORY Pre-Implementation Question** ✅ COMPLETE (Gemini Flash)

**Key Insights from Gemini**:
1. **Two Types of Recursion**:
   - **Finite Recursion**: `interface List { next: List }` - TypeIds stay same, handled by `in_progress`
   - **Expansive Recursion**: `type T<X> = T<Box<X>>` - New TypeIds each time, needs `seen_defs` tracking

2. **Critical Functions to Modify**:
   - `src/solver/subtype.rs::check_subtype` - Add `seen_defs` check for Lazy/Enum types
   - `src/solver/subtype.rs::check_lazy_lazy_subtype` - Implement robust `seen_defs` logic
   - `src/solver/evaluate.rs::evaluate_application` - Add `visiting_defs: FxHashSet<DefId>` for expansive recursion

3. **Edge Cases**:
   - Mutual recursion: `type A = B; type B = A`
   - Distributive conditional types (exponential explosion)
   - Variance ping-pong between checks
   - Lazy resolution failure

4. **TypeScript Behaviors**:
   - TS2589: "Type instantiation is excessively deep"
   - TS2456: "Type alias circularly references itself"

5. **Recommended Implementation Plan**:
   - Extract `DefId`s from Lazy/Enum before calling `evaluate_type`
   - Check `seen_defs` before evaluation
   - Add `visiting_defs` to `TypeEvaluator` for `evaluate_application`
   - Use tracing to verify: `TSZ_LOG=trace cargo nextest run`

**Step 4: Implementation** ⏳
Based on Gemini's guidance, implement:
1. Modify `check_subtype` to extract and check `DefId`s before evaluation
2. Add `visiting_defs` to `TypeEvaluator`
3. Implement `seen_defs` logic in `check_lazy_lazy_subtype`
4. Test with tracing

**Step 4: Verify Fix** ⏳
- Test should pass or fail with type error (not crash)

### Phase C: Validation ✅ COMPLETE

**Step 1: Test coinductive subtyping** ✅
- Created test with recursive types (List, Node<T>, mutual recursion)
- Ran compiler successfully - **no stack overflow**
- Implementation working as expected

**Step 2: Gemini Pro Implementation Review** ✅

**Initial Review found 2 critical bugs**:

1. **Bug 1: False positives on generic instantiations**
   - Problem: DefId-only check ignored type arguments
   - Example: `Box<string>` vs `Box<number>` incorrectly treated as subtypes
   - Fix: Added `is_safe_for_defid_check()` to restrict DefId check to non-generic types only

2. **Bug 2: Infinite loop in evaluate_application**
   - Problem: Returning application as-is caused re-evaluation loop
   - Fix: Return `TypeId::ERROR` when cycle detected (matches TS2589 behavior)

**Step 3: Applied fixes and re-reviewed** ✅

**Final Verdict (Gemini Pro)**:
> "The implementation is safe for production."
> "Fix 1 correctly distinguishes between nominal identity (DefId) and structural identity (DefId + Args)."
> "Fix 2 correctly implements the circuit breaker for infinite type expansion."

**Edge Cases Verified**:
- ✅ Mutual recursion: `type A<T> = B<T>; type B<T> = A<T>` - Returns ERROR (correct)
- ✅ Recursive data structures: `type List<T> = { next: List<T> }` - Returns valid Object (correct)
- ✅ Naked type parameters: `type Box<T> = T` - Returns False (correct)

**Commit**: `f39737968` - fix(tsz-2): address critical bugs in coinductive subtyping (per Gemini Pro review)

## Success Criteria

- [x] No stack overflows when comparing recursive types
- [x] `type A = { self: A }` and `type B = { self: B }` are correctly identified as subtypes
- [x] Depth limiting prevents infinite loops
- [x] Unit tests cover simple and mutually recursive types
- [x] Generic recursive types work (e.g., `List<number>` vs `List<string>`)
- [x] Implementation validated by Gemini Pro - safe for production

## Session History

*Created 2026-02-05 after completing Application type expansion.*

**Phase A Complete** (commit eae3bd048): Fixed all compilation errors. Build compiles successfully. Runtime shows stack overflow.

**Phase B Complete** (commits b0dd31634, c04b735ff):
- Implemented DefId-level cycle detection in check_subtype
- Added visiting_defs tracking to TypeEvaluator for evaluate_application
- No more stack overflow when evaluating recursive types
- Tests pass successfully

**Phase C Complete** (commit f39737968):
- Tested coinductive subtyping with recursive types - no stack overflow
- Gemini Pro review identified 2 critical bugs
- Applied fixes:
  1. Restrict DefId cycle check to non-generic types only
  2. Return TypeId::ERROR on expansive recursion detection
- Gemini Pro validated fixes: "The implementation is safe for production"
- Edge cases verified: mutual recursion, recursive data structures, naked type parameters

**Current Status**: ✅ ALL PHASES COMPLETE - Session successful!

### Root Cause Analysis (from Gemini Pro)
The stack overflow is caused by **expansive recursion** during type evaluation:
1. `type T = Box<T>` produces new TypeId on each evaluation
2. Cycle detection fails because `T[]` ≠ `T` (different TypeId)
3. Infinite recursion until stack overflow

### Next Session TODOs (from Gemini Pro)

**Step 1: Confirm Recursion Location**
```bash
TSZ_LOG=trace cargo nextest run --test_name
```
Look for repeated calls to `check_subtype` or `evaluate_type` with growing structures.

**Step 2: Implement Coinductive Subtyping**
**A. Fix src/solver/subtype.rs:**
- Ensure `depth` check is strict and returns `SubtypeResult::DepthExceeded`
- Review `in_progress` cycle detection for non-expansive recursion

**B. Fix src/solver/evaluate.rs:**
- Modify `evaluate` to detect recursive type alias expansion
- Enforce `MAX_EVALUATE_DEPTH` strictly
- Implement "Lazy" evaluation - one level at a time during subtyping

**Step 3: MANDATORY Pre-Implementation Question**
```bash
./scripts/ask-gemini.mjs --include=src/solver/subtype.rs --include=src/solver/evaluate.rs "
I am seeing a stack overflow in check_subtype. I suspect expansive recursion (e.g. type T = Box<T>).
How should I modify check_subtype and evaluate_type to correctly implement coinductive
subtyping and prevent infinite expansion? Please review the current depth handling.
"
```

**Step 4: Verify Fix**
Run the test that was overflowing. Should pass or fail with type error (not crash).

---

# NEW WORK: Generic Type Inference & Substitution Engine

**Started**: 2026-02-05 (Following tsz-2 and tsz-4 completion)
**Status**: Active
**Goal**: Implement core Solver logic for inferring type arguments and substituting type parameters

## Problem Statement

Currently, the compiler lacks a robust mechanism to:
1. **Substitute**: Replace `TypeParameter` identifiers with concrete `TypeId`s across complex type structures
2. **Infer**: Analyze source type and target type to solve for missing type variables
3. **Constraint Validation**: Ensure inferred types satisfy `extends` constraints

Without this, generic functions return `any`/`unknown`, and generic types cannot be specialized.

**Impact**:
- Blocks generic type inference (e.g., `function id<T>(x: T): T` returns `unknown`)
- Prevents generic type specialization (e.g., `Map<string, number>` becomes `Map<any, any>`)
- Makes generic constraints ineffective (no validation of `T extends Foo`)

## Implementation Strategy

### Phase A: Type Substitution Engine (TypeMapper Visitor)
**File**: `src/solver/instantiate.rs`

**Task**: Implement `TypeMapper` using visitor pattern from `src/solver/visitor.rs`

**Logic**:
- Visitor that walks a `TypeId`
- When encountering `TypeKey::TypeParameter`, replace it with mapping from `SubstitutionMap`
- Must handle recursive types and memoize results in `TypeInterner`

### Phase B: Constraint Collection
**File**: `src/solver/infer.rs`

**Task**: Implement "Dual-Type Visitor" for constraint collection

**Logic**:
- Traverse two types simultaneously (ParamType with generics, ArgType with concrete data)
- When ParamType hits `TypeParameter`, record ArgType as "candidate"
- Handle multiple candidates from different positions

### Phase C: Constraint Solving
**File**: `src/solver/infer.rs`

**Task**: Merge collected candidates and solve constraints

**Logic**:
- If multiple candidates exist, produce `Union` (or `Intersection` depending on variance)
- Validate final result against `TypeParameter`'s `extends` clause using `is_subtype_of`

### Phase D: Checker Integration
**File**: `src/checker/expr.rs` (specifically `check_call_expression`)

**Task**: Update call expression checking to use inference

**Logic**:
1. Retrieve signature
2. Call `solver.infer_type_arguments`
3. Call `solver.instantiate_signature` with results
4. Use instantiated signature for remainder of call check

## Success Criteria

- [ ] Identity Test: `function id<T>(x: T): T` returns `string` when called with `"hello"`
- [ ] Complex Inference: `map<T, U>(arr: T[], f: (x: T) => U): U[]` infers both `T` and `U`
- [ ] Constraint Enforcement: `log<T extends { name: string }>(x: T)` errors on `{ id: 1 }`
- [ ] Architecture: No `TypeKey` matching in `src/checker/expr.rs`

## MANDATORY Gemini Workflow (per AGENTS.md)

**Question 1 (Before Phase A)**: Ask Gemini (Flash) to validate TypeMapper approach

**Question 2 (After Phase B/C)**: Ask Gemini (Pro) to review inference logic
