# Default Lib Loading Bug

**Status**: NEEDS FIX
**Discovered**: 2026-02-05
**Component**: CLI (config.rs, driver.rs)
**Conformance Impact**: Many TS2339, TS2349, TS2488 errors

## Problem

When using `--target es6` (or other ES versions), tsz does not properly load the default lib files for that target. The `Symbol` global is not available, causing errors when using ES6+ features like iterators, generators, and well-known symbols.

### Test Case

```typescript
// test_symbol.ts
let s = Symbol();
console.log(Symbol.iterator);
```

```bash
# Works with explicit --lib
./.target/release/tsz test_symbol.ts --noEmit --target es6 --lib es6
# Exit code: 0

# Fails without --lib
./.target/release/tsz test_symbol.ts --noEmit --target es6
# error TS2349: Type 'Symbol' has no call signatures.
# error TS2339: Property 'iterator' does not exist on type 'Symbol'.
```

### Comparison with TSC

```bash
# TSC loads lib files automatically based on target
npx tsc test_symbol.ts --noEmit --target es6 --listFiles
# Lists 18 lib files including lib.es2015.symbol.d.ts
```

## Root Cause Analysis

The issue appears to be in the lib resolution chain:

1. `resolve_compiler_options(None)` (no tsconfig):
   - Sets `lib_files = resolve_default_lib_files(DEFAULT_TARGET)` (ES5)
   - Sets `lib_is_default = true`

2. `apply_cli_overrides`:
   - Updates `printer.target` to ES2015 (from `--target es6`)
   - Line 2748-2749: Should call `resolve_default_lib_files(ES2015)` but may not be working

3. `resolve_default_lib_files`:
   - Tries to find lib files via `default_lib_dir()`
   - Falls back to `materialize_embedded_libs` if disk libs not found

### Potential Issues

1. **Lib directory not found**: `default_lib_dir()` may fail, returning empty Vec before embedded libs fallback kicks in

2. **Embedded libs cache issue**: The `/tmp/tsz-embedded-libs` cache may be stale or incomplete

3. **Resolution order issue**: The `resolve_lib_files_with_options` function may be short-circuiting before trying the embedded libs path

### Key Files

- `src/cli/config.rs`:
  - `resolve_default_lib_files()` (line 777)
  - `default_lib_dir()` (line 948)
  - `default_libs_for_target()` (line 820)
  - `materialize_embedded_libs()` (line 1131)

- `src/cli/driver.rs`:
  - `apply_cli_overrides()` (line 2645)
  - `load_lib_files_for_contexts()` (line 1634)

### Debugging Steps

1. Add logging to `resolve_default_lib_files` to see what libs are being resolved
2. Check if `default_lib_dir()` is failing
3. Verify `materialize_embedded_libs` is being called and returning correct libs
4. Check if lib_files Vec is being properly passed to `load_lib_files_for_contexts`

## Expected Behavior

When `--target es6` is specified:
1. Default libs should include: es5, es2015.core, es2015.symbol, es2015.symbol.wellknown, etc.
2. The `Symbol` global should be available
3. `Symbol.iterator` and other well-known symbols should resolve

## Related Issues

- Many conformance tests fail due to this issue
- TS2488 errors (Symbol.iterator required for spread/for-of)
- TS2349 errors (Symbol() not recognized as callable)
- TS2339 errors (Symbol.iterator property missing)

## Workaround

Users can explicitly specify `--lib es6` to work around the issue:
```bash
tsz file.ts --target es6 --lib es6
```
