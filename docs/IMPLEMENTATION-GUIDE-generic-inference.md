# Implementation Guide: Generic Function Inference Fix

## Quick Reference for Implementation

This guide provides step-by-step instructions for implementing the fix for generic function inference in the pipe pattern.

## Issue Overview

**Location**: `crates/tsz-solver/src/operations.rs:2271-2445`
**Function**: `constrain_types_impl` - case `(TypeKey::Function, TypeKey::Function)` with generic source
**Test**: `tmp/pipe_simple.ts` for verification
**Impact**: Fixes ~100+ conformance tests

## Current Behavior (Bug)

When constraining `list<T>` against `ab: (...args: A) => B`:
1. Creates fresh inference variable `__infer_src_1` for `T`
2. Instantiates source: `(a: __infer_src_1) => __infer_src_1[]`
3. Constrains: `__infer_src_1[] <: B`
4. **BUG**: `__infer_src_1` has no constraints
5. Resolves: `__infer_src_1` → `unknown`, therefore `B` → `unknown[]`

## Solution Approach: Defer Instantiation

### Core Idea

Don't immediately instantiate generic functions with fresh variables. Instead, recognize when:
- Source is a generic function
- Target contains inference variables
- No concrete arguments are being passed to source

In this case, preserve the generic relationship.

### Implementation Steps

#### Step 1: Add Detection Logic

In `constrain_types_impl`, before line 2281:

```rust
// Before creating fresh inference variables, check if we should defer
if !s_fn.type_params.is_empty() {
    // Check if target function type contains inference variables from var_map
    let target_has_inference_vars = self.function_type_has_inference_vars(t_fn_id, var_map);

    if target_has_inference_vars {
        // Option: Try to constrain generically instead of instantiating
        return self.constrain_generic_function_generically(
            ctx, var_map, s_fn_id, t_fn_id, priority
        );
    }
}
```

#### Step 2: Implement Helper

Add new method to check if function type contains inference variables:

```rust
fn function_type_has_inference_vars(
    &self,
    fn_id: FunctionId,
    var_map: &FxHashMap<TypeId, InferenceVar>,
) -> bool {
    let fn_sig = self.interner.function_shape(fn_id);

    // Check parameters
    for param in &fn_sig.params {
        if self.type_contains_inference_var(param.type_id, var_map) {
            return true;
        }
    }

    // Check return type
    if self.type_contains_inference_var(fn_sig.return_type, var_map) {
        return true;
    }

    false
}

fn type_contains_inference_var(
    &self,
    type_id: TypeId,
    var_map: &FxHashMap<TypeId, InferenceVar>,
) -> bool {
    if var_map.contains_key(&type_id) {
        return true;
    }

    // Recursively check structure (arrays, tuples, etc.)
    // Use existing type_contains_placeholder logic as reference
    // ...
}
```

#### Step 3: Implement Generic Constraint

Add method to constrain without instantiation:

```rust
fn constrain_generic_function_generically(
    &mut self,
    ctx: &mut InferenceContext,
    var_map: &FxHashMap<TypeId, InferenceVar>,
    source_fn_id: FunctionId,
    target_fn_id: FunctionId,
    priority: InferencePriority,
) {
    let source_fn = self.interner.function_shape(source_fn_id);
    let target_fn = self.interner.function_shape(target_fn_id);

    // For pipe(list, box):
    // source: <T>(a: T) => T[]
    // target: (b: B) => C
    // We want to infer B from the structure without losing genericity

    // Key insight: The return type of source should constrain the
    // parameter type of target, but we need to preserve the generic
    // relationship through the type variable system.

    // One approach: Add the source function type itself as a candidate
    // for the inference variable, letting the system figure out the
    // instantiation later.

    // This is complex - see Alternative Approach below
}
```

### Alternative Approach: Preserve Function Type

Instead of instantiating, preserve the generic function as-is:

```rust
// In the location where we would instantiate with fresh vars:
if target_has_inference_vars && !s_fn.type_params.is_empty() {
    // Don't instantiate. Instead, add the generic function type itself
    // as a candidate for inference.

    // If target param is (b: B) => C and source is <T>(a: T) => T[],
    // we can infer that when called with argument type X,
    // the source will return X[], so B should be X[] for some X.

    // This requires adding the source function type to the candidates
    // without instantiation.

    // Create a constraint that says: source function is assignable to target
    // Let the inference system work it out during resolution.

    return; // Skip the instantiation logic below
}
```

## Testing Strategy

### 1. Minimal Test Case

First, ensure `tmp/pipe_simple.ts` passes:

```bash
.target/dist-fast/tsz tmp/pipe_simple.ts
# Should produce: No errors
```

### 2. Full Test

Run the complete failing test:

```bash
.target/dist-fast/tsz TypeScript/tests/cases/compiler/genericFunctionInference1.ts
# Compare with: cat TypeScript/tests/baselines/reference/genericFunctionInference1.errors.txt
# Should have: 1 error (TS2345 on line 138), not 50+
```

### 3. Regression Check

```bash
cargo nextest run
# All tests must pass
```

### 4. Conformance Impact

```bash
./scripts/conformance.sh run --max 100
# Should see improvement in pass rate
```

## Key Challenges

### Challenge 1: When to Instantiate

Not all generic function arguments should be preserved. Need heuristic:
- ✓ Preserve when: Target has inference vars AND no concrete args passed to source
- ✗ Don't preserve when: Source is being called with concrete arguments

### Challenge 2: Inference Resolution

The inference system needs to handle:
- Generic functions as candidates
- Delayed instantiation
- Proper constraint propagation

### Challenge 3: Circular Dependencies

Avoid infinite loops when:
- Generic functions reference each other
- Recursive type definitions
- Use existing recursion depth limits

## References

### TypeScript Source Code

Key files to study:
- `src/compiler/checker.ts`: `inferTypes`, `getInferenceMapper`
- Search for: "higher rank", "polymorphic", "generic instantiation"

### tsz Code Locations

- Current bug location: `crates/tsz-solver/src/operations.rs:2271-2445`
- Inference context: `crates/tsz-solver/src/infer.rs`
- Type instantiation: `crates/tsz-solver/src/instantiate.rs`
- Function shapes: `crates/tsz-solver/src/types.rs`

### Related Issues

- Higher-rank polymorphism (Haskell, ML)
- Rank-2 types
- First-class polymorphism

## Verification Commands

```bash
# Build
cargo build --profile dist-fast -p tsz-cli

# Test minimal case
.target/dist-fast/tsz tmp/pipe_simple.ts

# Test full case
.target/dist-fast/tsz TypeScript/tests/cases/compiler/genericFunctionInference1.ts | wc -l
# Should see far fewer errors

# Unit tests
cargo nextest run

# Conformance
./scripts/conformance.sh run --max 200 | grep "FINAL RESULTS"
```

## Success Criteria

- ✅ `tmp/pipe_simple.ts` produces no errors
- ✅ `genericFunctionInference1.ts` produces 1 error (not 50+)
- ✅ All unit tests pass (2,394 tests)
- ✅ Conformance pass rate improves by at least 5%
- ✅ No new regressions in passing tests

## Estimated Complexity

**Time**: 3-5 hours for careful implementation
**Risk**: Medium - touches core inference logic
**Impact**: High - unblocks ~100+ tests

## Notes

- Read `docs/HOW_TO_CODE.md` before implementing
- Use tracing, not `eprintln!`
- Measure performance before/after if changing hot paths
- Commit frequently with clear messages
- Run `git pull --rebase origin main && git push origin main` after each commit
