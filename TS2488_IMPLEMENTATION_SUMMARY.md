# TS2488 Iterator Protocol Error Implementation

## Summary

Fixed and verified TS2488 "Type must have Symbol.iterator" error emission for non-iterable types in array destructuring, spread operations, and for-of loops.

## Changes Made

### 1. Fixed Array Destructuring Check (src/checker/type_checking.rs)

**Modified `check_array_destructuring_target_type` function (lines ~1336-1379)**

**Before:**
- Used `is_array_destructurable_type` which didn't properly check for Symbol.iterator
- Emitted TS2461 (TYPE_IS_NOT_AN_ARRAY_TYPE)

**After:**
- Uses `is_iterable_type` which properly checks for Symbol.iterator in objects
- Emits TS2488 (TYPE_MUST_HAVE_SYMBOL_ITERATOR)

### 2. Existing Spread Iterability Checks (Already Implemented)

**Array Literal Spreads** (src/checker/type_computation.rs, line ~123)
- Checks `check_spread_iterability` for each spread element in array literals
- Emits TS2488 when spread operand is not iterable

**Function Call Spreads** (src/checker/state.rs, line ~5844)
- Checks `check_spread_iterability` for each spread argument in function calls
- Emits TS2488 when spread argument is not iterable

### 3. Existing for-of Loop Check (Already Implemented)

**for-of Loops** (src/checker/state.rs, line ~10657)
- Calls `check_for_of_iterability` which uses `is_iterable_type`
- Emits TS2488 for regular for-of
- Emits TS2504 for for-await-of when not async iterable

## Iterable Type Detection

The `is_iterable_type` function (src/checker/iterable_checker.rs) correctly identifies:

**Iterable Types:**
- `any` / `unknown` / `error` (permissive, no error)
- `string` type and string literals
- `Array<T>` types
- `Tuple<T1, T2, ...>` types
- Objects with `[Symbol.iterator]()` method
- Objects with `next()` method (iterator protocol)
- `ReadonlyArray<T>` (unwrapped before checking)
- **Union types where ALL members are iterable**

**Non-Iterable Types:**
- `number`, `boolean`, `void`, `null`, `undefined`, `never`
- Plain objects without iterator
- Class instances without iterator
- Functions
- **Union types with ANY non-iterable member**

## Test Files Created

1. **test_ts2488_array_destructuring.ts** - Basic array destructuring errors
2. **test_ts2488_spread.ts** - Spread operations in arrays and function calls
3. **test_ts2488_for_of.ts** - for-of and for-await-of loop errors
4. **test_ts2488_union_types.ts** - Union type iterability requirements
5. **test_ts2488_edge_cases.ts** - Edge cases and valid scenarios

## Error Code

- **TS2488**: `TYPE_MUST_HAVE_SYMBOL_ITERATOR` (error code 2488)
- Message: "Type '{0}' must have a Symbol.iterator method that returns an iterator."

## Example Errors

### Array Destructuring
```typescript
const [a, b] = 42;
// TS2488: Type 'number' must have a Symbol.iterator method that returns an iterator.
```

### Spread in Array Literal
```typescript
const arr = [...{}];
// TS2488: Type '{}' must have a Symbol.iterator method that returns an iterator.
```

### Spread in Function Call
```typescript
function foo(...args: number[]) {}
foo(true);
// TS2488: Type 'boolean' must have a Symbol.iterator method that returns an iterator.
```

### for-of Loop
```typescript
for (const x of null) {
    console.log(x);
}
// TS2488: Type 'null' must have a Symbol.iterator method that returns an iterator.
```

### Union Type with Non-Iterable Member
```typescript
type MaybeArray = number[] | number;
const val: MaybeArray = 42;
const [a] = val;
// TS2488: Type 'number' must have a Symbol.iterator method that returns an iterator.
```

## Valid Cases (No Error)

```typescript
// Arrays are iterable
const [a] = [1, 2, 3];

// Strings are iterable
const [b] = "hello";

// Tuples are iterable
const [c, d] = [1, "x"] as [number, string];

// Objects with Symbol.iterator
const iterable = {
    [Symbol.iterator]: function* () { yield 1; }
};
const [e] = iterable;

// any/unknown are permissive
const [f] = {} as any;

// Readonly arrays are iterable
const [g] = [1, 2, 3] as const;
```

## Behavior Differences from TSC

The implementation follows TSC behavior closely:
- Union types require ALL members to be iterable (consistent with TSC)
- `any` and `unknown` are permissive (no error)
- `null`, `undefined`, `number`, `boolean`, `void` are not iterable
- `never` is not iterable but may have special handling in some contexts

## Commits

1. `ca0ab4f6a` - Fix TS2488 emission for array destructuring
2. `f48b41d21` - Add comprehensive test files for TS2488 errors
3. `bda7a264e` - Add additional TS2488 test files (unions, edge cases)

All changes pushed to `worker-6` branch.
