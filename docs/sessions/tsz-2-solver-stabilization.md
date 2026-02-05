# Session tsz-2: Solver Stabilization

**Started**: 2026-02-05 (redefined from Application expansion session)
**Status**: Active
**Goal**: Reduce failing solver tests from 31 to zero

## Context

Original tsz-2 session (Application expansion) was completed successfully. This session is now focused on solver test stabilization.

**Recent Progress** (commit 28888e435):
- ✅ Fixed function contravariance in strict mode (AnyPropagationMode::TopLevelOnly)
- ✅ Fixed interface lowering (Object vs ObjectWithIndex)
- ✅ **Fixed generic inference in Round 2** - Preserved placeholder connections for unresolved type parameters
- Reduced test failures from 37 → 31 → 22

## Redefined Priorities (2026-02-05 by Gemini Pro)

### Priority 1: Intersection Normalization (2 tests) - QUICK WIN 🔴
**Tests**:
- `test_intersection_null_with_object_is_never`
- `test_intersection_undefined_with_object_is_never`

**Problem**: `null & object` and `undefined & object` should reduce to `never` but don't

**Root Cause**: Missing reduction rule for disjoint primitive/object combinations

**Files**: `src/solver/operations.rs` (intersection factory/reduction logic)

**First Step**: Ask Gemini for approach validation before implementing

---

### Priority 2: Property Access: Arrays & Tuples (7 tests) - HIGH IMPACT 🟡
**Tests**:
- `test_property_access_array_at_returns_optional_element`
- `test_property_access_array_entries_returns_tuple_array`
- `test_property_access_array_map_signature`
- `test_property_access_array_push_with_env_resolver`
- `test_property_access_array_reduce_callable`
- `test_property_access_readonly_array`
- `test_property_access_tuple_length`

**Hypothesis**: Single root cause in how solver synthesizes properties for Array/Tuple intrinsics

**Files**: Property lookup logic for `TypeKey::Array` and `TypeKey::Tuple`

---

### Priority 3: Function Variance (2 tests) 🟢
**Tests**:
- `test_any_in_function_parameters_strict_mode`
- `test_function_variance_with_return_types`

**Context**: Edge cases in Lawyer layer (compatibility checking)

---

### Priority 4: Generic Inference & Constraints (2 tests) ⚪
**Tests**:
- `test_constraint_satisfaction_multiple_candidates`
- `test_resolve_multiple_lower_bounds_union`

**Context**: Most complex, tackle after property access is stabilized

---

### Priority 5: Weak Types & Others (9 tests) ⚪
**Tests**: Pre-existing weak type failures, conditional types, keyof, narrowing, etc.

**Strategy**: Leave for last unless blocking other progress

## Current Status (22 Failing Tests Remaining)

### Fixed: Generic Inference with Callback Functions (commit 28888e435)

**Root Cause**: In Round 2 of generic call resolution, `get_current_substitution()` was used to re-instantiate
target types for contextual arguments. This substitution maps unresolved type parameters to `UNKNOWN`,
breaking the connection to placeholder types.

**Example**: For `map<T, U>(array: T[], callback: (x: T) => U): U[]`:
- Callback parameter type: `(x: placeholder_T) => placeholder_U`
- When callback arg `(x: number) => string` is constrained:
  - Round 2 should collect: `string <: placeholder_U`
  - But `get_current_substitution()` returned `UNKNOWN` for U
  - Constraint was never added, U resolved to `UNKNOWN` instead of `string`

**Fix**: Use the original `target_type` (with placeholders) for constraint collection in Round 2,
instead of re-instantiating with resolved types. This preserves the placeholder connection
for unresolved type parameters.

**Files Modified**:
- `src/solver/operations.rs` (Round 2 contextual argument processing, lines 806-836)

### Secondary Focus: Intersection Normalization (5 tests)
**Fallback if Generic Inference takes > 1 hour**

**Problem**: `null & object` should reduce to `never`

**Gemini Question** (Pre-implementation):
```bash
./scripts/ask-gemini.mjs --include=src/solver/operations.rs --include=src/solver/intern.rs \
"I need to fix intersection normalization.
Problem: 'null & object' is not reducing to 'never'.
1. Where is the canonical place to add reduction rules?
2. Does TypeScript handle this via the Lawyer layer or the Judge layer?
3. Please show the correct pattern."
```

---

## Remaining Failing Tests (22 tests)

### Still Failing: Generic Inference Tests (2 tests)
- `test_constraint_satisfaction_multiple_candidates`
- `test_resolve_multiple_lower_bounds_union`

### Still Failing: Conditional Types (1 test)
- `test_conditional_infer_optional_property_non_distributive_union_input`

### Still Failing: Generic Fallback (1 test)
- `test_generic_parameter_without_constraint_fallback_to_unknown`

### Still Failing: Intersection/Union (1 test)
- `test_intersection_object_same_property_intersect_types`

### Still Failing: Property Access (7 tests)
- `test_property_access_array_at_returns_optional_element`
- `test_property_access_array_entries_returns_tuple_array`
- `test_property_access_array_map_signature`
- `test_property_access_array_push_with_env_resolver`
- `test_property_access_array_reduce_callable`
- `test_property_access_readonly_array`
- `test_property_access_tuple_length`

### Still Failing: Function Variance (2 tests)
- `test_any_in_function_parameters_strict_mode`
- `test_function_variance_with_return_types`

### Still Failing: Intersection Normalization (2 tests)
- `test_intersection_null_with_object_is_never`
- `test_intersection_undefined_with_object_is_never`

### Still Failing: Keyof/Narrowing (2 tests)
- `test_keyof_union_string_index_and_literal_narrows`
- `test_narrow_by_typeof_any`

### Still Failing: Object with Index (2 tests)
- `test_object_with_index_satisfies_named_property_string_index`
- `test_object_with_index_satisfies_numeric_property_number_index`

### Still Failing: Weak Type Detection (2 tests) - 🟡 PRE-EXISTING
**Tests**:
- `test_weak_union_rejects_no_common_properties`
- `test_weak_union_with_non_weak_member_not_weak`

**Status**: Pre-existing failures, NOT a regression from commit ea1029cf3

**Issue**: `explain_failure` returns `None` instead of `TypeMismatch`

**Files**:
- `src/solver/compat.rs`
- `src/solver/lawyer.rs`

### Priority 3: Intersection Normalization (5 tests) - 🟢 PENDING
**Tests**:
- `test_intersection_null_with_object_is_never`
- `test_intersection_undefined_with_object_is_never`
- And 3 others...

**Issue**: `null & object` should reduce to `never` but doesn't

**Files**:
- `src/solver/operations.rs` (intersection factory function)

### Other Failing Tests (8 tests)
- Constraint resolution (2 tests)
- Narrowing (1 test)
- Conditional types (1 test)
- Generic fallback (1 test)
- Property intersection (1 test)
- Integration tests (2 tests)

## MANDATORY: Two-Question Rule

For ALL changes to `src/solver/` or `src/checker/`:

1. **Question 1** (Pre-implementation): Ask Gemini for approach validation
2. **Question 2** (Post-implementation): Ask Gemini Pro to review

Evidence from investigation: 100% of unreviewed solver/checker changes had critical type system bugs.

## Session History

*2026-02-05*: Redefined from Application expansion session to Solver Stabilization after Gemini consultation.
