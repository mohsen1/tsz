# Session tsz-8: Advanced Generic Inference & Markers

**Goal**: Complete the generic inference engine with multi-pass resolution and support advanced contextual markers.

**Status**: 🟡 IN PROGRESS (2026-02-05)

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

## Phase 8: Advanced Markers (ACTIVE)

**Goal**: Support `ThisType<T>` for object literal context, enabling "Options API" patterns.

**Why This Matters**: Essential for Vue 2, Pinia, and other libraries using the "Options API" pattern where `this` type is inferred from contextual markers rather than the object structure.

### Task 8.1: `ThisType<T>` Detection & Context
**Files**: `src/checker/type_computation.rs`, `src/checker/context.rs`, `src/solver/types.rs`
**Priority**: HIGH
**Status**: 🟡 IN PROGRESS (Awaiting Gemini validation)

**Description**:
1. **Detection**: In `get_type_of_object_literal`, check if the contextual type contains `ThisType<T>` (usually via intersection).
2. **Extraction**: Extract the type argument `T` using Solver utilities.
3. **Propagation**: Push `T` onto a `this_type_stack` in `CheckerContext` before checking properties.
4. **Resolution**: When checking `this` expressions, consult the stack.

**Test Case**:
```typescript
type ObjectDescriptor<D, M> = {
    data?: D;
    methods?: M & ThisType<D & M>;
};
function makeObject<D, M>(desc: ObjectDescriptor<D, M>): D & M { ... }
makeObject({
    data: { x: 0 },
    methods: {
        move() { this.x++; } // 'this' should be D & M
    }
});
```

**Mandatory Pre-Implementation Question (Two-Question Rule)**:
```bash
./scripts/ask-gemini.mjs --pro --include=src/solver --include=src/checker \
"I am starting Phase 8: ThisType<T> support.
Problem: Object literals need to resolve 'this' based on the ThisType<T> marker in the contextual type.

Planned Approach:
1. Solver: Ensure TypeKey::ThisType is correctly handled in the visitor and interner.
2. Checker: In 'get_type_of_object_literal', use a visitor to find 'ThisType<T>' within the contextual type.
3. Checker: If found, extract 'T' and push it onto a 'this_context_stack' in CheckerContext.
4. Checker: When checking MethodDeclarations or FunctionExpressions within that object, resolve 'this' from the stack.

Questions:
1. Is this the correct way to detect ThisType (especially when nested in Intersections/Unions)?
2. Should the Solver handle the extraction of T from ThisType<T>, or should the Checker do it?
3. Are there edge cases with generic ThisType<T> where T is still being inferred?"

Please provide architectural guidance and any edge cases I should handle.
```

**Architectural Notes** (from Gemini):
- **The "Where" (Checker)**: Object literal checking should identify the marker
- **The "What" (Solver)**: Should provide utility to find/extract ThisType from complex types
- **The "Who" (Binder)**: Handles `this` symbol, but Checker overrides its type based on context
- **Warning**: Be careful with inference - if `ThisType<T>` contains a type parameter being inferred from the same object, ensure inference happens before applying the `this` type

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
