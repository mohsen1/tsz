# Symbol/DecoratorMetadata Bug Fix - Session Summary (2026-02-12)

## Fix Applied

**File:** `crates/tsz-solver/src/lower.rs`
**Function:** `lower_identifier_type`

**Problem:** Primitive type keywords like "symbol", "string", "number" were being resolved as symbols BEFORE checking if they were built-in types. This caused type annotations like `: symbol` to resolve incorrectly when certain lib files were loaded.

**Solution:** Reordered checks to verify built-in primitive types FIRST, before attempting any symbol resolution. This ensures primitive keywords always resolve correctly and can never be shadowed.

## Test Results

### Before Fix
- **Pass Rate:** 68.3% (2,145/3,139 tests)
- **Key Issue:** `Symbol('test')` returned `DecoratorMetadata` instead of `symbol`

### After Fix  
- **Pass Rate:** 68.4% (2,147/3,139 tests)
- **Improvement:** +2 tests (+0.1%)
- **Key Fix:** `Symbol('test')` now correctly returns `symbol`

## Why Small Improvement?

The Symbol() bug is FIXED, but many test failures remain due to OTHER bugs:

### WeakKey Type Definition Issue
Tests like `acceptSymbolAsWeakType` still fail because:
- `WeakKey = WeakKeyTypes[keyof WeakKeyTypes]`
- `WeakKeyTypes` only has `{object: object}` in es5.d.ts
- `symbol: symbol` should be added by es2023.collection.d.ts
- But esnext loads es2024.collection which doesn't include this

**Example Error:**
```
error TS2345: Argument of type 'symbol' is not assignable to parameter of type 'WeakKey'.
```

TSC accepts symbol in WeakSet/WeakMap, but tsz doesn't because our WeakKey union is incomplete.

## Remaining Error Distribution

After fix:
- TS2345: extra=120 (down from 122, -2)
- TS2322: extra=106 (down from 107, -1)
- TS2339: extra=95 (unchanged)

Many of these are now due to:
1. WeakKey not including symbol
2. Other type definition issues in lib files
3. Unrelated type system bugs

## Impact Assessment

### Direct Impact (Symbol Bug)
- **Fixed:** Primitive type resolution in type annotations
- **Verified:** All 3,547 tsz-solver unit tests pass
- **Verified:** Symbol('test') returns correct type

### Indirect Impact (Other Bugs Revealed)
- **Identified:** WeakKey type definition incompleteness
- **Identified:** Lib file interface merging issues
- **Identified:** Need to audit other primitive type usages

## Next Steps

### High Priority
1. **Fix WeakKey type** - Add symbol to WeakKeyTypes for esnext
2. **Audit lib files** - Verify all TypeScript lib file versions match
3. **Interface merging** - Review how lib interfaces are merged across files

### Medium Priority  
4. **Implement missing error codes** - TS2792 (15 tests), TS2538 (9 tests)
5. **Fix "close to passing" tests** - 244 tests needing 1-2 error codes

## Lessons Learned

1. **Root cause ≠ Only cause** - Fixing the Symbol() bug revealed it was masking other issues
2. **Type system bugs cascade** - One wrong type (DecoratorMetadata) caused many downstream errors  
3. **Lib file dependencies** - Interface merging across lib files is complex and error-prone
4. **Test carefully** - Even with 3,547 passing unit tests, integration issues can hide bugs

## Files Changed
- `crates/tsz-solver/src/lower.rs` - Reordered primitive type checks

## Verification
- ✅ All tsz-solver unit tests pass (3,547/3,547)
- ✅ Symbol('test') returns symbol with all lib combinations
- ✅ Pre-commit checks pass (2,396 tests)
- ✅ Code committed and synced to main
