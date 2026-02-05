# Session tsz-2: Coinductive Subtyping (Recursive Types)

**Started**: 2026-02-05
**Status**: Active
**Goal**: Implement coinductive subtyping logic to handle recursive types without infinite loops

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

### Phase B: Coinductive Subtyping Implementation 🔄 IN PROGRESS
**Root Cause**: Expansive recursion - `type T = Box<T>` produces new TypeId on each evaluation, bypassing cycle detection

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

### Phase C: Validation ⏳
1. Run conformance tests
2. Test with complex recursive types
3. Ask Gemini Pro to review

## Success Criteria

- [ ] No stack overflows when comparing recursive types
- [ ] `type A = { self: A }` and `type B = { self: B }` are correctly identified as subtypes
- [ ] Depth limiting prevents infinite loops
- [ ] Unit tests cover simple and mutually recursive types
- [ ] Generic recursive types work (e.g., `List<number>` vs `List<string>`)

## Session History

*Created 2026-02-05 after completing Application type expansion.*

**Phase A Complete** (commit eae3bd048): Fixed all compilation errors. Build compiles successfully. Runtime shows stack overflow.

**Current Status**: Phase B - Coinductive Subtyping Implementation

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
