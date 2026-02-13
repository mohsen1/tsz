# Mapped Type Inference Issue

## Status
**DIAGNOSED** - Root cause identified

## Problem Summary
When a generic function has a mapped type in its parameter position, tsz fails to infer the type parameter, returning `unknown` instead.

## Test Case
```typescript
interface Point { x: number; y: number }
declare let p: Point;

type Identity<T> = { [K in keyof T]: T[K] }

declare function id<T>(arg: Identity<T>): T;

const result = id(p);
// TSC: result is Point
// tsz: result is unknown
```

## Root Cause Analysis

### Constraint Generation Flow (operations.rs:2069-2074)
When `id(p)` is called:
1. We need to constrain: `Point <: Identity<T>` to infer `T`
2. Code path: target is `Identity<T>` (mapped type)
3. We call `evaluate_mapped` to expand it
4. **BUG**: `evaluate_mapped` returns the mapped type deferred because:
   - `keyof T` can't be evaluated (T is inference variable)
   - Lines 186-194: returns `mapped.clone()` when keys can't be extracted
5. We end up constraining: `Point <: Identity<T>` (still deferred)
6. No constraints are generated for `T`
7. `T` resolves to `unknown`

### Why Evaluation is Deferred (evaluate_rules/mapped.rs:186-194)

```rust
let key_set = match self.extract_mapped_keys(keys) {
    Some(keys) => keys,
    None => {
        // Can't extract keys because T is unknown
        return self.interner().mapped(mapped.clone());
    }
};
```

## Expected Behavior (TypeScript)

TypeScript performs **inverse/reverse inference** for mapped types:
- Sees `Point <: Identity<T>`
- Recognizes `Identity<T>` is homomorphic identity mapping
- Infers `T = Point` by "reversing" the mapping

## Solution Approach

### Option 1: Inverse Mapped Type Inference
Before evaluating the mapped type, check if:
1. Target is a mapped type containing inference variables
2. Source is a concrete object type
3. Mapped type is homomorphic (like `Identity<T>`)

Then perform reverse inference:
- For `{ [K in keyof T]: T[K] }` matched against object type `S`
- Infer `T = S`

### Option 2: Structural Constraint Generation
When we can't evaluate a mapped type:
- Generate structural constraints property-by-property
- For each property in source, constrain against template instantiation
- Example: `Point.x <: T["x"]` where template is `T[K]`

### Option 3: Defer to Post-Processing
- Mark mapped types with inference vars as "pending"
- After initial inference, revisit and resolve
- More complex, could cause issues with interdependent constraints

## Key Code Locations

- **Constraint generation**: `crates/tsz-solver/src/operations.rs:2069-2074`
- **Mapped type evaluation**: `crates/tsz-solver/src/evaluate_rules/mapped.rs:186-194`
- **Key extraction**: `crates/tsz-solver/src/evaluate_rules/mapped.rs:183`

## Impact
- Blocks `mappedTypeRecursiveInference.ts` conformance test
- Affects any generic function with mapped type parameters
- Essential for utility type patterns: `Partial<T>`, `Required<T>`, etc.

## Next Steps
1. Implement inverse inference for homomorphic mapped types
2. Add test cases for various mapped type patterns
3. Verify no regressions with `cargo nextest run`
