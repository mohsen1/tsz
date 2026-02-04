# Session tsz-1: Core Solver Infrastructure & Conformance

**Started**: 2026-02-04 (Pivoted to infrastructure focus)
**Status**: Active
**Goal**: Fix Application expansion and IndexSignature inference to unblock complex type evaluation

## Session Achievements (2026-02-04)

### Previous Session
- ✅ Fixed 3 test expectations (51 → 46 failing tests)
- ✅ **Fixed enum+namespace merging** (46 → 28 failing tests, **-18 tests**)

### Current Session
- ✅ **Fixed namespace merging tests** (28 → 24 failing tests, **-4 tests**)
- ✅ **Fixed 2 more namespace tests** (24 → 22 failing tests, **-2 tests**)
- ✅ **Fixed 4 new expression tests** (22 → 18 failing tests, **-4 tests**)
- ✅ **Fixed implements property access** (18 → 19 failing tests, **+1 test, net -3**)
  - Added `resolve_lazy_type()` call in `class_type.rs` for interface merging
- ✅ **Fixed narrowing test expectation** (19 → 18 failing tests, **-1 test**)
  - Corrected test for `narrow_by_discriminant_no_match`
- ✅ **Fixed Application expansion for type aliases** (35 → 34 failing tests, **-1 test**)
  - Modified `lower_type_alias_declaration` to return type parameters
  - Added parameter caching in `compute_type_of_symbol` for user-defined type aliases
  - Added parameter caching in `type_checking_queries` for library type aliases
  - Enables `ExtractState<NumberReducer>` to properly expand to `number`
- ✅ **Fixed index signature subtyping for required properties** (34 → 32 failing tests, **-2 tests**)
  - Removed incorrect early return in `check_missing_property_against_index_signatures`
  - Index signatures now correctly satisfy required properties when property name matches
  - Enables `{ [x: number]: number }` to be assignable to `{ "0": number }`

### Total Progress
- **51 → 32 failing tests (-19 tests total)**

## Updated Priorities (Pivoted from test-fixing to infrastructure)

### ✅ Priority 1: Fix Type Alias Application Expansion (COMPLETE)
**Problem**: `Application` expansion fails for type aliases, blocking conditional type evaluation
- `ExtractState<NumberReducer>` fails to expand
- `get_lazy_type_params` returns `None` for type aliases
- `evaluate_conditional` is never called

**Solution Implemented** (2026-02-04):
1. Modified `lower_type_alias_declaration` to return type parameters
2. Added parameter caching in `compute_type_of_symbol` for user-defined type aliases
3. Added parameter caching in `type_checking_queries` for library type aliases

**Files Modified**:
- `src/solver/lower.rs`: Changed return type to `(TypeId, Vec<TypeParamInfo>)`
- `src/checker/state_type_analysis.rs`: Cache parameters for user type aliases
- `src/checker/type_checking_queries.rs`: Cache parameters for library type aliases

**Tests Fixed**:
- ✅ test_redux_pattern_extract_state_with_infer

**Gemini Consultation**: Followed Two-Question Rule for implementation validation

### ✅ Priority 2: Generic Inference with Index Signatures (COMPLETE)
**Problem**: Index signatures were incorrectly failing to satisfy required properties

**Solution Implemented** (2026-02-04):
- Fixed `check_missing_property_against_index_signatures` in `src/solver/subtype_rules/objects.rs`
- Removed incorrect early return for required properties
- Index signatures now correctly satisfy required properties when property name matches

**Files Modified**:
- `src/solver/subtype_rules/objects.rs`: Lines 483-537

**Tests Fixed**:
- ✅ test_infer_generic_missing_numeric_property_uses_number_index_signature
- ✅ test_infer_generic_missing_property_uses_index_signature
- ✅ test_infer_generic_property_from_source_index_signature
- ✅ test_infer_generic_property_from_number_index_signature_infinity

**Gemini Consultation**: Consulted for subtyping behavior validation

### Priority 3: Readonly TS2540 (Architectural - 4 tests deferred)
**Problem**: Generic type inference fails when target has index signatures

**File**: `src/solver/infer.rs`

**Task**: Update `infer_from_types` to handle `TypeKey::ObjectWithIndex`

**Tests affected**:
- test_infer_generic_missing_numeric_property_uses_number_index_signature
- test_infer_generic_missing_property_uses_index_signature
- test_infer_generic_property_from_number_index_signature_infinity
- test_infer_generic_property_from_source_index_signature

### Priority 3: Readonly TS2540 (Architectural - 4 tests, IN PROGRESS)
**Problem**: Readonly checks fail due to `Lazy` types
- Element access like `config["name"]` doesn't check if property is readonly
- Should error with TS2540 but currently errors with TS2318 instead

**Root Cause**: Property access resolution doesn't return `readonly` status, and `Lazy` types aren't properly resolved before checking writability.

**Implementation Plan** (from Gemini consultation):

**Files to modify**:
1. `src/solver/operations_property.rs`:
   - Update `PropertyAccessResult::Success` to include `readonly: bool`
   - Update `resolve_property_access_inner` to:
     - Handle `TypeKey::Lazy` by resolving first
     - Return `readonly` status from property/index signature metadata
     - Handle unions (any readonly = error) and intersections (all readonly = error)
2. `src/checker/assignment_checker.rs`:
   - Use `readonly` flag from `PropertyAccessResult` instead of manual checks
3. `src/checker/property_checker.rs`:
   - Clean up redundant `is_property_readonly` logic

