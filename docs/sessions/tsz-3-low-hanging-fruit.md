# Session TSZ-3: Low-Hanging Fruit Conformance Improvements

**Started**: 2026-02-05
**Status**: ✅ COMPLETED (Priority 1 fix complete)
**Focus**: Simple fixes with measurable conformance impact

## Summary

**Completed 2026-02-05**: Fixed TS2349/TS2511 error code mismatch for abstract class instantiation in union types.

### What Was Fixed

**Problem**: tsc reports TS2511 "Cannot create an instance of an abstract class" but tsz reported TS2349 "Type has no call signatures" when instantiating unions containing abstract class constructors.

**Example**:
```typescript
abstract class AbstractA { a: string; }
type Abstracts = typeof AbstractA;
declare const cls: Abstracts;
new cls(); // tsc: TS2511, tsz (before): TS2349, tsz (after): TS2511 ✅
```

**Root Cause**: When checking `new cls()` where `cls: typeof AbstractA | typeof AbstractB`:
1. Type aliases (Lazy types) weren't being checked for abstract symbols
2. Callable types (the resolved form of `typeof Class`) weren't being checked
3. Unions of type aliases weren't being resolved before checking

**Solution**:
1. Added Callable type check in `type_contains_abstract_class` - inspect the Callable's symbol field for the ABSTRACT flag
2. Added Lazy type handling with type alias recursion - check if mapped symbol is abstract, and if it's a type alias, recurse into the body
3. Modified `resolve_lazy_type` to handle unions/intersections by recursively resolving each member

**Impact**:
- Fixes false TS2349 errors on abstract class instantiation
- Correctly emits TS2511 for `new` expressions on unions containing abstract class constructors

**Files Modified**:
- `src/checker/type_computation_complex.rs`: Added Callable and Lazy type handling
- `src/checker/state_type_environment.rs`: Modified `resolve_lazy_type` to handle unions/intersections

**Test**: `TypeScript/tests/cases/compiler/abstractClassUnionInstantiation.ts`
- Before: Lines 14-15 showed TS2349 (incorrect)
- After: Lines 14-15 show TS2511 (correct)

## Remaining Priority Tasks

### Priority 2: TS1005 Parser Extra Errors (ASI-related)
**Impact**: ~50-100 false positives
**Complexity**: Low (parser fixes, often 1-line changes)
**Estimated Time**: 1-2 hours

**Problem**: Parser emits TS1005 in cases where TSC succeeds (automatic semicolon insertion)

**Fix Location**: `src/parser/` - likely ASI-related files
**Action**: Investigate failing TS1005 tests with `--error-code 1005 --max 20` and fix parser logic

**Test Cases**:
- TypeScript/tests/cases/compiler/asi*.ts tests

### Priority 3: TS2339 Property Access on String Literals
**Impact**: ~50-100 false positives
**Complexity**: Low-Medium (type computation fix)
**Estimated Time**: 2-3 hours

**Problem**: String literals are being treated as having all properties (any like in property access)

**Fix Location**: `src/checker/type_computation.rs`
**Function**: `compute_type_of_symbol` for STRING_LITERAL nodes
**Action**: When computing type of string literal in property access position, check if property exists in string literal types. If not, emit proper TS2339.

**Test Cases**:
- Any test with `"string".unknownProperty`

### Priority 4: TS2352 Object Literal Property Computation
**Impact**: ~30-50 false positives
**Complexity**: Low (object literal type computation)
**Estimated Time**: 1-2 hours

**Problem**: Object literal types not being computed correctly for excess property checks

**Fix Location**: `src/checker/object_literals.rs`
**Action**: Review excess property checking logic and ensure object literal types include all properties

**Test Cases**:
- TypeScript/tests/cases/compiler/excessPropertyChecks*.ts

## Dependencies

- **tsz-1**: Discriminant narrowing (COMPLETE)
- **tsz-3 previous**: CFA completeness (COMPLETE)

## Related Sessions

- **tsz-18**: Conformance testing infrastructure

## Session History

**Created 2026-02-05** following investigation that `import = require()` requires significant architectural work beyond the scope of low-hanging fruit fixes.

**Completed 2026-02-05**: Fixed TS2349/TS2511 error code mismatch for abstract class instantiation.

**Investigation Summary**:
- Multi-file test infrastructure (`@filename` directive) - ✅ Working
- `import = require()` namespace resolution - ❌ Requires cross-file module resolution infrastructure

**Key Finding**: `module_exports` HashMap in Binder is only for ambient modules. For `import = require()` to work, the checker needs to:
1. Resolve target file's binder
2. Collect exported symbols from target binder's `file_locals`
3. Construct module namespace object type

This requires changes in `src/checker/state_type_resolution.rs` in the `IMPORT_EQUALS_DECLARATION` handling.

---

*Session created by tsz-3 on 2026-02-05*

## Gemini's Recommended Low-Hanging Fruits

Based on current conformance data and codebase analysis:

### 1. Fix TS2349 Error Code Mismatch (Abstract Class Instantiation)
**Impact**: ~100-150 false positives
**Complexity**: Low (error code mapping fix)
**Estimated Time**: 1-2 hours

**Problem**:
- tsc reports: `TS2511: Cannot create an instance of an abstract class`
- tsz reports: `TS2349: Type has no call signatures`

**Fix Location**: `src/checker/type_computation.rs` or `src/checker/expr.rs`
**Action**: Detect when a type represents an abstract class and emit TS2511 instead of TS2349

**Test Cases**:
- TypeScript/tests/cases/compiler/abstractClassUnionInstantiation.ts

### 2. Fix TS2339 Property Access on String Literals
**Impact**: ~50-100 false positives
**Complexity**: Low-Medium (type computation fix)
**Estimated Time**: 2-3 hours

**Problem**: String literals are being treated as having all properties (any like in property access)

**Fix Location**: `src/checker/type_computation.rs`
**Function**: `compute_type_of_symbol` for STRING_LITERAL nodes
**Action**: When computing type of string literal in property access position, check if property exists in string literal types. If not, emit proper TS2339.

**Test Cases**:
- Any test with `"string".unknownProperty`

### 3. Fix TS1005 Parser Extra Errors (ASI-related)
**Impact**: ~50-100 false positives
**Complexity**: Low (parser fixes, often 1-line changes)
**Estimated Time**: 1-2 hours

**Problem**: Parser emits TS1005 in cases where TSC succeeds (automatic semicolon insertion)

**Fix Location**: `src/parser/` - likely ASI-related files
**Action**: Investigate failing TS1005 tests with `--error-code 1005 --max 20` and fix parser logic

**Test Cases**:
- TypeScript/tests/cases/compiler/asi*.ts tests

### 4. Fix TS2352 Object Literal Property Computation
**Impact**: ~30-50 false positives
**Complexity**: Low (object literal type computation)
**Estimated Time**: 1-2 hours

**Problem**: Object literal types not being computed correctly for excess property checks

**Fix Location**: `src/checker/object_literals.rs`
**Action**: Review excess property checking logic and ensure object literal types include all properties

**Test Cases**:
- TypeScript/tests/cases/compiler/excessPropertyChecks*.ts

## Implementation Plan

### Priority 1: TS2349 Error Code (1-2 hours)
1. Run `./scripts/conformance.sh run --error-code 2349 --max 20`
2. Pick a failing test case
3. Find where TS2349 is emitted for abstract class instantiation
4. Add logic to detect abstract classes and emit TS2511 instead
5. Test and commit

### Priority 2: TS1005 Parser Fixes (1-2 hours)
1. Run `./scripts/conformance.sh run --error-code 1005 --max 20`
2. Find pattern in ASI-related failures
3. Fix parser logic
4. Test and commit

### Priority 3: TS2339 String Literals (2-3 hours)
1. Find test case where string literal has incorrect property access
2. Trace type computation for string literals
3. Fix type computation logic
4. Test and commit

## MANDATORY Gemini Workflow

Per AGENTS.md, **MUST ask Gemini TWO questions** for any solver/checker changes:

### Question 1 (PRE-implementation)
```bash
./scripts/ask-gemini.mjs --include=src/checker/type_computation "
I need to fix TS2349 error code mismatch for abstract class instantiation.

Problem: tsc reports TS2511 but tsz reports TS2349

Planned approach: [YOUR APPROACH]

Before I implement:
1) Is this the right approach?
2) What functions should I modify?
3) What edge cases do I need to handle?
"
```

### Question 2 (POST-implementation)
```bash
./scripts/ask-gemini.mjs --pro --include=src/checker/type_computation "
I implemented TS2349->TS2511 fix in [FILE].

Changes: [PASTE CODE OR DESCRIBE CHANGES]

Please review:
1) Is this correct for TypeScript?
2) Did I miss any edge cases?
3) Are there type system bugs?
"
```

## Dependencies

- **tsz-1**: Discriminant narrowing (COMPLETE)
- **tsz-3 previous**: CFA completeness (COMPLETE)

## Related Sessions

- **tsz-18**: Conformance testing infrastructure

## Session History

**Created 2026-02-05** following investigation that `import = require()` requires significant architectural work beyond the scope of low-hanging fruit fixes.

**Investigation Summary**:
- Multi-file test infrastructure (`@filename` directive) - ✅ Working
- `import = require()` namespace resolution - ❌ Requires cross-file module resolution infrastructure

**Key Finding**: `module_exports` HashMap in Binder is only for ambient modules. For `import = require()` to work, the checker needs to:
1. Resolve target file's binder
2. Collect exported symbols from target binder's `file_locals`
3. Construct module namespace object type

This requires changes in `src/checker/state_type_resolution.rs` in the `IMPORT_EQUALS_DECLARATION` handling.

---

*Session created by tsz-3 on 2026-02-05*
