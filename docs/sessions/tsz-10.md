# Session tsz-10: Control Flow Analysis & Comprehensive Narrowing

**Goal**: Implement the full CFA pipeline from Binder flow nodes to Solver narrowing logic.

**Status**: 🟡 PLANNING (2026-02-05)

---

## Context

Sessions **tsz-3**, **tsz-8**, and **tsz-9** have established robust type system infrastructure:
- ✅ Contextual typing and bidirectional inference
- ✅ Priority-based generic inference
- ✅ ThisType<T> marker support
- ✅ Conditional type evaluation (840 lines, production-ready)

The **next critical priority** is Control Flow Analysis (CFA) and narrowing. This is the "missing link" that connects the Binder's flow graph to the Solver's type logic, enabling TypeScript's sophisticated type narrowing features.

---

## Why This Matters

Control Flow Analysis is essential for TypeScript's type safety:
- **Type Guards**: `typeof x === "string"` narrows `x` to `string`
- **Truthiness**: `if (x)` narrows `x` to non-null/non-undefined
- **Property Access**: `if (user.address)` narrows to objects with `address` property
- **Assignment Narrowing**: `let x = ...; if (cond) x = ...;` tracks type changes
- **Exhaustiveness**: Switch statements must handle all union members

Without comprehensive CFA, the compiler cannot catch many type errors that tsc would catch.

---

## Phase 1: Type Guard Narrowing (HIGH PRIORITY)

**Goal**: Implement `typeof` and `instanceof` narrowing in the Solver.

### Task 1.1: `typeof` Narrowing
**File**: `src/solver/narrowing.rs`
**Priority**: HIGH
**Status**: ⏸️ READY TO START

**Description**: Implement narrowing based on `typeof` type guards.

**Examples**:
```typescript
function foo(x: string | number) {
    if (typeof x === "string") {
        x.toLowerCase(); // x is string
    } else {
        x.toFixed(2);    // x is number
    }
}
```

**Mandatory Pre-Implementation Question** (Two-Question Rule):
```bash
./scripts/ask-gemini.mjs --pro --include=src/solver/narrowing.rs --include=src/checker \
"I am implementing typeof narrowing for TypeScript.

CURRENT STATE:
- src/solver/narrowing.rs exists with some narrowing functions
- Binder creates flow nodes with type guard information

PLANNED APPROACH:
1. Extract typeof check information from flow nodes
2. Narrow union types based on typeof result
3. Handle string/number/boolean/symbol/undefined/object

QUESTIONS:
1. What is the exact algorithm for narrowing based on typeof?
2. How do I handle 'object' (matches everything except primitives)?
3. Where do I integrate with the flow analysis?
4. Provide the implementation structure."
```

### Task 1.2: `instanceof` Narrowing
**File**: `src/solver/narrowing.rs`
**Priority**: HIGH
**Status**: ⏸️ DEFERRED (after Task 1.1)

**Description**: Implement narrowing based on `instanceof` type guards.

**Examples**:
```typescript
function foo(x: string | Date) {
    if (x instanceof Date) {
        x.getTime(); // x is Date
    } else {
        x.toLowerCase(); // x is string
    }
}
```

**Mandatory Pre-Implementation Question**:
```bash
./scripts/ask-gemini.mjs --pro --include=src/solver/narrowing.rs \
"I am implementing instanceof narrowing.

QUESTIONS:
1. How do I check if a type is an instance of a class?
2. What about interfaces (instanceof doesn't work)?
3. How do I handle class hierarchies (Dog extends Animal)?
4. Provide the algorithm."
```

---

## Phase 2: Property Access & Assignment Narrowing (HIGH PRIORITY)

**Goal**: Implement narrowing for property existence checks and variable reassignments.

### Task 2.1: Property Access Narrowing
**File**: `src/solver/narrowing.rs`
**Priority**: HIGH
**Status**: ⏸️ DEFERRED (after Phase 1)

