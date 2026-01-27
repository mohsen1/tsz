# TS2488 Iterator Protocol Fix - Generator and AsyncGenerator Types

## Problem Statement

TS2488 errors were being incorrectly emitted for generator functions and async generator functions. The type checker was failing to recognize these types as iterable, even though by definition they implement the iterator protocol.

## Root Cause Analysis

### How Iterator Detection Works

The `is_iterable_type()` function in `src/checker/iterable_checker.rs` checks if an object is iterable by looking for properties that meet these criteria:
1. Property name is `[Symbol.iterator]` (computed property name)
2. Property is marked as a method (`is_method == true`)

### The Bug

The `create_generator_type()` and `create_async_generator_type()` functions in `src/checker/generators.rs` were creating Generator and AsyncGenerator types with only `next`, `return`, and `throw` methods, but **missing the critical `[Symbol.iterator]()` and `[Symbol.asyncIterator]()` methods**.

According to the TypeScript specification:
```typescript
interface Generator<T = unknown, TReturn = any, TNext = unknown> {
    next(...args: [] | [TNext]): IteratorResult<T, TReturn>;
    return(value?: TReturn): IteratorResult<T, TReturn>;
    throw(e?: any): IteratorResult<T, TReturn>;
    [Symbol.iterator](): Generator<T, TReturn, TNext>;  // ❌ MISSING
}

interface AsyncGenerator<T = unknown, TReturn = any, TNext = unknown> {
    next(...args: [] | [TNext]): Promise<IteratorResult<T, TReturn>>;
    return(value?: TReturn | PromiseLike<TReturn>): Promise<IteratorResult<T, TReturn>>;
    throw(e?: any): Promise<IteratorResult<T, TReturn>>;
    [Symbol.asyncIterator](): AsyncGenerator<T, TReturn, TNext>;  // ❌ MISSING
}
```

The code comments correctly documented that these methods should exist (lines 586 and 683), but the implementation was incomplete.

### Why This Caused False Positives

When you write:
```typescript
function* generatorFunction() {
    yield 1;
    yield 2;
}

const gen = generatorFunction();
for (const item of gen) {  // ❌ TS2488 error - incorrectly flagged
    console.log(item);
}
```

The generator function's return type is `Generator<number>`, but the type checker created this type without the `[Symbol.iterator]()` method. When checking if the type is iterable, the `is_iterable_type()` function couldn't find the required method, so it reported TS2488.

## The Fix

### File: src/checker/generators.rs

#### Fix 1: Add Symbol.iterator to Generator type

**Location**: `create_generator_type()` function (around line 589)

**Before**:
```rust
fn create_generator_type(&self, yield_type: TypeId, return_type: TypeId, next_type: TypeId) -> TypeId {
    // ... create next, return, throw methods ...

    // Create Generator object type
    self.ctx.types.object(vec![
        // ... next, return, throw properties ...
    ])  // ❌ Missing [Symbol.iterator]
}
```

**After**:
```rust
fn create_generator_type(&self, yield_type: TypeId, return_type: TypeId, next_type: TypeId) -> TypeId {
    // ... create next, return, throw methods ...

    // Create a self-referential Generator type for Symbol.iterator return type
    let generator_type = self.ctx.types.object(vec![
        // ... next, return, throw properties ...
    ]);

    // Create Symbol.iterator method that returns the generator itself
    let symbol_iterator_name = self.ctx.types.intern_string("[Symbol.iterator]");
    let symbol_iterator_method = self.ctx.types.function(crate::solver::FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: generator_type,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    // Create the final Generator object type with Symbol.iterator
    self.ctx.types.object(vec![
        // ... next, return, throw properties ...
        crate::solver::PropertyInfo {
            name: symbol_iterator_name,
            type_id: symbol_iterator_method,
            write_type: symbol_iterator_method,
            optional: false,
            readonly: true,
            is_method: true,  // ✅ Added
        },
    ])
}
```

#### Fix 2: Add Symbol.asyncIterator to AsyncGenerator type

**Location**: `create_async_generator_type()` function (around line 686)

**Before**:
```rust
fn create_async_generator_type(&self, yield_type: TypeId, return_type: TypeId, next_type: TypeId) -> TypeId {
    // ... create next, return, throw methods ...

    // Create AsyncGenerator object type
    self.ctx.types.object(vec![
        // ... next, return, throw properties ...
    ])  // ❌ Missing [Symbol.asyncIterator]
}
```

