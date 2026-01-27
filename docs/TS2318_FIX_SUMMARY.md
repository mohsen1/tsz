# TS2318 Global Type Loading Fix

## Problem
TS2318 errors ("Cannot find global type 'X'") were not being emitted when `--noLib` was specified, because embedded lib files were being loaded even when lib paths were intentionally empty.

## Root Cause
In `src/cli/driver.rs`, the `load_lib_files_for_contexts` function had a fallback mechanism that would load embedded lib files when:
1. No disk files were loaded, OR
2. Core types (Object, Array) were missing

This fallback was unconditional, meaning even when `--noLib` was specified (which sets `lib_paths` to empty), embedded libs were still being loaded.

## Fix
Modified `load_lib_files_for_contexts` in `/Users/claude/code/tsz/src/cli/driver.rs` to check if lib loading was intentionally disabled:

```rust
// If no disk files were loaded OR core types are missing, fall back to embedded libs
// This ensures global types are always available even when disk files fail to parse
// IMPORTANT: Only load embedded libs if lib_files was not intentionally empty (i.e., noLib is false)
// When lib_files is empty and we tried to load disk files, it means either:
// 1. noLib is true (don't load ANY libs)
// 2. Disk files don't exist (load embedded libs as fallback)
let should_fallback_to_embedded = !lib_files.is_empty() || !lib_contexts.is_empty();
if (lib_contexts.is_empty() || !has_core_types) && should_fallback_to_embedded {
    // Load embedded libs...
}
```

The key change is the `should_fallback_to_embedded` condition:
- If `lib_files` is NOT empty (user specified libs): Allow fallback to embedded libs if disk files fail
- If `lib_contexts` is NOT empty (some disk files were loaded): Allow fallback to embedded libs for missing core types
- If both are empty (noLib is true): Do NOT load embedded libs

## Verification
The fix has been verified with the following test cases:

### Test 1: Object with --noLib
```typescript
let obj: Object;
```
Expected: `TS2318: Cannot find global type 'Object'`
Result: ✓ PASS

### Test 2: Promise with --noLib
```typescript
let prom: Promise<string>;
```
Expected: `TS2583: Cannot find name 'Promise'. Do you need to change your target library?`
Result: ✓ PASS

### Test 3: console with --noLib
```typescript
console.log("test");
```
Expected: `TS2304: Cannot find name 'console'`
Result: ✓ PASS

### Test 4: Array syntax with --noLib
```typescript
let arr: number[];
```
Expected: No error (array syntax is special-cased to create array type)
Result: ✓ PASS

### Test 5: Object without --noLib
```typescript
let obj: Object;
```
Expected: No error (lib files are loaded)
Result: ✓ PASS

## Impact
This fix ensures that when `--noLib` is specified, the compiler correctly emits errors for missing global types, matching TypeScript's behavior. This is important for:

1. **Test suites**: TypeScript conformance tests that use `@noLib: true` expect TS2318 errors for global types
2. **Minimal environments**: Projects that don't want any lib types loaded should get appropriate errors
3. **Error detection**: Missing global types are now properly reported instead of being silently resolved from embedded libs

## Files Modified
- `/Users/claude/code/tsz/src/cli/driver.rs` - Modified `load_lib_files_for_contexts` function

## Related Issues
- Task #6: Fix TS2318 global type loading
- TS2318 errors were missing 228x in baseline conformance tests
