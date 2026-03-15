# Problem 1.2: Coverage Gaps in query_boundaries

**Date**: 2026-03-15
**Scope**: Cross-reference of wrapped solver APIs in `query_boundaries/` against COMPUTATION and CONSTRUCTION imports from Problem 1.1

## Summary

| Metric | Value |
|--------|-------|
| Total query_boundaries files | 28 |
| Solver APIs already wrapped | ~95 (type queries, relations, classification, etc.) |
| Unwrapped COMPUTATION imports (3+ checker files) | 7 distinct APIs |
| Unwrapped CONSTRUCTION imports | 1 (`TypeInterner`, 4 files) |
| Unwrapped INTERNAL imports | 1 (`RelationCacheKey`, 3 files) |
| Total direct bypass call sites needing wrappers | ~96 |

## Already-Wrapped Solver APIs (Catalog)

The following solver APIs are already mediated through `query_boundaries/`:

### Relations / Assignability (`assignability.rs`)
- `is_subtype_of` (via `is_fresh_subtype_of`)
- `query_relation` / `query_relation_with_resolver` (via `is_assignable_*`, `is_subtype_with_resolver`, `is_redeclaration_identical_with_resolver`, `are_types_overlapping_with_env`, `check_application_variance_assignability`)
- `query_relation_with_overrides` (via `is_assignable_with_overrides`, `check_assignable_gate_with_overrides`)
- `analyze_assignability_failure_with_resolver` (via `analyze_assignability_failure_with_context`)
- `type_queries::classify_for_assignability_eval`
- `type_queries::classify_for_excess_properties`
- `type_queries::contains_infer_types_db`
- `type_queries::contains_any_type`
- `type_queries::is_any_type`
- `type_queries::get_return_type`
- `type_queries::rewrite_function_error_slots_to_any`
- `type_queries::replace_function_return_type`
- `type_queries::erase_function_type_params_to_any`

### Call Resolution (`checkers/call.rs`)
- `operations::resolve_call_with_checker` (via `resolve_call`)
- `operations::resolve_new_with_checker` (via `resolve_new`)
- `operations::compute_contextual_types_with_compat_checker` (via `compute_contextual_types_with_context`)
- `get_contextual_signature_with_compat_checker` (via `get_contextual_signature`)
- `get_contextual_signature_for_arity_with_compat_checker`

### Common Queries (`common.rs`)
- 30+ type_queries wrappers (classification, shape access, union/intersection members, etc.)

### Flow Analysis (`flow_analysis.rs`)
- `query_relation` variants (via `is_assignable`, `is_assignable_strict_null`, `is_assignable_with_env`, `are_types_mutually_subtype*`)
- `utils::union_or_single` (via `union_types`)
- `is_compound_assignment_operator`, `map_compound_assignment_to_binary`
- `fallback_compound_assignment_result`
- `type_queries::widen_literal_to_primitive`
- `type_queries::instance_type_from_constructor`
- `type_queries::is_promise_like`
- `type_queries::get_application_info`

### Type Computation (`type_computation/core.rs`)
- `BinaryOpEvaluator::evaluate_plus_chain` (via `evaluate_plus_chain`)
- `type_queries::evaluate_contextual_structure_with`

### Other Modules
- Classification functions in `checkers/generic.rs`, `checkers/iterable.rs`, `checkers/constructor.rs`, `checkers/promise.rs`, `checkers/property.rs`
- State queries in `state/checking.rs`, `state/type_analysis.rs`, `state/type_environment.rs`, `state/type_resolution.rs`
- Diagnostic helpers in `diagnostics.rs`
- Dispatch helpers in `dispatch.rs`
- Property access queries in `property_access.rs`
- Class-related queries in `class.rs`, `class_type.rs`
- Type checking queries in `type_checking.rs`, `type_checking_utilities.rs`

---

## Unwrapped COMPUTATION/CONSTRUCTION APIs (3+ checker files)

### Priority 1: `PropertyAccessResult` (40 import sites across 21 files)

**Solver type**: `tsz_solver::operations::property::PropertyAccessResult`
**Category**: COMPUTATION (result type from property access evaluation)

