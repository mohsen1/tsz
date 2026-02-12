# Type Guard Predicate Investigation

## Problem Statement

Array.find() and similar methods with type guard predicates don't narrow the return type correctly.

## Test Case

```typescript
function isNumber(x: any): x is number {
  return typeof x === "number";
}

const arr = ["string", false, 0];
const result: number | undefined = arr.find(isNumber);
// ❌ tsz: Type 'string | boolean | number | undefined' is not assignable to 'number | undefined'
// ✅ tsc: no error
```

## Expected Behavior

When `Array<T>.find()` is called with a type guard `(x: T) => x is S`, the return type should be `S | undefined`, not `T | undefined`.

## TypeScript Signature

```typescript
interface Array<T> {
    find<S extends T>(
        predicate: (value: T, index: number, obj: T[]) => value is S,
        thisArg?: any
    ): S | undefined;

    find(
        predicate: (value: T, index: number, obj: T[]) => unknown,
        thisArg?: any
    ): T | undefined;
}
```

## Root Cause Analysis

### Initial Hypothesis (INCORRECT)

Initially thought the issue was in `crates/tsz-solver/src/operations.rs:990-991`:
```rust
let return_type = instantiate_type(self.interner, func.return_type, &final_subst);
CallResult::Success(return_type)
```

Assumed we were ignoring `func.type_predicate` when computing return types.

### Actual Issue (CORRECT)

The problem is more subtle. The overload resolution and type argument inference need to work together:

1. **Overload Selection**: There are two `find()` overloads - one with type predicate, one without
2. **Type Argument Inference**: The first overload has a type parameter `S extends T` that must be inferred from the type guard predicate
3. **Return Type Computation**: Once `S` is inferred, the return type becomes `S | undefined`

The bug is likely in how we infer `S` from a callback with a type predicate.

## Investigation Steps

### Step 1: Test explicit overloads

```typescript
declare function find<T, S extends T>(
  arr: T[],
  predicate: (x: T) => x is S
): S | undefined;

declare function find<T>(
  arr: T[],
  predicate: (x: T) => boolean
): T | undefined;

function isNumber(x: any): x is number {
  return typeof x === "number";
}

const arr = [1, "hello", true];
const result: number | undefined = find(arr, isNumber);
```

**Result**: ✅ This works correctly! So our overload resolution and type inference work for explicit function overloads.

### Step 2: Test method call

```typescript
function isNumber(x: any): x is number {
  return typeof x === "number";
}

const arr = [1, "hello", true];
const result: number | undefined = arr.find(isNumber);
```

**Result**: ❌ This fails! Returns `string | number | boolean | undefined` instead of `number | undefined`.

### Conclusion

The issue is specific to **method calls on generic type instances** (like `Array<T>.find()`), not with type guards or overload resolution in general.

## Where to Look

### 1. Method Signature Resolution

When we access `.find` on an `Array<string | number | boolean>`, we need to:
1. Look up the `find` method on the Array interface
2. Instantiate its type with `T = string | number | boolean`
3. Get the instantiated signature(s)

**File**: `crates/tsz-checker/src/state_checking_members.rs` or similar

### 2. Type Parameter Substitution in Overloads

When instantiating Array<T>'s methods:
```typescript
// Original:
find<S extends T>(predicate: (value: T) => value is S): S | undefined

// After instantiation with T = string | number | boolean:
find<S extends string | number | boolean>(
    predicate: (value: string | number | boolean) => value is S
): S | undefined
```

The type parameter `S` should still be inferrable from the callback's type predicate.

### 3. Type Argument Inference from Type Predicates

When inferring `S` from the callback `(x: any) => x is number`:
- The callback has type `(x: any) => x is number`
- The parameter type is `(value: string | number | boolean) => value is S`
- We need to infer `S = number` from the fact that the predicate narrows to `number`

**Current behavior**: We might be inferring `S = string | number | boolean` (from the parameter type) instead of `S = number` (from the type predicate).

## Next Steps for Implementation

### Step 1: Add Tracing

Add debug tracing to method call resolution:
```rust
#[tracing::instrument(level = "debug", skip(self))]
fn resolve_method_call(&mut self, obj_type: TypeId, method_name: &str) -> TypeId {
    debug!(obj_type_id = obj_type.0, method_name, "Resolving method call");
    // ...
}
```

### Step 2: Test with Minimal Case

Create minimal reproduction:
```typescript
const arr: (string | number)[] = [1, "x"];
const result = arr.find((x: string | number): x is number => typeof x === 'number');
// What type is result?
```

Run with tracing:
```bash
TSZ_LOG="tsz_checker=debug,tsz_solver::operations=debug" \
TSZ_LOG_FORMAT=tree \
cargo run --bin tsz -- test.ts 2>&1 | grep -A5 -B5 "find"
```

### Step 3: Locate the Bug

Look for where method signatures are instantiated. Likely places:
- `crates/tsz-checker/src/state_checking_members.rs`
- `crates/tsz-checker/src/type_computation.rs`
- `crates/tsz-solver/src/instantiate.rs`

### Step 4: Fix Type Parameter Inference

The fix probably involves:
1. Recognizing when a callback has a type predicate
2. Using the predicate's narrowed type for inference instead of the callback's return type
3. Ensuring this works correctly with constraints (`S extends T`)

## Additional Test Cases

### Test 1: ReadonlyArray
```typescript
const arr: ReadonlyArray<string | number> = [1, "x"];
const result: number | undefined = arr.find((x): x is number => typeof x === 'number');
```

### Test 2: Multiple Type Parameters
```typescript
declare function filter<T, S extends T>(
    arr: T[],
    predicate: (x: T) => x is S
): S[];

const arr: (string | number)[] = [1, "x", 2];
const result: number[] = filter(arr, (x): x is number => typeof x === 'number');
```

### Test 3: Method Chaining
```typescript
const result = [1, "x", 2]
    .filter((x): x is number => typeof x === 'number')
    .find((x) => x > 1);
// result should be number | undefined
```

## References

- TypeScript lib.d.ts: Array interface definition
- `crates/tsz-solver/src/operations.rs`: Call resolution and type inference
- `crates/tsz-checker/src/call_checker.rs`: Call checking logic
- `crates/tsz-solver/src/instantiate.rs`: Type instantiation

## Estimated Effort

**Medium-High**: 4-8 hours
- Requires deep understanding of type inference
- Need to trace through method resolution path
- Fix may touch multiple files
- Need comprehensive testing

**Impact**: ~10-20 failing tests
