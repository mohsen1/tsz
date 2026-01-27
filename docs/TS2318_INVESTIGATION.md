# TS2318 Global Type Error Investigation

## Problem
Fix 3415 missing TS2318 errors (the top missing error in conformance tests).

## Changes Made

### 1. Array/ReadonlyArray --noLib Checks (Commit 15df4dc91)
Added --noLib checks in two locations in `src/checker/state.rs`:

#### Location 1: `get_type_from_type_reference` (line 1206-1238)
When `Array` or `ReadonlyArray` is referenced as a type name:
- If lib files are not loaded (--noLib), emit TS2318
- If lib files are loaded but Array not found, fall back to creating Array type

#### Location 2: `get_type_from_type_node_in_type_literal` (lines 3117-3128, 3186-3203)
Similar checks for when Array/ReadonlyArray is referenced in type literal contexts:
- With type arguments: `Array<number>`
- Without type arguments: `Array`

### 2. Missing Global Types (Commit f81497ff4)
Added the following types to the known global types list:

#### Pre-ES2015 Types (src/checker/context.rs)
- `IArguments`: Type for function arguments object

#### ES2015+ Types (src/lib_loader.rs)
- `CallableFunction`: ES2015 function type constraint
- `NewableFunction`: ES2015 constructor type constraint
- `Disposable`: ES2022 explicit resource management
- `AsyncDisposable`: ES2022 async explicit resource management

## Investigation Needed

### Issue: Conformance Test Results
- Baseline (500 tests): TS2318 missing 228x
- After fix (100 tests): TS2318 missing 3411x

The dramatic increase in missing errors suggests the fix may have broken existing TS2318 emission.

### Possible Causes
1. `resolve_named_type_reference` might be finding Array types even when lib is not loaded
2. The --noLib check might be in the wrong code path
3. Type array syntax (`T[]`) might be affected differently than type reference syntax (`Array<T>`)

### Test Case: noCrashOnNoLib.ts
```typescript
// @noLib: true
export function f() {
    let e: {}[] = [];  // Uses array type syntax, not Array reference
    while (true) {
      e = [...(e || [])];  // Spread operator
    }
}
```

Expected TS2318 errors from TSC:
- Array, Boolean, Function, IArguments, Number, Object, RegExp, String

Current TSZ behavior: Need to verify if TS2318 is being emitted correctly.

## Next Steps
1. Verify TS2318 emission for explicit Array<T> references with --noLib
2. Verify TS2318 emission for array type syntax (T[]) with --noLib  
3. Check if spread operator triggers Array type creation
4. Trace through `resolve_named_type_reference` to see if it's finding types when it shouldn't
