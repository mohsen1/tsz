# Session tsz-3 - Array & Tuple Type Inference

**Started**: 2026-02-04
**Goal**: Improve array and tuple type inference to match TypeScript behavior

## Problem Statement

Array and tuple type inference is fundamental to TypeScript conformance. Correctly distinguishing between `string[]` and `[string, number]` based on context, handling spreads, and computing "Best Common Type" for array elements are high-frequency operations that affect many tests.

## Scope

### 1. Contextual Tuple Inference
- Ensure array literals infer as Tuples when the contextual type is a Tuple
- Files: `src/checker/array_literals.rs`, `src/checker/type_computation.rs`
- Example: `let x: [number, number] = [1, 2]` should infer as tuple, not array

### 2. Best Common Type
- Verify `compute_best_common_type` correctly handles unions and subtypes
- Example: `let x = [1, "a"]` should infer `(string | number)[]`
- Example: `let x = [1, null]` should infer `(number | null)[]` (with strictNullChecks)

### 3. Spread Handling
- Validate spread elements in array literals correctly flatten types
- Example: `[...string[]]` should result in `string[]`
- Example: `[...[number, boolean]]` should result in `(number | boolean)[]` or preserve tuple structure

### 4. Readonly/Const
- Ensure `as const` infers `readonly` tuples/arrays
- Ensure readonly contexts infer correct types

## Test Cases

```typescript
// Contextual tuple inference
let x: [number, number] = [1, 2]; // Should be tuple, not array
let y: [number, string] = [1, "a"]; // Should work

// Best common type
let a = [1, "a"]; // Should be (string | number)[]
let b = [1, null]; // Should be (number | null)[] with strictNullChecks

// Spread
let arr: string[] = ["a", "b"];
let spread1 = [...arr]; // Should be string[]
let spread2 = [...[1, true]]; // Should be (number | boolean)[]
```

## Files to Focus On

- `src/checker/array_literals.rs` - Core array literal type checking
- `src/checker/type_computation.rs` - `get_type_of_array_literal`
- `src/checker/tuple_type.rs` - Tuple type utilities
- `src/checker/spread.rs` - Spread expression handling

## Progress

- [ ] Test current behavior with array/tuple inference
- [ ] Identify gaps in contextual tuple inference
- [ ] Fix best common type computation
- [ ] Verify spread handling
- [ ] Run conformance tests to measure impact

## Session Coordination

- **tsz-1**: Parser TS1005 errors (and CFA in tsz-4)
- **tsz-2**: Module resolution
- **tsz-3**: Array & Tuple inference (ACTIVE)
- **tsz-4**: Control Flow Analysis (taken by tsz-1)

No conflicts with active sessions.