**Checker files using it directly** (21 files):
1. `assignability/assignment_checker.rs`
2. `checkers/iterable_checker.rs` (3 uses)
3. `checkers/jsx_checker.rs` (6 uses)
4. `checkers/jsx_checker_attrs.rs` (4 uses)
5. `classes/class_checker.rs`
6. `classes/class_checker_compat.rs`
7. `classes/class_implements_checker.rs`
8. `declarations/import/core.rs`
9. `flow/control_flow/assignment.rs`
10. `state/state_checking/readonly.rs` (4 uses)
11. `state/type_analysis/computed_helpers.rs` (2 uses)
12. `state/type_analysis/computed_loops.rs`
13. `state/type_analysis/core.rs` (2 uses)
14. `state/variable_checking/destructuring.rs`
15. `types/computation/access.rs`
16. `types/computation/binary.rs`
17. `types/computation/helpers.rs`
18. `types/computation/identifier.rs`
19. `types/computation/object_literal_context.rs`
20. `types/property_access_helpers.rs` (2 uses)
21. `types/property_access_type.rs` (4 uses)
22. `types/utilities/core.rs`

**Analysis**: `PropertyAccessResult` is a result enum/struct returned by property access evaluation. It is used extensively as a return type and pattern-matched throughout the checker. Most uses are consuming the result of a property access computation that the solver performed. This is borderline between SAFE (it is a data type) and COMPUTATION (it comes from a computation API). Since it is a result type rather than a function that performs computation, wrapping may not be the right approach -- instead, re-exporting the type through a boundary module would suffice.

**Recommended wrapper location**: `query_boundaries/common.rs` -- add a re-export:
```rust
pub(crate) use tsz_solver::operations::property::PropertyAccessResult;
```

**Note**: The associated `PropertyAccessEvaluator` (1 file) is the actual computation entry point and should also be wrapped.

---

### Priority 2: `instantiate_type` (14 import sites across 10 files)

**Solver function**: `tsz_solver::instantiate_type`
**Category**: COMPUTATION (type instantiation/substitution)

**Checker files using it directly** (10 files):
1. `checkers/signature_builder.rs`
2. `classes/class_checker.rs` (2 uses)
3. `classes/class_checker_compat.rs` (2 uses)
4. `error_reporter/generics.rs`
5. `state/type_environment/core.rs` (3 uses)
6. `types/class_type/constructor.rs`
7. `types/class_type/core.rs`
8. `types/computation/call.rs`
9. `types/computation/tagged_template.rs`
10. `types/interface_type.rs`

**Analysis**: This is a core solver computation function that applies a `TypeSubstitution` to produce a new type. It is always used alongside `TypeSubstitution` (which IS already wrapped via re-export). The function signature is:
```rust
pub fn instantiate_type(db: &dyn TypeDatabase, type_id: TypeId, substitution: &TypeSubstitution) -> TypeId
```

**Recommended wrapper signature**:
```rust
pub(crate) fn instantiate_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    substitution: &TypeSubstitution,
) -> TypeId
```

**Recommended location**: `query_boundaries/state/type_environment.rs` (most uses are in type environment / class type building contexts). Could also go in `query_boundaries/common.rs` given its broad usage.

---

### Priority 3: `TypeSubstitution` (11 import sites across 9 files -- partially wrapped)

**Solver type**: `tsz_solver::TypeSubstitution`
**Category**: COMPUTATION (type substitution mapping)

**Checker files using it directly** (9 files):
1. `checkers/signature_builder.rs`
2. `classes/class_checker.rs`
3. `classes/class_checker_compat.rs`
4. `error_reporter/generics.rs`
5. `state/type_environment/core.rs` (3 uses)
6. `types/class_type/constructor.rs`
7. `types/class_type/core.rs`
8. `types/computation/call.rs`
9. `types/interface_type.rs`

**Analysis**: `TypeSubstitution` is already re-exported from `query_boundaries/checkers/call.rs` for use by the call checker, but it is not available in a general-purpose boundary module. The other 8 files import it directly from `tsz_solver`. This is a data structure (substitution map) rather than a function, so the proper fix is a re-export from a common location.

**Recommended wrapper**: Re-export from `query_boundaries/common.rs`:
```rust
pub(crate) use tsz_solver::TypeSubstitution;
```

**Recommended location**: `query_boundaries/common.rs`

---

### Priority 4: `BinaryOpEvaluator` (7 import sites across 6 files)

**Solver type**: `tsz_solver::BinaryOpEvaluator`
**Category**: COMPUTATION (binary operation type evaluation)

**Checker files using it directly** (6 files):
1. `assignability/assignment_checker.rs` (2 uses)
2. `dispatch.rs`
3. `error_reporter/operator_errors.rs`
4. `flow/control_flow/assignment.rs`
5. `types/computation/binary.rs`
6. `types/computation/helpers.rs`

**Analysis**: `BinaryOpEvaluator` is a solver struct that evaluates binary operations (`+`, `-`, `*`, etc.) to determine result types. It is constructed with a `QueryDatabase` reference and then called. A partial wrapper already exists in `type_computation/core.rs` for `evaluate_plus_chain`, but the general `BinaryOpEvaluator` is used directly for other operations.

