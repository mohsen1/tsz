# Mapped Type Inference Issue

## Status
**BLOCKED** - Root cause identified, requires architectural changes

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

## Investigation Results (2026-02-13)

### Attempted Fix #1: Evaluate Applications in `constrain_types`
Added cases to evaluate Application types on source/target sides before constraining.

**Result**: Failed - Application evaluation returns the same unevaluated type

### Attempted Fix #2: Manual instantiation in `constrain_types`
Tried to bypass evaluation by directly calling `resolve_lazy()` and `get_lazy_type_params()`.

**Result**: Failed - these methods return `None` during constraint generation

### Root Cause: Architectural Timing Issue

The fundamental problem is **initialization order**:

1. **During constraint generation** (when `id(p)` is type-checked):
   - `Identity<__infer_N>` Application type is created
   - `resolve_lazy(DefId(21749))` returns `None`
   - `get_lazy_type_params(DefId(21749))` returns `None`
   - Type info not yet registered with resolver

2. **Later in execution** (confirmed via tracing):
   - Same `DefId(21749)` CAN be resolved
   - Type params ARE available
   - Evaluation succeeds

3. **Constraint generation happens BEFORE type registration**:
   - Function parameters are instantiated early
   - Type aliases used in those parameters aren't fully registered yet
   - Application evaluator can't expand them

### What's Needed

This requires **architectural changes** to the type registration system:

1. **Option A**: Defer constraint generation until all type definitions are registered
2. **Option B**: Two-phase type checking - register all definitions, then check constraints
3. **Option C**: Lazy constraint evaluation - defer Application constraints until types are ready

All three options require significant refactoring of the checker/solver interaction.

### Workaround

Users can explicitly annotate return types to avoid the inference:
```typescript
function id<T>(arg: Identity<T>): T {
  return arg as T;  // Explicit cast
}

const result: Point = id(p);  // Explicit annotation
```

## Next Steps
1. Design type registration architecture to ensure definitions are available during constraint generation
2. Implement chosen approach (likely Option B - two-phase checking)
3. Add test cases for homomorphic mapped type inference
4. Verify no regressions
