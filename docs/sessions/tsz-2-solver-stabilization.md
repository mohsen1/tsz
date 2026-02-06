# Session tsz-2: Solver Stabilization

**Started**: 2026-02-05 (redefined from Application expansion session)
**Status**: Active
**Goal**: Reduce failing solver tests from 31 to zero

## Context

Original tsz-2 session (Application expansion) was completed successfully. This session is now focused on solver test stabilization.

**Recent Progress** (commit 64be9be59):
- ✅ Fixed function contravariance in strict mode (AnyPropagationMode::TopLevelOnly)
- ✅ Fixed interface lowering (Object vs ObjectWithIndex)
- ✅ **Fixed generic inference in Round 2** - Preserved placeholder connections for unresolved type parameters
- ✅ **Fixed intersection normalization** - Added `null & object = never` rule
- ✅ **Fixed property access for Array/Tuple** - Added type substitution for generic applications
- ✅ **Fixed function variance tests** - Fixed test bugs (missing strict_function_types, incorrect any expectation)
- ✅ **Fixed constraint resolution** - Fixed widen_candidate_types to widen literals with multiple candidates
- ✅ **Fixed disjoint primitive intersection** - Added `string & number = never` reduction
- ✅ **Fixed weak type detection** - Added disjoint properties check to shallow subtype
- Reduced test failures from 37 → 31 → 22 → 20 → 13 → 11 → 9 → 8 → 5

## Redefined Priorities (2026-02-05 by Gemini)

### ✅ Priority 1: Intersection Normalization (2 tests) - COMPLETED
**Fixed**: Added `intersection_has_null_undefined_with_object()` method in `src/solver/intern.rs`
**Tests**:
- ✅ `test_intersection_null_with_object_is_never`
- ✅ `test_intersection_undefined_with_object_is_never`

---

### ✅ Priority 2: Property Access: Arrays & Tuples (7 tests) - COMPLETED
**Fixed**: Added type substitution for `TypeKey::Application` with `Object` and `ObjectWithIndex` base types
**Root Cause**: `resolve_application_property` only handled `Callable` and `Lazy` base types, missing the test setup which uses `Object`
**Solution**: Added handlers for `TypeKey::Object` and `TypeKey::ObjectWithIndex` that:
1. Get type params from `self.db.get_array_base_type_params()`
2. Create substitution with `TypeSubstitution::from_args()`
3. Instantiate property type with `instantiate_type_with_infer()`
4. Handle `this` types with `substitute_this_type()`

**Tests**:
- ✅ `test_property_access_array_at_returns_optional_element`
- ✅ `test_property_access_array_entries_returns_tuple_array`
- ✅ `test_property_access_array_map_signature`
- ✅ `test_property_access_array_push_with_env_resolver`
- ✅ `test_property_access_array_reduce_callable`
- ✅ `test_property_access_readonly_array`
- ✅ `test_property_access_tuple_length`

**Files Modified**:
- `src/solver/operations_property.rs` - Added `Object`/`ObjectWithIndex` handlers in `resolve_application_property`
- `src/solver/tests/operations_tests.rs` - Fixed test to call `interner.set_array_base_type()`

---

### ✅ Priority 3: Function Variance (2 tests) - COMPLETED
**Fixed**: Test bugs - no code changes needed
**Tests**:
- ✅ `test_any_in_function_parameters_strict_mode` - Fixed test to call `set_strict_function_types(true)`
- ✅ `test_function_variance_with_return_types` - Fixed incorrect expectation (any IS assignable to string)

**Root Cause**: Test bugs, not implementation bugs
1. `test_any_in_function_parameters_strict_mode` only called `set_strict_any_propagation(true)` but not `set_strict_function_types(true)`
2. `test_function_variance_with_return_types` incorrectly expected `() => any` NOT to be assignable to `() => string`

**Fix**:
- Test 1: Added `checker.set_strict_function_types(true)`
- Test 2: Changed expectation to `is_assignable(returns_any, returns_string)` because `any` is assignable to everything

