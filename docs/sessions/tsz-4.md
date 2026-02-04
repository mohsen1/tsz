# Session tsz-4

## Current Work

**Completed**: Fixed compilation errors from tsz-3's const type parameter work

Fixed all missing `is_const` field compilation errors in `TypeParamInfo` initializations:
- src/solver/tests/visitor_tests.rs (12 occurrences)
- src/checker/function_type.rs (1 occurrence)
- src/checker/state_checking_members.rs (3 occurrences)
- src/checker/state_type_analysis.rs (2 occurrences)

**Root cause**: tsz-3 added `is_const: bool` field to `TypeParamInfo` struct but didn't update all test and source files that create `TypeParamInfo` instances.

**Changes made**: Added `is_const: false` to all `TypeParamInfo` initializations (defaulting to non-const type parameters for existing code).

---

---

## Session Status

**Latest test status**: 7872 passing, 41 failing, 156 skipped (+5 tests from session start)

### Today's Work (2025-02-04)

**Total Impact**: Fixed compilation, updated tests, removed duplicates
- Started: Tests didn't compile (1000+ errors)
- Ended: **7872 passing, 41 failing, 156 skipped**

### Fixes Completed

1. **Fixed compilation errors from tsz-3's const type parameter work** (Commit: 9bf4c32e3)
   - Added `is_const: false` to 1000+ TypeParamInfo instances across 18 files

2. **Fixed selective migration tests for Phase 4.3** (Commit: 8eb262bdc)
   - Updated test expectations for classes/interfaces to reflect DefId creation
   - Fixed 2 tests

3. **Fixed duplicate is_const fields** (Commit: 72b63fcbb)
   - Removed duplicate `is_const: false` fields from all TypeParamInfo instances
   - Used sed/perl scripts to clean up automated fixes

### Remaining Failing Tests (41 total)

**Categories**:
- Namespace/module merging (10+ tests)
- Abstract constructors (2 tests)
- Symbol resolution (15+ tests)
- Readonly properties (4 tests)
- LSP signature help (2 tests)
- Complex type inference (8+ tests)

**Assessment**: All remaining failures require deep investigation into:
- Binder symbol resolution
- Module resolution and merging
- Abstract class semantics
- Complex type narrowing

These are complex architectural features, not quick fixes.

---

## Session Summary

**Total tests fixed/unignored: 17**
- 14 TS2304 (Cannot find name) tests - fixed missing `report_unresolved_imports` flag
- 2 TS2339 (Property not exist) tests - fixed unknown type error suppression
- 1 Type inference test - fixed Object vs Object comparison and optional property bounds checking

---

### 2025-02-04: Fixed type inference for optional properties in object bounds

Fixed two issues in the type inference system in `src/solver/infer.rs`:

**Issue 1: Object vs Object comparison**
- The `is_subtype` method didn't handle `TypeKey::Object` vs `TypeKey::Object` comparison
- Objects created with `interner.object()` would fall through the match and return false
- Added a new case to handle Object vs Object comparison using `object_subtype_of`

**Issue 2: Optional property write type checking**
- When source property is non-optional and target is optional, the write type check was failing
- The check `union(STRING, UNDEFINED) <: STRING` was incorrectly required
- Fixed by skipping write type check when source is non-optional and target is optional

**Tests fixed** (1 test):
- test_resolve_bounds_optional_property_compatible (no longer ignored)

---

### 2025-02-03: Fixed another TS2304 test (test_ts2304_emitted_for_undefined_name)

Added `report_unresolved_imports = true` to the `check_without_lib` helper function
in `src/checker/tests/ts2304_tests.rs`.

**Root cause**: Same issue as previous TS2304 tests - the helper function wasn't
setting the flag needed to report TS2304 errors for unresolved identifiers.

**Tests fixed** (1 test):
- test_ts2304_emitted_for_undefined_name (no longer ignored)

---

### 2025-02-03: Fixed TS2339 property access on unknown types

Fixed error reporting for property access on `unknown` types to match TypeScript behavior. Previously, tsz was suppressing TS2339 errors when accessing properties on `unknown` types, but TypeScript correctly reports these errors.

**Root cause**: `error_property_not_exist_at` in `src/checker/error_reporter.rs` was suppressing errors on `TypeId::UNKNOWN`. The suppression was intended to prevent cascading errors from unresolved types, but `unknown` is a valid type that should error on arbitrary property access.

**Changes made**:
- `src/checker/error_reporter.rs`: Removed `TypeId::UNKNOWN` from the error suppression list in `error_property_not_exist_at`
- `src/tests/checker_state_tests.rs`: Fixed and unignored `test_ts2339_catch_binding_unknown` (destructuring catch variables with `useUnknownInCatchVariables=true`)
- `src/tests/checker_state_tests.rs`: Fixed expectation in `test_ts2339_unknown_property_access_after_narrowing` (from 1 to 2 errors to match tsc)

