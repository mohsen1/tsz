# Session tsz-8: Advanced Generic Inference & Markers

**Goal**: Complete the generic inference engine with multi-pass resolution and support advanced contextual markers.

**Status**: 🟡 PLANNING (2026-02-05)

---

## Context

Session `tsz-3` established the infrastructure for bidirectional inference and priority-based constraints. `tsz-8` implements the core logic that utilizes this infrastructure to match TypeScript's complex inference behavior (multi-pass resolution) and supports `ThisType<T>`.

---

## Phase 7b: Multi-Pass Resolution Logic (CRITICAL)

**Goal**: Implement priority-gated constraint collection in generic call resolution. This prevents lower-priority constraints (like loose contextual types) from polluting inference when higher-priority candidates (like explicit arguments) exist.

### Task 7.2.1: Multi-Pass Loop in `resolve_generic_call`
**File**: `src/solver/operations.rs`
**Priority**: HIGH
**Status**: 🟡 READY TO START

**Description**: Refactor `resolve_generic_call_inner` to iterate through inference priorities.

**Requirements**:
1. **Priority Loop**: Iterate from highest priority to lowest.
2. **Gated Collection**: In each pass, only collect constraints that match the current priority.
3. **State Management**: Decide whether to accumulate constraints or reset between passes (Ask Gemini).
4. **Circular Handling**: Implement the "Circular" priority logic to break infinite inference loops.

**Mandatory Gemini Prompt** (Two-Question Rule - Question 1):
```bash
./scripts/ask-gemini.mjs --pro --include=src/solver/operations.rs --include=src/solver/infer.rs \
"I am implementing the multi-pass generic resolution loop in resolve_generic_call_inner.
I have the InferencePriority enum and the constrain_types signature ready.

Please explain the exact algorithm:
1. What is the order of passes? (e.g., ReturnType -> Homomorphic -> etc.)
2. Do we solve constraints after *every* pass, or collect all then solve?
3. How does TypeScript handle the 'Circular' priority?
4. Provide a pseudocode skeleton for the loop."
```

---

## Phase 8: Advanced Markers

**Goal**: Support `ThisType<T>` for object literal context, enabling "Options API" patterns.

### Task 8.1: `ThisType<T>` Detection & Context
**Files**: `src/checker/type_computation.rs`, `src/checker/context.rs`
**Priority**: MEDIUM-HIGH
**Status**: ⏸️ DEFERRED (after Phase 7b)

**Description**:
1. **Detection**: In `get_type_of_object_literal`, check if the contextual type contains `ThisType<T>` (usually via intersection).
2. **Extraction**: Extract the type argument `T`.
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
