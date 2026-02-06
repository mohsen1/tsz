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

### Gemini Consultation (2026-02-06)

**Question**: Where should widen_freshness() be called during variable assignment?

**Gemini Response** (Pro model):
- **Location**: `src/checker/declarations.rs` (or state_checking_members.rs which delegates to it)
- **Function**: `check_variable_declaration` (specifically the part handling initializer type inference)
- **Fix**: When a variable has no type annotation, widen the inferred type before storing it

```rust
// Pseudocode for the fix:
if let Some(initializer) = node.initializer {
    let inferred_type = self.check_expression(initializer);

    // FIX: Widen freshness so 'y' doesn't trigger excess property checks later
    let final_type = if node.type_annotation.is_none() {
        use crate::solver::freshness::widen_freshness;
        widen_freshness(self.ctx.types, inferred_type)
    } else {
        inferred_type
    };

    self.ctx.register_type_for_symbol(symbol_id, final_type);
}
```

**Key Insight**: Freshness is transient - lost immediately upon assignment. The variable `y` itself should not have a fresh type.

### Implementation Status

**Pending**: Need to locate where variable types are registered and add the widening call.
- Variable declaration checking appears to be in `state_checking_members.rs`
- Need to find where types are registered for symbols
- Add `widen_freshness()` call when inferring types from initializers without type annotations

### Test Cases

**Should pass after fix**:
```typescript
const y = { a: "hello", b: 42 };
const z: T = y;  // Should NOT error - y should be widened
```

**Should still error**:
```typescript
const x: T = { a: "hello", b: 42 };  // Should error - direct literal is fresh
```
