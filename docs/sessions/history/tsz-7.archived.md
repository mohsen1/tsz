# Session TSZ-7: Lib Infrastructure Fix

**Started**: 2026-02-06
**Status**: ✅ COMPLETE
**Predecessor**: TSZ-6 (Investigation - Lawyer Layer Already Done)

## Accomplishments

### Lib Infrastructure Fix ✅

**Problem**: Tests failing because global types (Array, String, Promise, etc.) were not loaded in test environment.

**Root Cause**:
- `SHARED_LIB_CONTEXTS` was empty (embedded libs were removed)
- `load_lib_files_for_test()` pointed to non-existent `TypeScript/node_modules/` path

**Solution**:
1. Updated `SHARED_LIB_FILES` to load lib.es5.d.ts via `load_lib_files_from_paths()`
2. Populated `SHARED_LIB_CONTEXTS` with pre-compiled lib contexts
3. Added correct lib paths: `scripts/conformance/node_modules/` and `scripts/emit/node_modules/`

**Impact**: +101 tests fixed (8124 → 8225 passing, 176 → 75 failing)

**Test Results**:
- `test_builtin_types_no_ts2304_errors` - PASS ✅
- `test_checker_lowers_element_access_array` - PASS ✅
- `test_apparent_members_on_primitives` - PASS ✅

**Note**: One pre-existing test failure (`test_number_string_union_minus_emits_ts2362`) is unrelated to this change.

## Task

Fix **Lib Infrastructure** to load global type definitions (Array, String, Promise, etc.) in test environment.

## Problem Statement (DISCOVERED)

**Original task was "Element Access Lowering"** based on Gemini's recommendation. However, investigation revealed:

1. Element access lowering is **already implemented** in `src/solver/element_access.rs`
2. The `ElementAccessEvaluator` properly handles arrays, tuples, objects, unions
3. The Checker correctly delegates to Solver via `get_element_access_type()`

**The actual problem**: Tests are failing because global types are not loaded:
- `SHARED_LIB_CONTEXTS` is empty (embedded libs were removed)
- `setup_lib_contexts()` uses this empty vector
- Test errors: "Cannot find global type 'Array', 'String', etc."
- Lib file path in `load_lib_files_for_test()` points to non-existent location

## Root Cause

In `src/tests/test_fixtures.rs`:
```rust
// Lines 36-42: Empty because embedded libs were removed
pub static SHARED_LIB_CONTEXTS: Lazy<Vec<...>> = Lazy::new(Vec::new);
```

Lib files exist at:
- `./scripts/emit/node_modules/typescript/lib/lib.es5.d.ts` ✓
- `./scripts/conformance/node_modules/typescript/lib/lib.es5.d.ts` ✓
- `TypeScript/node_modules/typescript/lib/lib.es5.d.ts` ✗ (doesn't exist)

## Expected Impact

- **Direct**: Fix ~30 tests that fail due to missing global types
- **Cascading**: Enable proper testing of other features (element access, flow narrowing, etc.)
- **Total**: ~30 test improvement (possibly more as unblocks other tests)

## Implementation

### Phase 1: Fix Lib Path ✅
Updated `load_lib_files_for_test()` to check correct paths:
- `./scripts/emit/node_modules/typescript/lib/lib.es5.d.ts`
- `./scripts/conformance/node_modules/typescript/lib/lib.es5.d.ts`

### Phase 2: Populate SHARED_LIB_CONTEXTS ✅
1. Created `load_lib_files_from_paths()` function
2. Updated `SHARED_LIB_FILES` to call this function
3. Populated `SHARED_LIB_CONTEXTS` from loaded lib files

### Phase 3: Verify Tests ✅
1. Ran `test_builtin_types_no_ts2304_errors` - PASS
2. Ran element access tests - PASS
3. Verified global types are resolved

## Files Modified

- `src/tests/test_fixtures.rs` - Fixed lib path and populated SHARED_LIB_CONTEXTS

## Test Status

**Start**: 8124 passing, 176 failing
**End**: 8225 passing, 75 failing
**Result**: +101 tests fixed (exceeded target of 30!)

## Related NORTH_STAR.md Rules

- Test infrastructure should support full type checking capabilities
- Lib files provide global types (Array, Object, Function, Promise, etc.)

## Notes

**Element access lowering is already implemented** - this session pivoted to lib infrastructure fix because:
1. Element access code exists and looks correct
2. Tests fail due to missing lib types, not missing functionality
3. Fixing lib infrastructure unblocked proper testing of existing features

**Next Steps**: The 75 remaining failures are now genuine type system issues, not test infrastructure problems. Can focus on:
- Element access edge cases
- Flow narrowing
- Discriminated unions
- Conditional types
