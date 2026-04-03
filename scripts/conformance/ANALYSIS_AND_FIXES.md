# Conformance Test Analysis and Fixes

## Date: April 3, 2026

## Summary

Analyzed 66 conformance tests that are "one error away" from passing. These tests fail because exactly one diagnostic code is missing from the expected output.

## Priority Fixes (Target: +12-15 tests)

### 1. TS2322 - Type Not Assignable (7 tests) ✅ PARTIALLY FIXED

**Tests affected:**
- indexedAccessRelation.ts - Intersection with indexed access types
- mergedDeclarations7.ts - Interface return type mismatch  
- typedArraysCrossAssignability01.ts - Typed array [Symbol.toStringTag] property mismatch
- iterableTReturnTNext.ts - Iterator return types
- restElementWithAssignmentPattern2.ts - Destructuring assignment
- importTag17.ts - JSDoc import
- tsxElementResolution15.tsx - JSX element resolution

**Root Cause:**
The `should_suppress_assignability_diagnostic()` function in `assignability_checker.rs` was suppressing TS2322 errors when:
- Target has complex generic constraints (e.g., indexed access types)
- Source contains type parameters
- Target is not callable

This suppression logic (lines 844-846) was intended to avoid false positives for complex generic constraints, but it was also suppressing legitimate errors for indexed access types that could resolve to incompatible concrete types.

**Fix Applied:**
Added a check to NOT suppress TS2322 when the target type contains indexed access types. The fix adds a `target_contains_indexed_access()` helper that checks:
1. If the target itself is an indexed access type
2. If the target is a union containing indexed access types

**File:** `crates/tsz-checker/src/assignability/assignability_checker.rs`

**Lines:** 820-849 (new helper function), 864-867 (condition added)

### 2. TS2339 - Property Does Not Exist (7 tests) - NOT YET FIXED

**Tests affected:**
- inferFromGenericFunctionReturnTypes1.ts - Property access on incompatible types
- mixinPrivateAndProtected.ts - Property access on intersection reduced to 'never'
- importTsBeforeDTs.ts - Module augmentation property resolution

**Root Cause:**
Property access type resolution may not be emitting TS2339 for:
1. Property access on 'never' type (should always error)
2. Missing properties on complex generic types

**Potential Fix Location:**
`crates/tsz-checker/src/types/property_access_type.rs` around line 1444

### 3. TS2307 - Cannot Find Module (7 tests) - NOT YET FIXED

**Tests affected:**
- symbolLinkDeclarationEmitModuleNames.ts
- symbolLinkDeclarationEmitModuleNamesImportRef.ts
- Various symlink-related tests

**Root Cause:**
Module resolution errors not being emitted for specific import patterns involving:
- Symbolic links
- Monorepo structures
- Self-referencing packages

**Potential Fix Location:**
`crates/tsz-checker/src/state/type_resolution/module.rs`

### 4. Other One-Missing Tests (45 tests)

- TS2345: 4 tests (argument type)
- TS2304: 2 tests (cannot find name)
- TS2305: 2 tests (module has no exported member)
- TS2591: 1 test (cannot find module)
- TS2430: 1 test (property missing)
- TS2314: 1 test (no implicit any)
- TS5107: 1 test (lib compiler option)
- And 33 more...

## Implementation Strategy

The fixes should be surgical - adding missing error emission without changing core type system behavior:

1. **TS2322 fix** - Modify suppression logic to allow errors for indexed access types ✅
2. **TS2339 fix** - Ensure property access on 'never' and complex types emits errors
3. **TS2307 fix** - Verify module resolution error emission paths
4. **Quick wins** - Single line fixes for the 30+ remaining one-missing tests

## Verification

To verify the fixes, run:
```bash
./scripts/conformance/conformance.sh run --filter "indexedAccessRelation|mergedDeclarations7|typedArraysCrossAssignability01"
```

Expected: These tests should now emit TS2322 errors and pass.