**Recommended wrapper signature**:
```rust
pub(crate) fn evaluate_binary_op(
    db: &dyn QueryDatabase,
    left: TypeId,
    right: TypeId,
    operator: &str,
) -> BinaryOpResult

pub(crate) fn evaluate_compound_binary(
    db: &dyn QueryDatabase,
    left: TypeId,
    right: TypeId,
    operator_token: u16,
) -> BinaryOpResult
```

**Recommended location**: `query_boundaries/type_computation/core.rs` (extend existing module that already wraps `evaluate_plus_chain`)

---

### Priority 5: `CallResult` (5 import sites across 5 files -- partially wrapped)

**Solver type**: `tsz_solver::operations::CallResult` / `tsz_solver::CallResult`
**Category**: COMPUTATION (call resolution result)

**Checker files using it directly** (5 files):
1. `checkers/call_checker.rs` (2 uses -- already uses boundary for `resolve_call`)
2. `types/computation/call.rs`
3. `types/computation/call_inference.rs`
4. `types/computation/call_result.rs`
5. `types/computation/complex.rs`

**Analysis**: `CallResult` is a result type from call resolution. It is already used via the `checkers/call.rs` boundary in call_checker.rs, but other files import it directly. Since it is a data type (enum of call outcomes), a re-export is appropriate.

**Recommended wrapper**: Re-export from `query_boundaries/checkers/call.rs` (already imports it) -- just ensure it is accessible to the `types/computation/` modules:
```rust
pub(crate) use tsz_solver::operations::CallResult;
```

**Recommended location**: `query_boundaries/common.rs` for broad access, or `query_boundaries/type_computation/core.rs` for the computation-specific files.

---

### Priority 6: `TypeInterner` (4 import sites across 4 files)

**Solver type**: `tsz_solver::TypeInterner`
**Category**: CONSTRUCTION (direct type interning)

**Checker files using it directly** (4 files):
1. `classes/class_inheritance.rs`
2. `state/state_checking/readonly.rs`
3. `types/computation/access.rs`
4. `types/property_access_helpers.rs`

**Analysis**: `TypeInterner` provides direct access to the solver's type interning mechanism, allowing creation of new types. This is the most architecturally concerning bypass because it allows unconstrained type construction outside the boundary. Each use should be examined to determine if a specific type construction helper can replace the raw interner access.

**Recommended wrapper signature**: Rather than wrapping `TypeInterner` itself (which would leak the abstraction), create purpose-specific construction helpers:
```rust
pub(crate) fn intern_union_type(db: &dyn TypeDatabase, members: Vec<TypeId>) -> TypeId
pub(crate) fn intern_intersection_type(db: &dyn TypeDatabase, members: Vec<TypeId>) -> TypeId
pub(crate) fn intern_readonly_type(db: &dyn TypeDatabase, inner: TypeId) -> TypeId
```

**Recommended location**: New submodule `query_boundaries/type_construction.rs`

---

### Priority 7: `BinaryOpResult` (3 import sites across 3 files)

**Solver type**: `tsz_solver::BinaryOpResult`
**Category**: COMPUTATION (result of binary operation evaluation)

**Checker files using it directly** (3 files):
1. `assignability/assignment_checker.rs`
2. `flow/control_flow/assignment.rs`
3. `types/computation/binary.rs`

**Analysis**: `BinaryOpResult` is the result type returned by `BinaryOpEvaluator`. It should be wrapped alongside `BinaryOpEvaluator` (Priority 4).

**Recommended wrapper**: Re-export alongside the `BinaryOpEvaluator` wrappers:
```rust
pub(crate) use tsz_solver::BinaryOpResult;
```

**Recommended location**: `query_boundaries/type_computation/core.rs`

---

### Priority 8: `TypeEvaluator` (3 import sites across 2 files)

**Solver type**: `tsz_solver::TypeEvaluator`
**Category**: COMPUTATION (type evaluation engine)

**Checker files using it directly** (2 files):
1. `state/type_environment/lazy.rs`
2. `types/computation/object_literal_context.rs` (2 uses)

**Analysis**: `TypeEvaluator` is used to evaluate complex types (conditionals, mapped types, index access). This is a high-level solver computation entry point. Though only 2 files use it, it performs significant type computation.

**Recommended wrapper signature**:
```rust
pub(crate) fn evaluate_type(
    db: &dyn QueryDatabase,
    env: &TypeEnvironment,
    type_id: TypeId,
) -> TypeId
```

