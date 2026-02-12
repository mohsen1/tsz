# WeakKey Type Resolution Bug

## Problem

`WeakKey` type is not resolving correctly, causing both `object` and `symbol` to be incompatible with it.

## Reproduction

```typescript
// With --lib esnext --target esnext
const test1: WeakKey = {} as object;   // ERROR: Type 'object' is not assignable to type 'WeakKey'
const test2: WeakKey = Symbol() as symbol;  // ERROR: Type 'symbol' is not assignable to type 'WeakKey'
```

## Expected Behavior

Per TypeScript conformance test `acceptSymbolAsWeakType.ts`:
- Symbol values should be accepted by WeakSet, WeakMap, WeakRef, FinalizationRegistry
- No errors should occur

## Type Definition Chain

### es5.d.ts
```typescript
interface WeakKeyTypes {
    object: object;
}
type WeakKey = WeakKeyTypes[keyof WeakKeyTypes];
```

### es2023.collection.d.ts  
```typescript
interface WeakKeyTypes {
    symbol: symbol;
}
```

### Reference Chain (esnext)
- esnext.d.ts → es2024.d.ts → es2023.d.ts → es2023.collection.d.ts
- This should merge both `object` and `symbol` into WeakKeyTypes
- `WeakKey` should resolve to `object | symbol`

## Current Behavior

- `WeakKey` is resolving to something that rejects BOTH `object` AND `symbol`
- This suggests the problem is NOT just missing interface merging
- The indexed access type `WeakKeyTypes[keyof WeakKeyTypes]` may not be resolving correctly

## Investigation Needed

1. **Check interface merging**: Are both WeakKeyTypes declarations being merged?
2. **Check indexed access resolution**: Is `WeakKeyTypes[keyof WeakKeyTypes]` being evaluated correctly?
3. **Check lib loading order**: Are files being loaded in the correct order?
4. **Check type alias expansion**: Is the `type WeakKey = ...` being expanded correctly?

## Related Issues

- Affects ~50+ conformance tests
- Tests: acceptSymbolAsWeakType, and all tests using WeakSet/WeakMap/WeakRef with symbols
- Error codes: TS2345, TS2769

## Files to Investigate

- `crates/tsz-solver/src/lower.rs` - Type lowering for indexed access types
- `crates/tsz-binder/src/state.rs` - Interface merging logic
- `crates/tsz-checker/src/type_checking_queries.rs` - Type resolution

## Priority

HIGH - Affects many tests, but deeper issue than initially thought. Not a simple lib file fix.

## Status

BLOCKED - Requires investigation of indexed access type resolution or interface merging.
Not just adding symbol member to WeakKeyTypes.
