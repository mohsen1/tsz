# Default Lib Loading Bug

**Status**: NEEDS FIX
**Discovered**: 2026-02-05
**Component**: CLI (config.rs, driver.rs), Binder (symbol merging)
**Conformance Impact**: Many TS2339, TS2349, TS2488 errors

## Problem

When using `--target es6` (or other ES versions), tsz loads the lib files but the symbol types are incorrect. The `Symbol` variable is resolved to the `Symbol` interface type instead of `SymbolConstructor`.

When using `--lib es6`, lib symbols are not found at all and fall back to `ANY`, which silently suppresses errors.

### Test Cases

```typescript
// test.ts
const s: SymbolConstructor = Symbol;  // Should pass, but fails with --target es6
const x = Symbol.iterator;             // Property 'iterator' not found
```

| Command | Result | Explanation |
|---------|--------|-------------|
| `--target es6` | TS2322, TS2339 | Symbol found but has wrong type (interface Symbol instead of SymbolConstructor) |
| `--lib es6` | No errors | Symbol NOT found, falls back to ANY, suppresses all property errors |
| TSC equivalent | No errors | Correctly resolves Symbol to SymbolConstructor |

## Root Cause Analysis (Updated)

### Investigation Findings

Debug tracing revealed:

```
# With --target es6:
[RESOLVE] 'Symbol' FOUND in scope at depth 0 (id=258)
Property 'iterator' does not exist on type 'Symbol'.

# With --lib es6:
[RESOLVE] 'Symbol' NOT FOUND - searched scopes, file_locals, and 0 lib binders
(Type = ANY, no property errors reported)
```

### Two Distinct Issues

1. **`--target es6` Problem**: Symbol is found in scope (merged into binder) but has the wrong type
   - The lib files ARE loaded and parsed correctly (16 files for ES2015)
   - Symbols ARE merged into the main binder
   - But `declare var Symbol: SymbolConstructor` resolves to interface `Symbol` instead of `SymbolConstructor`
   - Likely a bug in `compute_type_of_symbol` or type annotation resolution for lib symbols

2. **`--lib es6` Problem**: Lib symbols are not found at all
   - The alias "es6" -> "es2015.full" requires a file that doesn't exist (`lib.es2015.full.d.ts`)
   - Falls back to embedded libs which may not be loading correctly
   - Results in 0 lib binders, so symbols default to ANY
   - This masks type errors rather than fixing them

### Type Resolution Bug

For `declare var Symbol: SymbolConstructor`, the expected flow is:
1. Find Symbol variable symbol (flags = VARIABLE)
2. Get type annotation node (`SymbolConstructor`)
3. Resolve `SymbolConstructor` to its interface type
4. Return that as the variable's type

But tsz appears to return the `Symbol` interface type instead, possibly:
- Confusing the Symbol interface with the Symbol variable
- Not properly handling type annotation resolution for lib symbols
- Using the wrong symbol when both interface and variable exist with same name

## Key Files

- `src/cli/config.rs`:
  - `resolve_default_lib_files()` (line 777)
  - `default_libs_for_target()` (line 820) - list looks correct

- `src/cli/driver.rs`:
  - `load_lib_files_for_contexts()` (line 1658)
  - `merge_lib_contexts_into_binder()` call at line 1721

- `src/binder/state.rs`:
  - `merge_lib_contexts_into_binder()` (line 1183) - symbol merging logic

- `src/checker/state_type_analysis.rs`:
  - `compute_type_of_symbol()` (line 988) - type resolution for variables

## Expected Behavior

When `--target es6` is specified:
1. Lib files should load (WORKS)
2. Symbols should merge into binder (WORKS)
3. `Symbol` variable should have type `SymbolConstructor` (BROKEN)
4. `Symbol.iterator` should resolve to the `iterator` property (BROKEN)

## Recommended Fix Approach

1. Investigate `compute_type_of_symbol` for VARIABLE symbols from lib files
2. Ensure type annotation (`SymbolConstructor`) is resolved in lib context
3. May need to ensure value_resolver is properly set when lowering lib types
4. Check symbol flag merging when interface and variable have same name

## Workaround

Currently there is no reliable workaround:
- `--lib es6` avoids the error but only because it falls back to ANY
- `--target es6 --lib es6` might work but has same issues

For now, code using Symbol requires `noLib` and manual type declarations.
