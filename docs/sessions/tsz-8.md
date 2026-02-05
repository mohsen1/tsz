# Session tsz-8: Advanced Generic Inference & Markers

**Goal**: Complete the generic inference engine with multi-pass resolution and support advanced contextual markers.

**Status**: 🟢 COMPLETE ✅ (2026-02-05)

---

## Context

Session `tsz-3` established the infrastructure for bidirectional inference and priority-based constraints. `tsz-8` implements the core logic that utilizes this infrastructure to match TypeScript's complex inference behavior (multi-pass resolution) and supports `ThisType<T>`.

---

## Phase 7b: Multi-Pass Resolution Logic ✅ COMPLETE

**⚠️ CRITICAL FINDING FROM GEMINI PRO (2026-02-05):**

**TypeScript does NOT iterate through priority passes!**

The algorithm is:
1. **Collect ALL constraints** from arguments (NakedTypeVariable priority) and context (ReturnType priority)
2. **Solve ONCE** - the InferenceContext.resolve() handles priority filtering internally
3. **No loop needed** - priority is metadata attached to candidates, not a multi-pass iteration

**Key Insight**: The name "Multi-Pass" was misleading. It's "Collect All -> Solve Once", where the solver uses priority to pick the best candidate.

### Task 7.2.1: Priority-Aware Constraint Collection ✅ COMPLETE
**File**: `src/solver/operations.rs`
**Status**: ✅ COMPLETE (Already implemented in tsz-3 Phase 7a)

**Discovery**: This functionality was ALREADY implemented during tsz-3 Phase 7a when we refactored `constrain_types` signatures!

**Implementation Verification**:
1. ✅ **Line 667-673**: Contextual type constraints with `InferencePriority::ReturnType`
2. ✅ **Line 717-723**: Argument constraints with `InferencePriority::NakedTypeVariable`
3. ✅ **Line 726-732**: Rest tuple constraints with `InferencePriority::NakedTypeVariable`
4. ✅ **Line 1178**: `filter_candidates_by_priority` uses `.min()` to find highest priority
5. ✅ **Line 1143**: `resolve_from_candidates` uses priority filtering

**Test Verification**:
```typescript
// Test: NakedTypeVariable (argument) should override ReturnType (context)
function identity<T>(x: T): T { return x; }
const result1: string = identity(42);
//    ^^^^^^ Correctly infers 'number' from argument (higher priority)
// Error: TS2322: Type 'number' is not assignable to type 'string' ✅
```

---

## Phase 8: Advanced Markers ✅ COMPLETE

**Goal**: Support `ThisType<T>` for object literal context, enabling "Options API" patterns.

**Why This Matters**: Essential for Vue 2, Pinia, and other libraries using the "Options API" pattern where `this` type is inferred from contextual markers rather than the object structure.

### Task 8.1: `ThisType<T>` Detection & Context ✅ COMPLETE
**Files**: `src/solver/contextual.rs`, `src/checker/type_computation.rs`
**Status**: ✅ COMPLETE (2026-02-05)

**Implementation Summary**:

**Question 1 (Pre-Implementation)**: ✅ Completed - Got architectural guidance from Gemini Pro
- Key finding: `ThisType<T>` is `TypeKey::Application`, not `TypeKey::ThisType`
- Must check for Application where base is global ThisType interface

**Question 2 (Post-Implementation)**: ✅ Completed - Gemini Pro review caught CRITICAL BUG
- **Bug Found**: `is_this_type_application` returned true for ALL Lazy types
- **Impact**: Would have broken `Partial<T>`, `Readonly<T>`, and all generic type aliases
- **Fix Applied**: Changed to fail-safe - return false for unidentifiable Lazy types
- **TODO Added**: Union distribution improvement (Phase 2 limitation)

**Implementation Details**:

1. **Added `ThisTypeMarkerExtractor` visitor** (`src/solver/contextual.rs`):
   - Extracts type `T` from `ThisType<T>` applications
   - Handles intersections: `ThisType<A> & ThisType<B>` → `this` is `A & B`
   - Distributes over unions (with documented limitation)
   - **CRITICAL**: Safely handles Lazy types to avoid breaking other type aliases

2. **Added `get_this_type_from_marker()` to `ContextualTypeContext`**:
   - Public API for Checker to extract this type from markers
   - Returns `Option<TypeId>` with the type `T` from `ThisType<T>`

3. **Updated `get_type_of_object_literal`** (`src/checker/type_computation.rs`):
   - Extract ThisType marker from contextual type before checking properties
   - Push to `this_type_stack` (methods pick it up via existing mechanism)
   - Pop after checking (RAII-like pattern)
   - Safe: No early returns in loop, but pattern is brittle

**Known Limitations**:
- **Union Distribution**: Currently picks first `ThisType` from union
  - Correct: Narrow contextual type based on object shape first
  - Acceptable for Phase 1
  - Documented with TODO for Phase 2

- **Lazy Type Detection**: Cannot identify ThisType without symbol table access
  - Fails safe: Returns false to avoid breaking other type aliases
  - Works for TypeParameter case (direct name check)

**Test Case**:
```typescript
type ObjectDescriptor<D, M> = {
    data?: D;
    methods?: M & ThisType<D & M>;
};
const obj: ObjectDescriptor<{x: number}, {greet(): void}> = {
    data: { x: 0 },
    methods: {
        greet() { this.x; } // 'this' should be D & M
    }
};
```

**Commits**:
- `cf071617b` - Initial implementation
- `2f98a171e` - CRITICAL BUG FIX (Gemini Pro review)

**Both questions of Two-Question Rule completed successfully!**

---

## Phase 9: Stabilization (Time Permitting)

**Goal**: Ensure the new inference engine is robust.

### Task 9.1: Conformance Sweep
**Priority**: MEDIUM
**Status**: ⏸️ PENDING

Run `./scripts/conformance/run.sh --max=500` to identify any regressions caused by the multi-pass logic.

---

## Session History

### Previous Session: tsz-3 (COMPLETE ✅)
- **Phase 5**: CheckerState Integration ✅
- **Phase 6**: Contextual Typing Hardening ✅
- **Phase 7a**: Infrastructure & Signature Refactoring ✅

See `docs/sessions/history/tsz-3.md` for full details.

---

## Coordination Notes

**tsz-1, tsz-2, tsz-3, tsz-4, tsz-5, tsz-6, tsz-7**: Various sessions in progress or complete.

**Priority**: Phase 7b is the critical path. This is the "Final Boss" of generic inference and should be the sole focus until complete.

---

## Gemini Consultation Plan

Following the mandatory Two-Question Rule from `AGENTS.md`:

### Question 1: Algorithm Validation (BEFORE implementation)
See mandatory prompt under Task 7.2.1 above.

### Question 2: Implementation Review (AFTER implementation)
```bash
./scripts/ask-gemini.mjs --pro --include=src/solver/operations.rs \
"I implemented multi-pass generic resolution in resolve_generic_call_inner.

Changes: [PASTE CODE OR DIFF]

Please review:
1) Is this the correct algorithm for TypeScript's multi-pass inference?
2) Did I handle the 'Circular' priority correctly?
3) Are there type system bugs or edge cases I missed?
4) Does this match tsc behavior for complex generic inference?"
```
