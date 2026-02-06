# Session tsz-3: Excess Property Checking (TS2322)

**Started**: 2026-02-06
**Status**: Active - Investigation Phase
**Predecessor**: tsz-3-infer-fix-complete (Conditional Type Inference - COMPLETED)

## Task: Excess Property Checking

**Goal**: Implement or refine Excess Property Checking to eliminate extra TS2322 errors and match TypeScript's behavior exactly.

### Problem

TypeScript allows width subtyping (extra properties) in general, but **prohibits** them in fresh object literals. This is the "Lawyer" layer responsibility - it must detect when the source is a fresh object literal and trigger TS2322 if extra properties exist.

**Current Status**: 68/100 conformance tests passing
- **TS2322**: missing=1, extra=6 (Type not assignable)
- Many of these extra errors may be due to incorrect excess property checking

### Architecture (per NORTH_STAR.md Section 3.3)

**Judge vs Lawyer**:
- **Judge** (`SubtypeChecker` in `src/solver/subtype.rs`): Pure structural subtyping - should allow extra properties
- **Lawyer** (`CompatChecker` in `src/solver/lawyer.rs`): TypeScript-specific quirks including excess property checks

The Lawyer must:
1. Track "freshness" or literal status of object literals
2. Detect when a fresh object literal has properties not in the target type
3. Emit TS2322 for excess properties

### Files to Investigate

1. **`src/solver/lawyer.rs`** - Main compatibility layer
2. **`src/solver/subtype.rs`** - Judge layer (should allow extra properties structurally)
3. **`src/checker/expr.rs`** - Where object literals are created (mark as fresh)

### Test Cases

**Excess property should error**:
```typescript
type T = { a: string };
const x: T = { a: "hello", b: 42 }; // TS2322: Object literal may only specify known properties
```

**Excess property should allow with type assertion**:
```typescript
type T = { a: string };
const x = { a: "hello", b: 42 } as T; // OK - excess properties allowed with type assertion
```

**Excess property should allow in assignment context**:
```typescript
type T = { a: string };
const y = { a: "hello", b: 42 };
const x: T = y; // OK - y is not fresh, so excess properties allowed
```

### MANDATORY Gemini Workflow

Per AGENTS.md, before implementing:

**Question 1 (Approach)**:
```bash
./scripts/ask-gemini.mjs --include=src/solver/lawyer.rs --include=src/checker/expr.rs "I need to implement Excess Property Checking to fix TS2322 errors.

My plan:
1. Add a 'freshness' flag to track object literals created in expr.rs
2. Modify the Lawyer's assignability check to detect fresh object literals with excess properties
3. Emit TS2322 when a fresh literal has properties not in the target type

Is this the correct approach? Which functions should I modify?
What are the edge cases for freshness (e.g., spread, destructuring, generics)?"
```

**Question 2 (Review)**: After implementation, submit for review.

### Investigation Results (2026-02-06)

**Discovery**: Excess Property Checking infrastructure is **already implemented**!
- `ObjectFlags::FRESH_LITERAL` flag exists in `src/solver/types.rs`
- `is_fresh_object_type()` checks the flag in `src/solver/freshness.rs`
- `widen_freshness()` removes the flag in `src/solver/freshness.rs`
- `check_excess_properties()` and `find_excess_property()` exist in `src/solver/compat.rs`
- Object literals are created with `FRESH_LITERAL` flag via `create_fresh_object_literal_type()` in `src/checker/arena.rs`

**Test Results**:
```typescript
type T = { a: string };
const x: T = { a: "hello", b: 42 };  // ✅ TS2353 - Works correctly!
const x = { a: "hello", b: 42 } as T;   // ✅ No error - Works correctly!
```

**Bug Found**: Assignment context incorrectly checks for excess properties
```typescript
const y = { a: "hello", b: 42 };
const z: T = y;  // tsz: TS2353 (WRONG), tsc: No error (CORRECT)
```

The problem: When `y` is created, it has `FRESH_LITERAL` flag. When assigned to `z`, tsz should recognize that `y` is being assigned and **widen** the type (remove `FRESH_LITERAL`), but it's not doing this correctly.

### Root Cause Analysis

The issue is in how freshness is handled during variable assignment. The flow should be:
1. `const y = { a: "hello", b: 42 }` - creates type with `FRESH_LITERAL`
2. `const z: T = y` - should **widen** the type (strip `FRESH_LITERAL`) before checking assignability
3. Assignability check should NOT trigger excess property checking because source is not fresh

### Next Steps

1. Investigate where freshness widening should happen during assignment
2. Check if `widen_freshness()` is being called at the right time
3. Fix the widening logic to match TypeScript's behavior