---

### Priority 4: Generic Inference & Constraints (2 tests) - NEXT 🔴
**Tests**:
- `test_any_in_function_parameters_strict_mode`
- `test_function_variance_with_return_types`

**Context**: Edge cases in Lawyer layer (compatibility checking)

---

### ✅ Priority 4: Generic Inference & Constraints (2 tests) - COMPLETED
**Fixed**: Removed priority check in `widen_candidate_types` that prevented literal widening

**Tests**:
- ✅ `test_constraint_satisfaction_multiple_candidates`
- ✅ `test_resolve_multiple_lower_bounds_union`

**Root Cause**: `widen_candidate_types` had a check `candidate.priority != InferencePriority::NakedTypeVariable` that prevented widening for NakedTypeVariable candidates. But `add_lower_bound` uses `NakedTypeVariable` priority for all candidates.

**Example**: For `T extends string | number` with lower bounds `literal "hello"` and `literal 42`:
- Candidates: `[InferenceCandidate { type_id: "hello", priority: NakedTypeVariable, is_fresh_literal: true }, ...]`
- `widen_candidate_types` skipped widening because `priority == NakedTypeVariable`
- Result: `"hello" | 42` (union of literals) instead of `string | number` (union of widened types)

**Fix**: Removed the `candidate.priority != InferencePriority::NakedTypeVariable` check. The `is_const` parameter in `resolve_from_candidates` already protects const type parameters from unwanted widening.

**Files Modified**:
- `src/solver/infer.rs` - Fixed `widen_candidate_types` function

---

### ✅ Priority A: Structural Core - Intersection Merging (1 test) - COMPLETED
**Fixed**: Added disjoint primitive intersection reduction

**Tests**:
- ✅ `test_intersection_object_same_property_intersect_types`

**Root Cause**: `intersect_types_raw()` didn't check for disjoint primitives like `string & number`

**Solution**: Added `has_disjoint_primitives()` check that:
1. Detects when intersection contains disjoint primitive types (string, number, boolean, bigint, symbol)
2. Returns `never` for disjoint primitives (e.g., `string & number = never`)
3. Handles literals correctly (e.g., `"hello" & 42 = never`)

**Files Modified**:
- `src/solver/intern.rs` - Added `PrimitiveKind` enum, `has_disjoint_primitives()`, `get_primitive_kind()`, `are_primitives_disjoint()`

---

### ✅ Priority B: Weak Type Detection (2 tests) - COMPLETED
**Fixed**: Added disjoint properties check to shallow subtype check

**Tests**:
- ✅ `test_weak_union_rejects_no_common_properties`
- ✅ `test_weak_union_with_non_weak_member_not_weak`

**Root Cause**: `is_object_shape_subtype_shallow` incorrectly returned `true` for objects with completely disjoint properties:
- `{ b?: number } <: { a?: number }` returned `true` (wrong!)
- This caused union `{a?: number} | {b?: number}` to be reduced to just `{a?: number}`, breaking weak union detection
- The function allowed missing optional properties in source, but didn't check if source had properties that target didn't know about

**Fix**: Added property overlap check in `is_object_shape_subtype_shallow`:
```rust
let has_any_property_overlap = s
    .properties
    .iter()
    .any(|sp| t.properties.iter().any(|tp| sp.name == tp.name));
if !has_any_property_overlap {
    return false;
}
```

This ensures that objects with completely disjoint properties are not considered subtypes, preventing incorrect union reductions while preserving valid reductions like `{a: 1} | {a: 1, b: 2}` → `{a: 1}`.

**Files Modified**:
- `src/solver/intern.rs` - Added disjoint properties check in `is_object_shape_subtype_shallow`

---

## Redefined Priorities (2026-02-06 by Gemini)

### ✅ Priority 1: Index Signature Inference - DELETED TESTS (Commit c1460e42c)

**Investigation Result**: After consulting Gemini Pro and verifying with tsc, the 4 "failing" tests were actually testing **incorrect TypeScript behavior**.

