# TS2769 Overload Resolution Issue - 2026-02-13

## Problem

TSZ reports TS2769 "No overload matches this call" errors in cases where TSC accepts the code, particularly with complex generic types and Array methods.

**Impact**: 6 false positive errors (too strict)
**Priority**: Medium (UX issue - false positives)

## Example Case: arrayConcat3.ts

```typescript
// @strictFunctionTypes: true
type Fn<T extends object> = <U extends T>(subj: U) => U

function doStuff<T extends object, T1 extends T>(
    a: Array<Fn<T>>,
    b: Array<Fn<T1>>
) {
    b.concat(a);  // TSZ ERROR TS2769, TSC accepts
}
```

**TSZ Error**:
```
TS2769: No overload matches this call.
  Argument of type 'Fn<T>[]' is not assignable to parameter of type 'Node<Fn<T1>>'
  Argument of type 'Fn<T>[]' is not assignable to parameter of type 'Fn<T1> | Node<Fn<T1>>'
```

**TSC**: No error

## Analysis

### Type Relationships

1. **Type Parameters**: `T extends object`, `T1 extends T`
   - `T1` is a subtype of `T`

2. **Function Type**: `Fn<T> = <U extends T>(subj: U) => U`
   - This is a generic function type (polymorphic)

3. **Arrays**: `Array<Fn<T>>` and `Array<Fn<T1>>`

4. **Method Call**: `b.concat(a)`
   - `b` is `Array<Fn<T1>>`
   - `a` is `Array<Fn<T>>`

### Why This Should Work

With `strictFunctionTypes`, generic functions are treated specially:
- A generic function `<U extends T>(u: U) => U` is covariant in `T`
- Since `T1 extends T`, we have `Fn<T1>` is a subtype of `Fn<T>`
- In arrays: `Array<Fn<T1>>` can accept `Array<Fn<T>>` via `concat`

### Array.concat() Signature

```typescript
interface Array<T> {
    concat(...items: ConcatArray<T>[]): T[];
    concat(...items: (T | ConcatArray<T>)[]): T[];
}
```

The second overload should match: `Array<Fn<T>>` matches `(Fn<T1> | ConcatArray<Fn<T1>>)[]`

## Root Cause Hypothesis

TSZ's overload resolution may be:

1. **Too strict on variance checks** in generic contexts
2. **Not properly handling generic function types** in strict mode
3. **Checking overloads in wrong order** or with incorrect priority
4. **Failing to recognize covariance** of higher-order function types

## Investigation Needed

**Files to Check**:
- `crates/tsz-checker/src/call_checker.rs` - Overload resolution
- `crates/tsz-solver/src/subtype.rs` - Subtyping with strict function types
- `crates/tsz-solver/src/application.rs` - Generic type instantiation

**Key Questions**:
1. How does TSZ handle generic function types in variance checks?
2. Is `strictFunctionTypes` mode correctly implemented for polymorphic functions?
3. Does overload resolution consider all viable candidates before reporting errors?

## Related Issues

Similar patterns likely affect other TS2769 false positives (6 total). Once the root cause is identified, the fix may resolve multiple cases.

## Workaround

Users can:
1. Add explicit type assertions
2. Use intermediate variables with explicit types
3. Disable `strictFunctionTypes` (not recommended)

## Next Steps

1. **Add tracing** to overload resolution in `call_checker.rs`
2. **Compare** with TSC behavior using minimal test case
3. **Check** variance handling for generic function types
4. **Review** strictFunctionTypes implementation for higher-order types

## Testing Strategy

Create minimal test cases:
```typescript
// Test 1: Basic generic function subtyping
type Fn1<T> = <U extends T>(u: U) => U;
let f1: Fn1<object>;
let f2: Fn1<{x: number}>;
f1 = f2; // Should work

// Test 2: In arrays
let a1: Array<Fn1<object>>;
let a2: Array<Fn1<{x: number}>>;
a2.concat(a1); // Should work

// Test 3: With constraints
function test<T extends object, T1 extends T>(
    a: Array<Fn1<T>>,
    b: Array<Fn1<T1>>
) {
    b.concat(a); // Should work
}
```

## References

- TypeScript issue #20454 (mentioned in test comment)
- Strict function types documentation
- Variance in generic function types