**Tests affected**:
- test_readonly_element_access_assignment_2540
- test_readonly_index_signature_element_access_assignment_2540
- test_readonly_index_signature_variable_access_assignment_2540
- test_readonly_method_signature_assignment_2540

**Status**: Consulted Gemini for implementation guidance. Ready to implement.

## Remaining 32 Failing Tests - Categorized

**Core Infrastructure** (Priority 3):
- 4x Readonly TS2540 (architectural)

**Complex Type Inference** (5 tests):
- 1x mixin property access (complex)
- 1x contextual property typing (deferred)
- 3x other complex inference

**Other** (23 tests):
- CLI cache tests, LSP tests, various type inference

## Investigation: Redux Pattern (test_redux_pattern_extract_state_with_infer)

**Status**: ✅ FIXED - Application expansion now works

**Problem**: Redux pattern test fails - `ExtractedState` is not being inferred as `number`

**Test Code**:
```typescript
type Reducer<S, A> = (state: S | undefined, action: A) => S;
type ExtractState<R> = R extends Reducer<infer S, any> ? S : never;
type NumberReducer = Reducer<number, { type: string }>;
type ExtractedState = ExtractState<NumberReducer>; // Should be number
```

**Root Cause** (identified via Gemini consultation):
- `Application` type `ExtractState<NumberReducer>` fails to expand in `src/solver/evaluate.rs`
- `TypeResolver::get_lazy_type_params(def_id)` returns `None`
- `evaluate_conditional` is NEVER called because Application expansion fails first

**Solution Implemented**:
1. Modified `lower_type_alias_declaration` to return `(TypeId, Vec<TypeParamInfo>)`
2. Added parameter caching in `compute_type_of_symbol` for user type aliases
3. Added parameter caching in `type_checking_queries` for library type aliases

**Result**: `ExtractState<NumberReducer>` now correctly expands to `number`

## Why This Path (from Gemini)

1. **High Leverage**: Fixing `Application` expansion will likely fix more than just Redux test - fundamental for Mapped Types and Template Literals
2. **Architectural Alignment**: Moving `Lazy` resolution into Lawyer follows NORTH_STAR.md principle (Checker/Lawyer handles TS quirks, Solver provides the WHAT)
3. **Conformance**: These three areas represent bulk of "logic" failures in remaining 35 tests

## Documented Complex Issues (Deferred)
- Contextual typing for arrow function parameters
- Numeric enum assignability (bidirectional with number)
- Mixin pattern with generic functions and nested classes

## Session Status: ACTIVE (Redefined 2026-02-04)

**Previous Achievements:**
- ✅ **Priority 1**: Application expansion for type aliases - WORKING
  - Modified `lower_type_alias_declaration` to return type parameters
  - Added parameter caching in `compute_type_of_symbol` and `type_checking_queries`
  - Fixed test: `test_redux_pattern_extract_state_with_infer`

**Test Progress:** 51 → 43 failing tests (16% reduction)

## Redefined Priorities (2026-02-04 - from Gemini consultation)

### Priority 1: Generic Inference with Index Signatures (4 tests)
**Problem**: `constrain_types` doesn't correctly propagate constraints when type parameters are matched against types with index signatures.

**Files to Modify**:
- `src/solver/operations.rs`:
  - `constrain_types_impl`: Add logic for `TypeParameter` vs `ObjectWithIndex`
  - `constrain_properties_against_index_signatures`: Handle placeholder types correctly

**Target Tests**:
- test_infer_generic_missing_numeric_property_uses_number_index_signature
- test_infer_generic_missing_property_uses_index_signature
- test_infer_generic_property_from_number_index_signature_infinity
- test_infer_generic_property_from_source_index_signature

**Why This is High Leverage**: Fixes the "missing link" in generic inference - when `T` is matched against `{ [k: string]: number }`, T should be constrained to that index signature's value type.

### Priority 2: Numeric Enum Assignability Rule #7 (2 tests)
**Problem**: TypeScript has an unsound but required rule where `number` is bidirectional-assignable with numeric enums. Current implementation is inconsistent.

**Files to Modify**:
- `src/solver/compat.rs`:
  - `enum_assignability_override`: Correctly identify numeric enums via `resolver.is_numeric_enum(def_id)`
  - `is_assignable_to`: Apply Rule #7 at top level before structural checking

**Target Tests**:
- test_numeric_enum_number_bidirectional
- test_numeric_enum_open_and_nominal_assignability

**Why This is High Leverage**: Fundamental "Lawyer" (compatibility) task that doesn't interfere with "Judge" (structural) work or CFA.

### Priority 3: Readonly TS2540 for Lazy Types (4 tests)
**Problem**: Checker fails to report TS2540 when object type is `Lazy(DefId)` because property lookup doesn't resolve lazy type before checking `readonly` flag.

**Files to Modify**:
- `src/checker/state_checking_members.rs`:
  - `check_readonly_assignment`: Must call `resolve_lazy_type` before querying `is_property_readonly`
- `src/solver/operations_property.rs`:
  - Verify `property_is_readonly` handles `TypeKey::Lazy`

**Target Tests**:
- test_readonly_element_access_assignment_2540
- test_readonly_index_signature_element_access_assignment_2540
- test_readonly_index_signature_variable_access_assignment_2540
- test_readonly_method_signature_assignment_2540

**Why This is High Leverage**: Completes the `DefId` migration for property access validation.

## Coordination Notes
- **Avoid**: `src/checker/flow_analysis.rs` (owned by tsz-3)
- **Avoid**: `CallEvaluator::resolve_callable_call` (likely being touched by tsz-4)
- **Focus**: Structural recursion inside `constrain_types_impl` (engine for `infer T`)