**Root Cause**: TypeScript does NOT infer type parameters from index signatures when the target property is **required**.

**Evidence**:
```typescript
function foo<T>(bag: { a: T }): T { return bag.a; }
const arg: { [k: string]: number } = {};
const result = foo(arg);
// tsc error: Property 'a' is missing in type '{ [k: string]: number; }'
// but required in type '{ a: unknown; }'
// Notice: T defaults to unknown, NOT number
```

**Solution**: Modified `constrain_index_signatures_to_properties` to only extract candidates from index signatures when the target property is **optional**.

**Files Modified**:
- `src/solver/operations.rs` - Added `if !prop.optional { continue; }` check
- `src/solver/tests/operations_tests.rs` - Deleted 4 incorrect tests with explanatory comments

**Deleted Tests**:
- `test_infer_generic_missing_property_uses_index_signature`
- `test_infer_generic_missing_numeric_property_uses_number_index_signature`
- `test_infer_generic_property_from_source_index_signature`
- `test_infer_generic_property_from_number_index_signature_infinity`

**Why This Is Correct**:
- TypeScript's inference mirrors assignability rules
- Required property `{ a: T }` is NOT satisfied by index signature `{ [k: string]: V }`
- Therefore, no inference happens - T defaults to unknown
- Optional property `{ a?: T }` WOULD be satisfied, and inference works

---

### ✅ Priority 2: Generic Fallback (Commit bf6c740d6) - COMPLETED

**Problem**: SubtypeChecker incorrectly allowed `is_assignable(source, T)` to return TRUE when source satisfied T's constraint. This is unsound.

**Example**:
```typescript
T extends { id: number }
source = { id: 5, name: 'hi' }

Old code: source is assignable to T (because source satisfies constraint)
New code: source is NOT assignable to T (T is opaque)
```

**Why This Matters**: T could be instantiated as a specific subtype like `{ id: number, tag: 'special' }` which source doesn't satisfy.

**Solution**: Modified `check_subtype_inner` in `src/solver/subtype.rs` (lines 1757-1764):
- When TARGET is a TypeParameter, return FALSE
- Concrete types are never assignable to opaque type parameters
- This applies whether T has a constraint or not
- Exceptions for never/any handled by wrapper code

**Tests**:
- ✅ `test_generic_parameter_without_constraint_fallback_to_unknown` - Fixed expectation
- Updated `test_unconstrained_generic_fallback_to_unknown` with correct expectation
- Deleted `test_generic_with_constraint_uses_constraint_not_any` - Incorrect expectations
- Deleted `test_multiple_generic_constraints` - Incorrect expectations

**Files Modified**:
- `src/solver/subtype.rs` - Lines 1757-1764
- `src/solver/tests/integration_tests.rs` - Updated/deleted tests
- `src/solver/compat.rs` - Removed debug eprintln

---

### ✅ Priority 3: Object Index Signatures (Complete) - COMPLETED
**Tests**:
- ✅ `test_object_with_index_satisfies_named_property_string_index`
- ✅ `test_object_with_index_satisfies_numeric_property_number_index`

**Status**: COMPLETED - Added soundness check in SubtypeChecker

---

### Priority 3: Narrowing `any` (1 test)
**Tests**:
- `test_narrow_by_typeof_any`

**Goal**: Ensure `typeof any === "typename"` narrows to that type.

**Files**: `src/solver/narrowing.rs`

---

### Priority 4: Generic Fallback (1 test)
**Tests**:
- `test_generic_parameter_without_constraint_fallback_to_unknown`

**Goal**: Ensure unconstrained generics default to `unknown` when inference fails.

**Files**: `src/solver/infer.rs`

---

### Priority 5: Keyof Union (1 test)
**Tests**:
- `test_keyof_union_string_index_and_literal_narrows`

**Goal**: Fix `keyof` distribution over unions with index signatures.

**Files**: `src/solver/operations.rs`

---