**Recommended location**: `query_boundaries/state/type_environment.rs` (most contextually appropriate)

---

### Priority 9: `RelationCacheKey` (3 import sites across 3 files)

**Solver type**: `tsz_solver::RelationCacheKey`
**Category**: INTERNAL (solver-internal cache key)

**Checker files using it directly** (3 files):
1. `assignability/assignability_checker.rs`
2. `assignability/subtype_identity_checker.rs`
3. `context/compiler_options.rs`

**Analysis**: This is an **architecture violation**. `RelationCacheKey` is a solver-internal type used for cache key construction. Checker code uses its `FLAG_*` constants to construct relation policy flags. The constants should be exposed through a boundary type or the flags should be passed as semantic booleans rather than raw bit flags.

**Recommended wrapper**: Create a boundary type or re-export only the flag constants:
```rust
pub(crate) struct RelationFlags;
impl RelationFlags {
    pub const STRICT_NULL_CHECKS: u16 = tsz_solver::RelationCacheKey::FLAG_STRICT_NULL_CHECKS;
    pub const STRICT_FUNCTION_TYPES: u16 = tsz_solver::RelationCacheKey::FLAG_STRICT_FUNCTION_TYPES;
    // ... other flags
}
```

**Recommended location**: `query_boundaries/assignability.rs` (already mediates relation queries)

---

## Below-Threshold APIs (< 3 files, notable mentions)

| API | Files | Notes |
|-----|-------|-------|
| `freshness::is_fresh_object_type` | 2 | Freshness queries, pair with `widen_freshness` |
| `freshness::widen_freshness` | 2 | Used in state and identifier computation |
| `IndexKind` / `IndexSignatureResolver` | 2-3 | Index signature resolution, computation |
| `TypeResolver` | 2 | Trait impl, hard to wrap |
| `PropertyAccessEvaluator` | 1 | Entry point for property access |
| `ApplicationEvaluator` | 1 | Application type evaluation |
| `substitute_this_type` | 1 | This-type substitution |
| `instantiate_generic` | 1 | Generic instantiation |
| `instantiate_type_with_depth_status` | 1 | Depth-tracked instantiation |
| `apply_const_assertion` | 1 | Const assertion widening |
| `expression_ops` | 1 | Expression operation helpers |

## Prioritized Action Plan

| Rank | API | Sites | Action | Location |
|------|-----|-------|--------|----------|
| 1 | `PropertyAccessResult` | 40 | Re-export type | `common.rs` |
| 2 | `instantiate_type` | 14 | Thin wrapper fn | `common.rs` |
| 3 | `TypeSubstitution` | 11 | Re-export type | `common.rs` |
| 4 | `BinaryOpEvaluator` | 7 | Wrapper fn(s) | `type_computation/core.rs` |
| 5 | `CallResult` | 5 | Re-export type | `common.rs` |
| 6 | `TypeInterner` | 4 | Purpose-specific fns | NEW: `type_construction.rs` |
| 7 | `BinaryOpResult` | 3 | Re-export type | `type_computation/core.rs` |
| 8 | `TypeEvaluator` | 3 | Wrapper fn | `state/type_environment.rs` |
| 9 | `RelationCacheKey` | 3 | Flag constants type | `assignability.rs` |

**Total call sites that would be mediated by wrapping these 9 APIs: ~90 of 112 COMPUTATION imports (80%)**

## Recommendations

1. **Start with re-exports** (Priority 1, 3, 5, 7): These are data types, not functions. Re-exporting them through `common.rs` is zero-risk and immediately reduces the bypass count by ~59 import sites.

2. **Wrap `instantiate_type` next** (Priority 2): This is the single highest-impact function wrapper, covering 14 call sites across 10 files. The function signature is stable and thin.

3. **Wrap `BinaryOpEvaluator`** (Priority 4): Create 2-3 purpose-specific wrapper functions rather than re-exporting the evaluator struct. This hides the construction pattern.

4. **Create `type_construction.rs`** (Priority 6): This addresses the most architecturally concerning bypass. Raw `TypeInterner` access should be replaced with purpose-specific construction helpers.

5. **Replace `RelationCacheKey` usage** (Priority 9): This is a true architecture violation. The flag constants should be exposed through a boundary type, and the raw `RelationCacheKey` import should be removed from checker code.

## Coverage After Completion

If all 9 APIs are wrapped:
- COMPUTATION bypasses: 112 -> ~22 (80% reduction)
- CONSTRUCTION bypasses: 4 -> 0 (100% reduction)
- INTERNAL bypasses: 3 -> 0 (100% reduction)
- Remaining bypasses are single-file uses (below the 3-file threshold)
