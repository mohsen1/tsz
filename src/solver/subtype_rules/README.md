# Subtype Rules Module Structure

This directory contains the refactored subtype checking logic, organized by type category for maintainability.

## Module Overview

| Module | Description | Functions |
|--------|-------------|-----------|
| `intrinsics.rs` | Primitive/intrinsic type compatibility | `check_intrinsic_subtype`, `is_object_keyword_type`, `is_callable_type`, `apparent_primitive_*` |
| `literals.rs` | Literal types and template literal matching | `check_literal_to_intrinsic`, `check_literal_matches_template_literal`, `match_*_pattern` |
| `unions.rs` | Union and intersection type logic | `check_union_source_subtype`, `check_union_target_subtype`, `check_intersection_*_subtype` |
| `tuples.rs` | Array and tuple compatibility | `check_tuple_subtype`, `check_array_to_tuple_subtype`, `check_tuple_to_array_subtype` |
| `objects.rs` | Object property matching and index signatures | `check_object_subtype`, `check_property_compatibility`, `check_*_index_compatibility` |
| `functions.rs` | Function/callable signature compatibility | `check_function_subtype`, `check_callable_subtype`, `check_call_signature_subtype` |
| `generics.rs` | Type parameters, references, and applications | `check_ref_*_subtype`, `check_application_*_subtype`, `try_expand_*` |
| `conditionals.rs` | Conditional type checking | `check_conditional_subtype`, `conditional_branches_subtype`, `subtype_of_conditional_target` |

## Integration Status

**Current Status**: Modules created but NOT integrated into the main build.

The modules are structured as `impl` blocks that extend `SubtypeChecker`. To integrate them:

1. Enable the module in `solver/mod.rs`:
   ```rust
   mod subtype_rules;
   ```

2. Remove the corresponding functions from `subtype.rs` to avoid duplicate definitions.

3. Ensure `SubtypeChecker` fields are `pub(crate)` (already done).

## Function Migration Checklist

When enabling a module, remove these functions from `subtype.rs`:

### conditionals.rs
- [ ] `check_conditional_subtype` (line ~1346)
- [ ] `conditional_branches_subtype` (line ~1398)
- [ ] `subtype_of_conditional_target` (line ~1434)

### unions.rs
- [ ] `check_union_source_subtype` (line ~3484)
- [ ] `check_union_target_subtype` (line ~3544)
- [ ] `check_intersection_source_subtype` (line ~3588)
- [ ] `check_intersection_target_subtype` (line ~3647)
- [ ] `types_equivalent` (line ~1464)
- [ ] `union_includes_keyof_primitives` (line ~1480)
- [ ] `check_type_parameter_subtype` (line ~3661)
- [ ] `check_subtype_with_method_variance` (line ~3729)
- [ ] `explain_failure_with_method_variance` (line ~3748)

### intrinsics.rs
- [ ] `check_intrinsic_subtype` (line ~963)
- [ ] `is_object_keyword_type` (line ~1544)
- [ ] `is_callable_type` (around line ~1600)
- [ ] `apparent_primitive_*` functions

### literals.rs
- [ ] `check_literal_to_intrinsic`
- [ ] `check_literal_matches_template_literal`
- [ ] `match_template_literal_recursive`
- [ ] All `match_*_pattern` functions
- [ ] Helper functions: `format_number_for_template`, `find_number_length`, etc.

### tuples.rs
- [ ] `check_tuple_subtype`
- [ ] `check_array_to_tuple_subtype`
- [ ] `tuple_allows_empty`
- [ ] `check_tuple_to_array_subtype`
- [ ] `expand_tuple_rest`
- [ ] `get_array_element_type`

### objects.rs
- [ ] `lookup_property`
- [ ] `check_private_brand_compatibility`
- [ ] `check_object_subtype`
- [ ] `check_property_compatibility`
- [ ] `check_string_index_compatibility`
- [ ] `check_number_index_compatibility`
- [ ] `check_object_with_index_subtype`
- [ ] `check_object_with_index_to_object`
- [ ] `check_missing_property_against_index_signatures`
- [ ] `check_properties_against_index_signatures`
- [ ] `check_object_to_indexed`
- [ ] `optional_property_type`
- [ ] `optional_property_write_type`

### functions.rs
- [ ] `are_parameters_compatible`
- [ ] `are_type_predicates_compatible`
- [ ] `are_parameters_compatible_impl`
- [ ] `type_contains_this_type` and `type_contains_this_type_inner`
- [ ] `are_this_parameters_compatible`
- [ ] `required_param_count`
- [ ] `extra_required_accepts_undefined`
- [ ] `check_return_compat`
- [ ] `check_function_subtype`
- [ ] `check_function_to_callable_subtype`
- [ ] `check_callable_to_function_subtype`
- [ ] `check_callable_subtype`
- [ ] `check_call_signature_subtype`
- [ ] `check_call_signature_subtype_to_fn`
- [ ] `check_call_signature_subtype_fn`
- [ ] `evaluate_type`

### generics.rs
- [ ] `check_resolved_pair_subtype`
- [ ] `check_ref_ref_subtype`
- [ ] `check_typequery_typequery_subtype`
- [ ] `check_ref_subtype`
- [ ] `check_to_ref_subtype`
- [ ] `check_typequery_subtype`
- [ ] `check_to_typequery_subtype`
- [ ] `check_application_to_application_subtype`
- [ ] `check_application_expansion_target`
- [ ] `check_source_to_application_expansion`
- [ ] `check_mapped_expansion_target`
- [ ] `check_source_to_mapped_expansion`
- [ ] `try_expand_application`
- [ ] `try_expand_mapped`
- [ ] `try_evaluate_mapped_constraint`
- [ ] `try_get_keyof_keys`

## Testing

After integration, run the full test suite:
```bash
cargo test
```

Pay special attention to:
- `src/solver/subtype_tests.rs`
- `src/solver/compat_tests.rs`
- Integration tests in `src/solver/integration_tests.rs`

## Benefits of This Structure

1. **Maintainability**: Each module focuses on one type category
2. **Testability**: Modules can be tested in isolation
3. **Documentation**: Each module is well-documented with TypeScript examples
4. **Code Navigation**: Easy to find relevant code by type category
5. **Reduced File Size**: `subtype.rs` goes from ~5,800 lines to ~500 lines (coordinator only)