## Current Status (4 Failing Tests Remaining - 1 Solver, 3 Checker/Narrowing)

**Solver Tests**: 1 remaining
**Checker Tests**: 3 remaining (control flow / narrowing - pre-existing issues)

The solver stabilization has significantly progressed. Remaining issues are primarily in the checker's control flow analysis, not the type solver itself.

---

## Final Priorities (2026-02-06 by Gemini - Only 3 Tests Remaining!)

### 🔴 Priority 1: Narrowing `any` (High Value) - NEXT
**Test**: `test_narrow_by_typeof_any`
**File**: `src/solver/narrowing.rs`

**Why This Is #1**: This is the most critical remaining issue for practical usage. TypeScript developers frequently use `any` as a boundary type (e.g., `JSON.parse()`, API responses) and immediately use `typeof` checks to regain type safety.

**Current Behavior**: Compiler likely treats `any` as a "black hole" that absorbs the narrowing predicate, leaving it as `any`.

**Target Behavior**: `typeof any_var === "string"` must narrow `any_var` to `string` within the block.

**Architectural Alignment**: This belongs in the **Lawyer** layer logic within `narrow()`. While the **Judge** (strict set theory) might say "any is everything," the Lawyer knows that explicit runtime checks should override the `any` type.

**Implementation Strategy**:
1. Modify `Solver::narrow`
2. Detect if the `type_id` being narrowed is `Any`
3. If the `narrower` is a specific primitive type (derived from `typeof`), allow the narrowing to proceed

---

### 🟠 Priority 2: Keyof Union Distribution (Structural Integrity)
**Test**: `test_keyof_union_string_index_and_literal_narrows`
**File**: `src/solver/evaluate.rs` (or `operations.rs`)

**Why This Is #2**: Tests the correctness of the `keyof` operator when applied to unions. Fundamental to mapped types and advanced generics.

**The Rule**: `keyof (A | B) = (keyof A) & (keyof B)`

**The Complexity**: One member has a string index signature (`{ [k: string]: any }`), and the other has literals. The intersection of `string` (from index) and specific literals needs to be handled correctly.

**Risk**: High complexity. Incorrect implementation can break other `keyof` operations.

---

### 🟡 Priority 3: Template Literal with Any (Edge Case)
**Test**: `test_template_literal_with_any`
**File**: `src/solver/intern.rs` or `src/solver/operations.rs`

**Why This Is #3**: This is a specific edge case in type construction. While correct behavior is needed, it blocks fewer real-world patterns than narrowing or `keyof`.

**The Rule**: `` `prefix-${any}` `` usually collapses to `string` (not `any`, and not a template literal type).

**Value**: Low compared to the others.

---

## Current Status (3 Failing Solver Tests Remaining)

**Completed Priorities**:
- ✅ Priority 1: Index Signature Inference (deleted incorrect tests)
- ✅ Priority 2: Generic Fallback (fixed SubtypeChecker)

**Remaining**: 3 solver tests (Narrowing any, Keyof union, Template literal with any)

---

## Historical Context

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

## Remaining Failing Tests (5 tests)

### Still Failing: Generic Fallback (1 test)
- `test_generic_parameter_without_constraint_fallback_to_unknown`

### Still Failing: Keyof/Narrowing (2 tests)
- `test_keyof_union_string_index_and_literal_narrows`
- `test_narrow_by_typeof_any`

### Still Failing: Object with Index (2 tests)
- `test_object_with_index_satisfies_named_property_string_index`
- `test_object_with_index_satisfies_numeric_property_number_index`

## MANDATORY: Two-Question Rule

For ALL changes to `src/solver/` or `src/checker/`:

1. **Question 1** (Pre-implementation): Ask Gemini for approach validation
2. **Question 2** (Post-implementation): Ask Gemini Pro to review

Evidence from investigation: 100% of unreviewed solver/checker changes had critical type system bugs.

## Session History

*2026-02-05*: Redefined from Application expansion session to Solver Stabilization after Gemini consultation.