**Tests fixed** (2 tests):
- test_ts2339_catch_binding_unknown (no longer ignored)
- test_ts2339_unknown_property_access_after_narrowing (fixed test expectation)

**Impact**: +2 tests passing (510 passed vs 508 before)

---

### 2025-02-03: Fixed 13 TS2304 (Cannot find name) ignored tests

Fixed all TS2304 ignored tests in `src/tests/checker_state_tests.rs` by adding the missing `checker.ctx.report_unresolved_imports = true;` flag and removing `#[ignore]` attributes.

**Root cause**: The tests were missing `checker.ctx.report_unresolved_imports = true;` before calling `checker.check_source_file(root);`. The flag defaults to `false` which suppresses TS2304 errors for unresolved identifiers in expressions.

**Tests fixed** (13 tests):
- test_missing_identifier_emits_2304
- test_ts2304_undeclared_var_in_function_call
- test_ts2304_undeclared_var_in_binary_expression
- test_ts2304_out_of_scope_block_variable
- test_ts2304_typo_with_suggestion
- test_ts2304_undeclared_var_in_return
- test_ts2304_undeclared_var_in_array_spread
- test_ts2304_undeclared_var_in_object_literal
- test_ts2304_undeclared_var_in_conditional
- test_ts2304_undeclared_class_in_extends
- test_ts2304_undeclared_interface_in_implements
- test_ts2304_undeclared_var_in_template_literal
- test_ts2304_undeclared_var_in_for_of

**Reduced ignored test count by 13** (from 87 to 74 total `#[ignore]` occurrences).

---

## History (Last 20)

### 2025-02-04: Fixed compilation errors and duplicate is_const fields

**Root cause**: tsz-3 added `is_const: bool` field to `TypeParamInfo` struct in `src/solver/types.rs` but didn't update all test and source files that create `TypeParamInfo` instances. Additionally, automated fixes created duplicate fields.

**Changes made**:
1. Added `is_const: false` to 1000+ `TypeParamInfo` instances across 18 files
2. Updated selective migration tests for Phase 4.3 behavior (classes/interfaces now have DefIds)
3. Removed duplicate `is_const: false` fields using sed/perl scripts

**Impact**: +5 tests (from non-compiling to 7872 passing, 41 failing, 156 skipped)

**Commits**:
- 9bf4c32e3 - Fix compilation errors
- 8eb262bdc - Fix selective migration tests
- 72b63fcbb - Remove duplicate is_const fields

---

### 2025-02-04: Fixed selective migration tests for Phase 4.3 behavior

**Root cause**: Tests were written for Phase 4.2.1 when DefId creation was selective, but commit `5e1495492` extended Lazy(DefId) pattern to all named types including classes and interfaces.

**Changes made**:
- Renamed `test_selective_migration_class_no_def_id` → `test_selective_migration_class_has_def_id`
- Renamed `test_selective_migration_interface_no_def_id` → `test_selective_migration_interface_has_def_id`
- Updated test expectations to verify DefIds ARE created (not that they aren't)

**Impact**: Fixed 2 tests (7867 passing → 7869 passing, 43 failing → 41 failing)

---

### 2025-02-04: Fixed compilation errors from tsz-3's const type parameter work

**Root cause**: tsz-3 added `is_const: bool` field to `TypeParamInfo` struct in `src/solver/types.rs` but didn't update all test and source files that create `TypeParamInfo` instances.

**Changes made**:
- src/solver/tests/visitor_tests.rs - Fixed 12 `TypeParamInfo` initializations
- src/solver/tests/evaluate_tests.rs - Fixed 1 `TypeParamInfo` initialization
- src/solver/tests/*.rs - Fixed remaining test files using automated script
- src/checker/function_type.rs - Fixed 1 occurrence
- src/checker/state_checking_members.rs - Fixed 3 occurrences
- src/checker/state_type_analysis.rs - Fixed 2 occurrences

All initializations now include `is_const: false` (defaulting to non-const type parameters for existing code).

**Impact**: Tests now compile successfully (7867 passing, 43 failing, 156 skipped).

**Note**: Several pre-existing test failures remain:
- `test_tail_recursive_conditional` - tsz-3's tail-recursion work has issues
- `test_generic_parameter_without_constraint_fallback_to_unknown` - Unknown fallback not working
- Readonly property tests (TS2540) - Failing due to symbol resolution issues (TS2318)

---

## Punted Todos

*No punted items*
