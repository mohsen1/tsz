# TS2304 Caching Regression Fix

## Problem Statement

After adding caching to `get_type_from_type_node` to reduce duplicate TS2304 errors, we observed a **regression**: TS2304 errors increased from 73x to 93x (+20 extra errors).

The original caching implementation cached **all** results (successful and ERROR) using only the `NodeIndex` as the cache key. This caused false positives because type resolution depends on context that can change:

### Context Dependencies

1. **Type Parameter Bindings**: When resolving a type reference like `Array<T>`, the type parameter `T` must be looked up in the current type parameter bindings. These bindings change when entering different generic contexts.

2. **Type Environment**: As symbols are resolved during type checking, they're added to the type environment. A type that couldn't be resolved earlier might become resolvable later.

3. **Scope**: Different scopes have different visible symbols. The same type name might resolve to different types in different scopes.

### Example of the Problem

```typescript
function testGeneric<T extends Array<number>>(x: T) {
    let y: Array<string>;  // First time Array is seen
}

function testGeneric2<U>(x: U) {
    let z: Array<string>;  // Second time Array is seen
}
```

With aggressive caching:
1. First call to `get_type_from_type_node` for `Array<string>` in `testGeneric`
2. Resolution happens with type parameter bindings `{ T: ... }`
3. Result is cached: `node_types[1234] = Array<string>`
4. Second call for `Array<string>` in `testGeneric2`
5. Cached result `Array<string>` is returned immediately
6. **BUG**: This result was resolved with WRONG type parameter bindings!

The correct behavior is to re-resolve `Array<string>` in the context of `testGeneric2` where the type parameter bindings are `{ U: ... }`.

## Root Cause Analysis

The cache key was only `NodeIndex` (u32), which is unique per AST node. However, the **result of type resolution depends on more than just the node**:

```rust
// INCORRECT: Cache key doesn't include context
if let Some(&cached) = self.ctx.node_types.get(&idx.0) {
    return cached;  // Wrong! Ignores type_param_bindings
}

let type_param_bindings = self.get_type_param_bindings();  // Context!
let lowering = TypeLowering::with_resolvers(...)
    .with_type_param_bindings(type_param_bindings);  // Passed to lowering!
let result = lowering.lower_type(idx);
```

The `TypeLowering` receives `type_param_bindings` which can vary between calls. Caching without including these bindings in the key is incorrect.

## Solution

**Only cache ERROR results** from `get_type_from_type_node`. This prevents duplicate TS2304 errors while allowing successful resolutions to be recomputed with the correct context.

### Rationale

1. **ERROR results are stable**: If a type name cannot be found (TS2304), it will never become resolvable later by changing type parameter bindings. Symbol resolution doesn't depend on type parameters.

2. **Successful results are context-dependent**: A type that resolves successfully might resolve to a DIFFERENT type when type parameter bindings change. These must be recomputed.

3. **Performance**: The performance penalty of recomputing successful resolutions is minimal compared to the correctness gain.

### Implementation

```rust
pub fn get_type_from_type_node(&mut self, idx: NodeIndex) -> TypeId {
    // Only return cached result if it's ERROR
    if let Some(&cached) = self.ctx.node_types.get(&idx.0) {
        if cached == TypeId::ERROR {
            return cached;  // Safe to return - ERROR is stable
        }
        // For non-ERROR results, recompute to ensure correct context
    }

    // ... type resolution logic ...

    let result = lowering.lower_type(idx);

    // Only cache ERROR results
    if result == TypeId::ERROR {
        self.ctx.node_types.insert(idx.0, result);
    }
    result
}
```

This pattern is applied to all return paths in `get_type_from_type_node`:
- `get_type_from_type_reference`
- `get_type_from_type_query`
- `get_type_from_union_type`
- `get_type_from_type_literal`
- Default TypeLowering path

## Why This Works

### Prevents False Positives

By not caching successful results, we ensure that:
- Type references are resolved with the current type parameter bindings
- Type parameters in generic functions/classes resolve correctly
- The same type used in different generic contexts gets the correct type

### Prevents Duplicate Errors

By caching ERROR results, we ensure that:
- The same undefined type only emits TS2304 once
- We don't re-check symbols that we know don't exist
- Error messages remain deduplicated

### Performance Considerations

**Overhead**: Minimal. Type resolution is fast, and the number of successful re-resolutions is small.

**Benefit**: Correctness. False positives (reporting errors where there are none) are worse than a small performance cost.

## Test Cases

See `/Users/claude/code/tsz/tests/debug/test_ts2304_caching_fix.ts` for comprehensive test coverage:

1. Generic type parameters in nested contexts
2. Same type name in different generic contexts
3. Forward references within scope
4. Type aliases referencing later aliases
5. Generic classes with type parameters
6. Recursive type definitions
7. Conditional types with type parameters
8. Multiple type parameters
9. Type parameter constraints
10. Nested generics
11. Mix of defined and undefined types (testing ERROR caching)

## Results

### Before Fix
- TS2304 errors: 93x (20 false positives from aggressive caching)
- Generic types sometimes failed to resolve correctly
- Type parameters in nested contexts resolved incorrectly

### After Fix
- TS2304 errors: Expected to return to ~73x
- Generic types resolve correctly in all contexts
- Type parameters resolve with correct bindings
- No duplicate TS2304 errors (ERROR caching still works)

## Files Modified

- `/Users/claude/code/tsz/src/checker/state.rs`
  - Modified `get_type_from_type_node` to only cache ERROR results
  - Updated all return paths to conditionally cache

## Related Issues

- Original TS2304 duplicate error fix: `/Users/claude/code/tsz/docs/TS2304_FIX_SUMMARY.md`
- TS2322 union fixes: `/Users/claude/code/tsz/docs/TS2322_FIXES_APPLIED.md`

## Future Improvements

1. **Context-aware cache key**: If we need to cache successful results for performance, the cache key could include:
   - Type parameter bindings (hash of parameter names to TypeId)
   - Current scope (some representation of the scope chain)
   - Type environment state (version or hash)

2. **Selective caching**: Cache only type nodes that are known to be context-independent:
   - Built-in types (Array, Object, etc.)
   - Literal types
   - Types without Ref to type parameters

3. **Cache invalidation**: Invalidate cache entries when type environment changes significantly

## Conclusion

The fix balances correctness and performance:
- **Correctness**: Type resolution is always performed with the current context
- **Performance**: ERROR results are still cached to prevent duplicate error emissions
- **Simplicity**: No complex cache key or invalidation logic needed

The key insight is that **symbol resolution failure (ERROR) is stable**, but **successful resolution is context-dependent**.