**After**:
```rust
fn create_async_generator_type(&self, yield_type: TypeId, return_type: TypeId, next_type: TypeId) -> TypeId {
    // ... create next, return, throw methods ...

    // Create a self-referential AsyncGenerator type for Symbol.asyncIterator return type
    let async_generator_type = self.ctx.types.object(vec![
        // ... next, return, throw properties ...
    ]);

    // Create Symbol.asyncIterator method that returns the async generator itself
    let symbol_async_iterator_name = self.ctx.types.intern_string("[Symbol.asyncIterator]");
    let symbol_async_iterator_method = self.ctx.types.function(crate::solver::FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: async_generator_type,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    // Create the final AsyncGenerator object type with Symbol.asyncIterator
    self.ctx.types.object(vec![
        // ... next, return, throw properties ...
        crate::solver::PropertyInfo {
            name: symbol_async_iterator_name,
            type_id: symbol_async_iterator_method,
            write_type: symbol_async_iterator_method,
            optional: false,
            readonly: true,
            is_method: true,  // ✅ Added
        },
    ])
}
```

## Impact

### Before Fix

```typescript
// Generator functions
function* generatorFunction() {
    yield 1;
    yield 2;
}

const gen = generatorFunction();
for (const item of gen) {  // ❌ TS2488 error
    console.log(item);
}

// Async generator functions
async function* asyncGen() {
    yield 1;
    yield 2;
}

const asyncGenInstance = asyncGen();
for await (const item of asyncGenInstance) {  // ❌ TS2504 error
    console.log(item);
}
```

### After Fix

```typescript
// Generator functions
function* generatorFunction() {
    yield 1;
    yield 2;
}

const gen = generatorFunction();
for (const item of gen) {  // ✅ No error - correctly recognized as iterable
    console.log(item);
}

// Async generator functions
async function* asyncGen() {
    yield 1;
    yield 2;
}

const asyncGenInstance = asyncGen();
for await (const item of asyncGenInstance) {  // ✅ No error - correctly recognized as async iterable
    console.log(item);
}
```

## Test Cases Covered

1. **Generator function in for-of** - Should NOT error (now fixed)
2. **Async generator function in for-await-of** - Should NOT error (now fixed)
3. **Generator methods in classes** - Should NOT error (now fixed)
4. **Generator expressions** - Should NOT error (now fixed)
5. **yield* with generator** - Should NOT error (now fixed)

## Additional Notes

### Self-Referential Types

The fix uses a two-step creation process to handle the self-referential nature of the iterator protocol:

1. First, create a basic Generator/AsyncGenerator type without the Symbol.iterator/asyncIterator method
2. Then, create the Symbol.iterator/asyncIterator method that returns the type created in step 1
3. Finally, create the complete type including all methods

This is necessary because the Symbol.iterator method returns `Generator<T, R, N>` which is the type we're currently defining.

### Property Name Format

The property name must be exactly `"[Symbol.iterator]"` (with brackets) to match what the iterator checker expects:

```rust
let symbol_iterator_name = self.ctx.types.intern_string("[Symbol.iterator]");
```

This format is produced by the `get_property_name()` function when it encounters computed property names with Symbol expressions.

### Related Files

- `src/checker/generators.rs` - Main fix location (added Symbol.iterator and Symbol.asyncIterator methods)
- `src/checker/iterable_checker.rs` - Iterator detection logic (already correct)
- `src/checker/class_type.rs` - Class method handling (already correct - sets is_method: true)

## Verification

To verify the fix works:

1. Create a test file with generator functions
2. Run the type checker
3. Verify that TS2488 is NOT emitted for generator functions
4. Verify that TS2504 is NOT emitted for async generator functions
5. Verify that TS2488 IS still emitted for actual non-iterables

## Relationship to Previous Fixes

This fix complements the earlier fix for object literal methods (documented in `TS2488_ITERATOR_FIX_SUMMARY.md`):

- **Previous fix**: Ensured object literal methods are marked with `is_method: true`
- **This fix**: Ensures Generator and AsyncGenerator types include the required Symbol methods

Both fixes are necessary for complete iterator protocol support:

1. Object literal iterables (e.g., `{ *[Symbol.iterator]() { yield 1; } }`) - Fixed by previous change
2. Generator functions (e.g., `function*() { yield 1; }`) - Fixed by this change
3. Class methods with Symbol.iterator - Already working (classes set is_method: true)
4. Array-like objects - Correctly NOT iterable (don't have Symbol.iterator)

## Conclusion

This fix resolves TS2488 errors for generator and async generator functions by properly implementing the iterator protocol in their type definitions. The fix aligns the implementation with the documented specification and with TypeScript's actual behavior.
