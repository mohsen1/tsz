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
**Status**: 🔍 Investigation - Major Discovery: Type Inference Already Implemented!
**Goal**: Investigate and fix issues with generic type inference

## 🚨 Major Discovery (2026-02-05)

**Type inference is already FULLY IMPLEMENTED** in `src/solver/operations.rs`:

### Complete Implementation Found
1. **Type Substitution** (`src/solver/instantiate.rs`):
   - `TypeInstantiator` visitor pattern for type parameter substitution
   - Handles recursive types, cycle detection, shadowing

2. **Type Inference** (`src/solver/infer.rs`):
   - `InferenceVar` with Union-Find algorithm (ena crate)
   - `InferenceContext` with constraint collection and resolution
   - Multi-pass inference with priority levels

3. **Generic Call Resolution** (`src/solver/operations.rs:573-842`):
   - `resolve_generic_call_inner`: Full type inference engine
   - Creates inference variables for each type parameter
   - Uses placeholders for constraint collection
   - **Downward inference**: Seeds constraints from return type (line 667-680)
   - **Upward inference**: Collects constraints from arguments (line 686-752)
   - Resolves inference variables with constraints (line 762-786)
   - Validates inferred types against constraints (line 790-801)
   - Instantiates return type with final substitution (line 840)

### Manual Tests Created
Created `src/checker/tests/generic_inference_manual.rs` with 3 passing tests:
- ✅ `test_identity_function_inference`: Upward inference from arguments
- ✅ `test_constraint_validation`: Type constraint validation
- ✅ `test_downward_inference`: Downward inference from contextual type

### Real Bug Found: Complex Nested Inference
**Issue**: `map<T, U>(arr: T[], f: (x: T) => U): U[]` fails to infer `T` and `U`

**Example**:
```typescript
function map<T, U>(arr: T[], f: (x: T) => U): U[] {
    return arr.map(f);
}

const result = map([1, 2, 3], x => x.toString());
// Error: Property 'toString' does not exist on type 'T'
```

**Root Cause**: When checking `arr.map(f)`:
- `arr` has type `T[]` where `T` is still a TypeParameter (not yet resolved)
- The compiler tries to look up the `map` method signature
- But `T` is not resolved yet, causing the callback parameter to have type `error`

**This is a known complex case**: TypeScript uses **deferred type inference** to handle this, where the type parameters are resolved incrementally during the type checking process.

**Impact**:
- Simple generic functions work correctly (`identity`, `first`, `apply`)
- Complex nested inference fails (`map`, generic callbacks on generic arrays)
- This is the gap that needs to be fixed

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

- [x] Identity Test: `function id<T>(x: T): T` returns `string` when called with `"hello"`
- [x] Constraint Enforcement: `log<T extends { name: string }>(x: T)` errors on `{ id: 1 }`
- [x] Downward Inference: `const x: string = identity(42)` correctly errors
- [ ] **Complex Inference BUG**: `map<T, U>(arr: T[], f: (x: T) => U): U[]` fails to infer `T` and `U`
- [ ] Architecture: No `TypeKey` matching in `src/checker/expr.rs` (needs verification)

## Next Actions

### Immediate: Commit Discovery ✅ COMPLETE
1. ✅ Create manual test suite (`src/checker/tests/generic_inference_manual.rs`)
2. ✅ Fix pre-existing compilation errors in `src/solver/tests/inference_candidates_tests.rs`
3. ✅ Verify basic inference works (3 passing tests)
4. ✅ Document discovery in session file
5. ✅ Commit and push to origin (commit `8cc6b567a`)

### Next: Fix Complex Nested Inference Bug (Multi-Pass Inference)

**Problem**: `map<T, U>(arr: T[], f: (x: T) => U): U[]` fails with "Property 'toString' does not exist on type 'T'"

**Root Cause**: The issue is in how method calls on generic types are resolved when the type parameter is not yet fully resolved.

**Solution** (per Gemini Flash 2026-02-05): **Multi-Pass Inference**

TypeScript uses "Inference Rounds":
1. **Round 1 (Non-contextual)**: Infer from arguments that don't depend on context (like `[1, 2, 3]`)
2. **Fixing**: "Fix" (resolve) the type variables that have enough information
3. **Round 2 (Contextual)**: Use the fixed types to provide contextual types to remaining arguments (like the lambda)

**Implementation Plan**:

#### Phase 1: Modify `resolve_generic_call_inner` (src/solver/operations.rs:573-842)
Split the argument processing loop into two phases:

```rust
// Round 1: Non-contextual arguments
for (i, &arg_type) in arg_types.iter().enumerate() {
    if !self.is_contextually_sensitive(arg_type) {
        // Process non-contextual arguments (arrays, primitives)
        self.constrain_types(...);
    }
}

// Fixing: Resolve variables with enough information
infer_ctx.strengthen_constraints()?;

// Round 2: Contextual arguments
for (i, &arg_type) in arg_types.iter().enumerate() {
    if self.is_contextually_sensitive(arg_type) {
        // Process contextual arguments (lambdas, object literals)
        // Use current inference to instantiate target type
        let current_subst = self.get_current_substitution(&infer_ctx, &var_map);
        let contextual_target = instantiate_type(self.interner, target_type, &current_subst);
        // Re-check lambda with contextual_target
    }
}
```

#### Phase 2: Implement `is_contextually_sensitive`
- Function types / Callables
- Object literals (freshness checking)
- Type parameters with deferred constraints

#### Phase 3: Fixing Mechanism (src/solver/infer.rs)
- Modify `strengthen_constraints` to be callable multiple times
- Ensure `resolve_with_constraints` can return partial results
- Fix variables when they have candidates and no circular dependencies

#### Phase 4: Enforce Priority Order (src/solver/types.rs)
- **Priority 1 (`NakedTypeVariable`)**: Arguments like `arr: T[]`
- **Priority 32 (`ReturnType`)**: Contextual return types
- **Deferred**: Lambdas and object literals

**References**:
- Pierce & Turner: "Local Type Inference" paper
- TypeScript Spec: "Local Type Inference" section
- `tsc` source: `checkCalls.ts` → `inferTypeArguments`

**Key Files to Modify**:
- `src/solver/operations.rs:573-842` - `resolve_generic_call_inner`
- `src/solver/infer.rs` - `InferenceContext` fixing mechanism
- `src/solver/types.rs` - `InferencePriority` enforcement

**Test Case**:
```typescript
function map<T, U>(arr: T[], f: (x: T) => U): U[] {
    return arr.map(f);
}

const result = map([1, 2, 3], x => x.toString());
// Should infer T = number, U = string
```

## MANDATORY Gemini Workflow (per AGENTS.md)

**Question 1 (Before Phase A)**: Ask Gemini (Flash) to validate TypeMapper approach

**Question 2 (After Phase B/C)**: Ask Gemini (Pro) to review inference logic
