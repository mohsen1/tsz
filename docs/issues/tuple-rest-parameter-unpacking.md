# Issue: Tuple Rest Parameter Unpacking

**Date**: 2026-02-13
**Status**: ✅ IMPLEMENTED (as of recent commits)
**Priority**: ~~HIGH~~ COMPLETE

## Implementation Status

Tuple rest parameter unpacking has been implemented in:
1. **Function subtype checking** (`crates/tsz-solver/src/subtype_rules/functions.rs:297-309`)
2. **Generic inference** (`crates/tsz-solver/src/operations.rs:2197-2208`)

The helper function `unpack_tuple_rest_parameter` in `crates/tsz-solver/src/type_queries.rs:768` handles the unpacking.

**Note**: The remaining failures in `genericFunctionInference1.ts` are due to **higher-order generic function inference** issues (see `generic-function-inference-pipe-pattern.md`), not tuple unpacking.

## Problem Statement

tsz doesn't unpack tuple rest parameters into individual parameters for function type matching, causing massive failures in generic function inference.

## Root Cause

When a function has a rest parameter with a tuple type like `(...args: [A, B]) => R`, TypeScript treats this as equivalent to `(arg0: A, arg1: B) => R` for parameter matching purposes.

tsz currently treats it as a single rest parameter, preventing proper matching against functions with regular parameters.

## Concrete Example

```typescript
// This should work but fails in tsz:
type F1 = (a: string, b: number) => void;
type F2 = (...args: [string, number]) => void;

declare const f1: F1;
const test: F2 = f1;  // ❌ tsz error, ✅ TSC accepts
```

### Generic Function Inference Example

```typescript
declare function pipe<A extends any[], B, C>(
  ab: (...args: A) => B,
  bc: (b: B) => C
): (...args: A) => C;

declare function list<T>(a: T): T[];
declare function box<V>(x: V): { value: V };

// Should infer A = [T], B = T[], C = { value: T[] }
const f01 = pipe(list, box);  // ❌ tsz fails, ✅ TSC works
```

**Why it fails**:
1. `list` has signature `<T>(a: T) => T[]` (1 regular param)
2. `pipe` expects `(...args: A) => B` (rest param)
3. Should infer `A = [T]` by recognizing that 1 param = rest param with single-element tuple
4. But tsz treats them as incompatible parameter structures

## Impact

**Estimated Tests Affected**: 200+ conformance tests
**Examples**:
- `TypeScript/tests/cases/compiler/genericFunctionInference1.ts` - TSC: 1 error, tsz: 50+ errors
- All `pipe` utility type patterns
- Higher-order function inference
- Function composition patterns

## Technical Details

### Current Behavior

In `crates/tsz-solver/src/subtype_rules/functions.rs:228-476`, `check_function_subtype`:

```rust
// Lines 310-320: Treats rest params as fixed_count - 1
let target_fixed_count = if target_has_rest {
    target_params.len().saturating_sub(1)  // ❌ Doesn't unpack tuple
} else {
    target_params.len()
};

// Lines 346-382: Compares fixed parameters pairwise
for i in 0..fixed_compare_count {
    let s_param = &source.params[i];
    let t_param = &target_params[i];
    // ...
}
```

**Problem**: When target has `...args: [A, B]`:
- `target_fixed_count = 0` (because rest param)
- But should be `target_fixed_count = 2` (tuple unpacked)

### TypeScript's Behavior

TypeScript unpacks tuple rest parameters:
- `(...args: [A, B, C])` → 3 fixed params
- `(...args: [A, B, ...C[]])` → 2 fixed params + 1 rest element
- `(...args: [...A[], B])` → rest prefix + 1 fixed param

### Locations to Fix

1. **Function subtype checking**:
   - `crates/tsz-solver/src/subtype_rules/functions.rs:228` - `check_function_subtype`
   - Need to detect tuple rest params and unpack before parameter comparison

2. **Generic inference (constraint collection)**:
   - `crates/tsz-solver/src/operations.rs:2126` - `constrain_types_impl` for functions
   - Line 2262: `for (s_p, t_p) in instantiated_params.iter().zip(t_fn.params.iter())`
   - Need to unpack tuple rest params before zipping

3. **Call resolution**:
   - `crates/tsz-solver/src/operations.rs:576` - `resolve_function_call`
   - May need tuple unpacking for argument matching

## Implementation Strategy

### Phase 1: Helper Function

Create a helper to unpack tuple rest parameters:

```rust
/// Unpack a rest parameter with tuple type into individual fixed parameters.
///
/// Input: `...args: [A, B, C]`
/// Output: `[ParamInfo { type_id: A, optional: false, rest: false },
///          ParamInfo { type_id: B, optional: false, rest: false },
///          ParamInfo { type_id: C, optional: false, rest: false }]`
///
/// Input: `...args: [A, B, ...C[]]`
/// Output: `[ParamInfo { type_id: A }, ParamInfo { type_id: B },
///          ParamInfo { type_id: C, rest: true }]`
fn unpack_tuple_rest_parameter(
    interner: &dyn TypeDatabase,
    param: &ParamInfo
) -> Vec<ParamInfo> {
    if !param.rest {
        return vec![param.clone()];
    }

    // Check if rest type is a tuple
    if let Some(tuple_id) = get_tuple_list_id(interner, param.type_id) {
        let tuple_elements = interner.tuple_list(tuple_id);

        // Convert tuple elements to parameters
        tuple_elements.iter()
            .enumerate()
            .map(|(i, elem)| {
                ParamInfo {
                    name: Some(intern_string(format!("__tuple_{}", i))),
                    type_id: elem.type_id,
                    optional: elem.optional,
                    rest: elem.rest,  // Preserve rest for trailing ...T[]
                }
            })
            .collect()
    } else {
        // Not a tuple - keep as-is
        vec![param.clone()]
    }
}
```

### Phase 2: Update Function Subtype Checking

In `check_function_subtype`:

```rust
// BEFORE fixed parameter comparison:
let source_params_unpacked: Vec<ParamInfo> = source.params.iter()
    .flat_map(|p| unpack_tuple_rest_parameter(self.interner, p))
    .collect();

let target_params_unpacked: Vec<ParamInfo> = target_params.iter()
    .flat_map(|p| unpack_tuple_rest_parameter(self.interner, p))
    .collect();

// Then use unpacked params for comparison
let source_has_rest = source_params_unpacked.last().is_some_and(|p| p.rest);
let target_has_rest = target_params_unpacked.last().is_some_and(|p| p.rest);

let source_fixed_count = if source_has_rest {
    source_params_unpacked.len().saturating_sub(1)
} else {
    source_params_unpacked.len()
};

// ... rest of logic uses unpacked params
```

### Phase 3: Update Generic Inference

In `constrain_types_impl` for function-to-function matching:

```rust
// After instantiating parameters (line 2227):
let instantiated_params_unpacked: Vec<ParamInfo> = instantiated_params.iter()
    .flat_map(|p| unpack_tuple_rest_parameter(self.interner, p))
    .collect();

let target_params_unpacked: Vec<ParamInfo> = t_fn.params.iter()
    .flat_map(|p| unpack_tuple_rest_parameter(self.interner, p))
    .collect();

// Constrain using unpacked params
for (s_p, t_p) in instantiated_params_unpacked.iter().zip(target_params_unpacked.iter()) {
    self.constrain_types(ctx, &combined_var_map, t_p.type_id, s_p.type_id, priority);
}
```

### Phase 4: Testing

**Test cases to validate**:

1. Basic equivalence:
```typescript
type F1 = (a: string) => void;
type F2 = (...args: [string]) => void;
const test1: F2 = (x: string) => {};  // Should work
```

2. Multiple parameters:
```typescript
type F1 = (a: string, b: number, c: boolean) => void;
type F2 = (...args: [string, number, boolean]) => void;
const test2: F2 = (x: string, y: number, z: boolean) => {};  // Should work
```

3. Optional parameters:
```typescript
type F = (...args: [string, number?]) => void;
// Should match (a: string, b?: number) => void
```

4. Mixed tuple with rest:
```typescript
type F = (...args: [string, ...number[]]) => void;
// Should match (a: string, ...rest: number[]) => void
```

5. Generic inference (the original `pipe` example)

## Acceptance Criteria

✅ Basic tuple rest parameter equivalence works
✅ Generic function inference with tuple rest parameters works
✅ The `pipe(list, box)` example passes
✅ `genericFunctionInference1.ts` shows significant improvement (50+ errors → ~1 error)
✅ No regressions in existing passing tests

## Related Issues

- Generic function inference failures (blocks ~200 tests)
- Higher-order function type incompatibility
- Function composition pattern failures

## References

- TypeScript behavior: https://www.typescriptlang.org/docs/handbook/2/functions.html#rest-parameters-and-arguments
- TSC issue discussing this: https://github.com/microsoft/TypeScript/issues/5453
