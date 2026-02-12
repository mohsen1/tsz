# Array Method Return Type Bug

## Severity: HIGH
**Impact**: ~80+ tests (arrayconcat, arrayFind, and many array operation tests)

## Problem

Array methods like `.sort()`, `.find()`, `.filter()`, etc. are returning malformed types that include the entire Array interface structure instead of the correct simplified type.

## Reproduction

```typescript
const arr: number[] = [3, 1, 2];
const sorted = arr.sort();
const x: number[] = sorted; // ❌ Error: massive type mismatch
```

### Expected
- `sorted` should have type `number[]`
- No error on assignment

### Actual
- `sorted` has type `{ (index: number, value: T): T[]; [Symbol.iterator]: { (): Blob<T> }; entries: { (): Blob<[..., ...]> }; ... }` (massive object type)
- TS2322 error on assignment

## Full Error Message

```
Type '{ (index: number, value: T): T[]; [Symbol.iterator]: { (): Blob<T> };
entries: { (): Blob<[..., ...]> }; keys: { (): Blob<number> }; values: { (): Blob<T> };
length: number; toString: { (): string }; toLocaleString: { (): string; ... };
pop: { (): undefined | T }; push: { (items: T[]): number };
concat: { (items: BlobEvent<...>[]): T[]; ... }; join: { (separator: string): string };
reverse: { (): T[] }; shift: { (): undefined | T };
slice: { (start: number, end: number): T[] };
sort: { (compareFn: (a: T, b: T) => number): number[] };
... (and 30+ more method signatures) ...
}<number>' is not assignable to type 'number[]'.
```

## Analysis

The returned type shows:
1. **Callable signature**: `(index: number, value: T): T[]` - array indexer
2. **Symbol properties**: `[Symbol.iterator]`, `[Symbol.unscopables]`
3. **All array methods**: `sort`, `map`, `filter`, etc. as nested objects
4. **Generic parameter**: Still has `<number>` at the end

This looks like:
- The Array interface is being expanded into an object type literal
- Instead of returning `T[]`, we're returning the full interface structure
- The type is not being simplified/normalized

## Hypothesis

When resolving method return types on generic instances like `Array<number>`:
1. We correctly instantiate the Array interface with `T = number`
2. But instead of returning the array type, we're returning the interface's object representation
3. Thismight be happening during:
   - Method call resolution
   - Return type substitution
   - Type simplification/normalization

## Potential Root Causes

### 1. Method `this` Type Handling
Array methods like `sort()` have signature:
```typescript
interface Array<T> {
    sort(compareFn?: (a: T, b: T) => number): this;
}
```

The return type is `this` (the array itself). We might be:
- Not properly handling `this` type in method returns
- Expanding `this` to the full interface structure instead of the array type

### 2. Type Normalization Missing
After method call resolution, we should normalize/simplify the return type:
- `Array<T>` interface → `T[]` type
- Interface object representation → Array type

We might be skipping this step or not handling arrays specially.

### 3. Type Instantiation Issue
When instantiating `Array<number>`:
- Should produce `number[]` type
- Might be producing expanded object type with all interface members

## Files to Investigate

### 1. Method Call Resolution
**`crates/tsz-checker/src/state_checking_members.rs`**
- How do we resolve methods on generic types?
- How is the return type computed?

### 2. This Type Handling
**`crates/tsz-solver/src/operations.rs`**
- Search for "this" type handling in method calls
- Check `resolve_function_call` and related functions

### 3. Type Simplification
**`crates/tsz-solver/src/canonicalize.rs` or similar**
- Do we have type simplification/normalization?
- Should `Array<T>` interface be simplified to `T[]`?

### 4. Array Type Representation
**`crates/tsz-solver/src/types.rs`**
- How are array types represented?
- Is there a distinction between `Array<T>` and `T[]`?

## Debugging Steps

### 1. Add Tracing
```rust
#[tracing::instrument(level = "debug", skip(self))]
fn resolve_method_call_return_type(&mut self, method_type: TypeId) -> TypeId {
    debug!(method_type_id = method_type.0, "Computing method return type");
    let result = /* ... */;
    debug!(return_type_id = result.0, return_type = ?self.lookup(result), "Method return type");
    result
}
```

### 2. Test with Tracing
```bash
TSZ_LOG="tsz_checker=debug,tsz_solver=debug" TSZ_LOG_FORMAT=tree \
cargo run --bin tsz -- tmp/test_sort_simple.ts 2>&1 | grep -A10 "sort"
```

### 3. Compare with Function Call
Test explicit function vs method:
```typescript
declare function sort<T>(arr: T[]): T[];
const arr1: number[] = [1, 2, 3];
const sorted1: number[] = sort(arr1); // Does this work?

const arr2: number[] = [1, 2, 3];
const sorted2: number[] = arr2.sort(); // This fails
```

If the first works but the second doesn't, it confirms the issue is in method resolution.

## Fix Strategy

### Option 1: Fix `this` Type Handling
If the issue is `this` type:
1. Detect when method return type is `this`
2. Substitute with the receiver's type (`number[]` not the expanded interface)
3. Ensure proper type simplification

### Option 2: Add Type Normalization Pass
After method call resolution:
1. Check if result type is `Array<T>` interface
2. Convert to `T[]` array type
3. Apply recursively to nested types

### Option 3: Fix Array Type Representation
Ensure `Array<T>` is always represented as `T[]`:
1. When instantiating Array interface, produce array type not object type
2. Maintain distinction between interface and its instances
3. Use array type consistently in all contexts

## Test Cases

```typescript
// Basic sort
const arr1: number[] = [3, 1, 2];
const sorted1: number[] = arr1.sort(); // Should work

// With callback
const arr2: number[] = [3, 1, 2];
const sorted2: number[] = arr2.sort((a, b) => a - b); // Should work

// On object property
interface Options { name: string }
class Parser {
    options: Options[];
    sort() {
        this.options = this.options.sort((a, b) =>
            a.name.localeCompare(b.name)
        ); // Should work
    }
}

// Other methods
const arr3: number[] = [1, 2, 3];
const filtered: number[] = arr3.filter(x => x > 1); // Should work
const mapped: string[] = arr3.map(x => x.toString()); // Should work
const found: number | undefined = arr3.find(x => x > 1); // Should work
```

## Affected Tests (Estimated)

Based on conformance analysis:
- All tests with `array` in name using methods: ~30 tests
- Tests with TS2322 false positives on array operations: ~50 tests
- Total estimated impact: **80-100 tests**

This is one of the highest-impact bugs in the conformance suite!

## Estimated Fix Effort

**Medium-High**: 4-6 hours
- Need to understand type representation system
- Likely requires changes in multiple files
- Need comprehensive testing to avoid regressions
- High impact means careful validation needed

## Priority

**HIGHEST** - This single fix could improve pass rate by 2-3%!