**Description**: Implement narrowing when checking for property existence.

**Examples**:
```typescript
function foo(x: { a: number } | { b: string }) {
    if ('a' in x) {
        x.a; // x is { a: number }
    } else {
        x.b; // x is { b: string }
    }
}
```

### Task 2.2: Assignment Narrowing
**File**: `src/checker/control_flow.rs`, `src/solver/narrowing.rs`
**Priority**: MEDIUM-HIGH
**Status**: ⏸️ DEFERRED (after Task 2.1)

**Description**: Track type changes across variable reassignments.

**Examples**:
```typescript
let x: string | number;
x = "hello";
if (typeof x === "string") {
    // x is string (from assignment, not just typeof)
}
x = 42;
// x is now number
```

---

## Phase 3: Truthiness & Falsiness Narrowing (MEDIUM PRIORITY)

**Goal**: Implement narrowing based on truthiness checks.

### Task 3.1: Truthiness Narrowing
**File**: `src/solver/narrowing.rs`
**Priority**: MEDIUM
**Status**: ⏸️ DEFERRED (after Phase 2)

**Description**: Narrow types in truthy/falsy branches.

**Examples**:
```typescript
function foo(x: string | null | undefined) {
    if (x) {
        x.toLowerCase(); // x is string (not null/undefined)
    }
}
```

**Note**: TypeScript's truthiness narrowing is complex - it doesn't narrow primitive types, only literals and unions.

---

## Phase 4: Exhaustiveness Checking (MEDIUM PRIORITY)

**Goal**: Ensure switch statements handle all union members.

### Task 4.1: Switch Exhaustiveness
**File**: `src/checker/`
**Priority**: MEDIUM
**Status**: ⏸️ DEFERRED (after Phase 3)

**Description**: Check that switches cover all union members.

**Examples**:
```typescript
type Shape = { kind: 'circle', radius: number } | { kind: 'square', side: number };

function area(shape: Shape) {
    switch (shape.kind) {
        case 'circle': return Math.PI * shape.radius ** 2;
        case 'square': return shape.side ** 2;
        // Error: Not all code paths return a value (missing default)
    }
}
```

---

## Coordination Notes

**tsz-1, tsz-2, tsz-4, tsz-5, tsz-6, tsz-7**: Check docs/sessions/ for status.

**tsz-3, tsz-8, tsz-9**: Complete (contextual typing, ThisType, conditional types)

**Priority**: This session (CFA & Narrowing) is **HIGH PRIORITY** because it's essential for matching tsc's type safety.

---

## Complexity Assessment

**Overall Complexity**: **VERY HIGH**

**Why Very High**:
- CFA requires deep integration between Binder (flow graph) and Solver (narrowing)
- Many edge cases in type narrowing (intersections, unions, generics)
- Assignment narrowing requires tracking variable state across control flow
- High risk of subtle bugs (reversed subtype checks, incorrect narrowing)

**Mitigation**:
- Follow Two-Question Rule strictly for ALL changes
- Test with real TypeScript codebases
- Incremental implementation with thorough testing

---

## Gemini Consultation Plan

Following the mandatory Two-Question Rule from `AGENTS.md`:

### For Each Major Task:
1. **Question 1** (Pre-Implementation): Algorithm validation
2. **Question 2** (Post-Implementation): Code review

**CRITICAL**: Type narrowing bugs are subtle and can cause false negatives (missing errors). Use Gemini Pro for all reviews.

---

## Architectural Notes

**From NORTH_STAR.md**:
- **Solver-First Architecture**: Narrowing logic belongs in the Solver
- **TypeKey Pattern Matching**: Checker should NOT pattern match on TypeKey (Rule 3.2.1)
- **Flow Nodes**: Binder creates flow graph, Solver uses it for narrowing

**Pre-Session Audit**:
Before starting, verify that no TypeKey pattern matching is happening in the Checker. If found, refactor to use TypeResolver.
