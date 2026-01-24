# Evaluate Rules Module Structure

This directory contains the planned modular structure for type evaluation logic, organized by type category for maintainability.

## Current Status

**Status**: Planning phase - this README documents the intended refactoring of `evaluate.rs`.

The `evaluate.rs` file is currently ~5,800 lines and handles all meta-type evaluation. This document outlines how to split it into focused modules.

## Planned Module Overview

| Module | Description | Functions |
|--------|-------------|-----------|
| `conditional.rs` | Conditional type evaluation | `evaluate_conditional`, `distribute_conditional`, `filter_inferred_by_constraint`, `try_evaluate_*_infer` |
| `index_access.rs` | Index access type evaluation | `evaluate_index_access`, `evaluate_object_index`, `evaluate_tuple_index`, `evaluate_array_index` |
| `mapped.rs` | Mapped type evaluation | `evaluate_mapped`, `is_homomorphic_mapped_type`, `extract_source_from_homomorphic`, `extract_mapped_keys` |
| `template_literal.rs` | Template literal evaluation | `evaluate_template_literal`, `extract_literal_strings`, `count_literal_members` |
| `keyof.rs` | keyof operator evaluation | `evaluate_keyof`, `keyof_union`, `keyof_intersection`, `array_keyof_keys`, `intersect_keyof_sets` |
| `string_intrinsic.rs` | String intrinsic evaluation | `evaluate_string_intrinsic`, `apply_string_intrinsic_to_template_literal`, `apply_string_transform` |
| `infer_pattern.rs` | Infer type pattern matching | `match_infer_pattern`, `bind_infer`, `substitute_infer`, `type_contains_infer` |
| `apparent.rs` | Apparent type utilities | `apparent_primitive_shape`, `apparent_method_type`, `apparent_primitive_keyof`, `array_member_kind` |

## Integration Plan

To integrate these modules:

1. Create each module file with functions as `impl<'a, R: TypeResolver> TypeEvaluator<'a, R>` blocks

2. Add accessor methods to `TypeEvaluator`:
   ```rust
   pub(crate) fn interner(&self) -> &'a dyn TypeDatabase { self.interner }
   pub(crate) fn resolver(&self) -> &'a R { self.resolver }
   pub(crate) fn no_unchecked_indexed_access(&self) -> bool { self.no_unchecked_indexed_access }
   pub(crate) fn is_depth_exceeded(&self) -> bool { *self.depth_exceeded.borrow() }
   pub(crate) fn set_depth_exceeded(&self, value: bool) { *self.depth_exceeded.borrow_mut() = value; }
   ```

3. Enable the module in `solver/mod.rs`:
   ```rust
   mod evaluate_rules;
   ```

4. Remove the corresponding functions from `evaluate.rs` to avoid duplicate definitions

## Function Migration Checklist

When enabling a module, remove these functions from `evaluate.rs`:

### conditional.rs (~600 lines)
- [ ] `evaluate_conditional` (line ~654)
- [ ] `distribute_conditional` (line ~1265)
- [ ] `filter_inferred_by_constraint` (line ~3156)
- [ ] `filter_inferred_by_constraint_or_undefined` (line ~3188)
- [ ] `try_evaluate_array_infer` (line ~852)
- [ ] `try_evaluate_tuple_infer` (line ~945)
- [ ] `try_evaluate_object_infer` (line ~1040)
- [ ] Helper functions for conditional evaluation

### index_access.rs (~400 lines)
- [ ] `recurse_index_access` (line ~1329)
- [ ] `evaluate_index_access` (line ~1339)
- [ ] `evaluate_object_index` (line ~1437)
- [ ] `evaluate_object_with_index` (line ~1477)
- [ ] `evaluate_array_index` (line ~1725)
- [ ] `evaluate_tuple_index` (line ~1833)
- [ ] `add_undefined_if_unchecked` (line ~1770)
- [ ] `rest_element_type` (line ~1777)
- [ ] `tuple_element_type` (line ~1796)
- [ ] `tuple_index_literal` (line ~1810)
- [ ] `is_number_like` (line ~1919)
- [ ] `optional_property_type` (line ~1555)
- [ ] `union_property_types` (line ~1564)

### mapped.rs (~250 lines)
- [ ] `evaluate_mapped` (line ~1974)
- [ ] `is_homomorphic_mapped_type` (line ~2150)
- [ ] `extract_source_from_homomorphic` (line ~2170)
- [ ] `evaluate_keyof_or_constraint` (line ~2790)
- [ ] `extract_mapped_keys` (line ~2815)
- [ ] `MappedKeys` struct (line ~67)

### template_literal.rs (~200 lines)
- [ ] `evaluate_template_literal` (line ~2201)
- [ ] `count_literal_members` (line ~2263)
- [ ] `extract_literal_strings` (line ~2293)

### keyof.rs (~200 lines)
- [ ] `recurse_keyof` (line ~2374)
- [ ] `evaluate_keyof` (line ~2380)
- [ ] `keyof_union` (line ~2507)
- [ ] `keyof_intersection` (line ~2520)
- [ ] `array_keyof_keys` (helper)
- [ ] `append_tuple_indices` (helper)
- [ ] `intersect_keyof_sets` (line ~2530)
- [ ] `KeyofKeySet` struct (line ~73)

### string_intrinsic.rs (~200 lines)
- [ ] `recurse_string_intrinsic` (line ~2530)
- [ ] `evaluate_string_intrinsic` (line ~2541)
- [ ] `apply_string_intrinsic_to_template_literal` (line ~2653)
- [ ] `apply_string_transform` (line ~2747)

### infer_pattern.rs (~1500 lines)
- [ ] `substitute_infer` (line ~2990)
- [ ] `type_contains_infer` (line ~2998)
- [ ] `type_contains_infer_inner` (line ~3003)
- [ ] `bind_infer` (line ~3228)
- [ ] `bind_infer_defaults` (line ~3253)
- [ ] `bind_infer_defaults_inner` (line ~3264)
- [ ] `match_signature_params` (line ~3733)
- [ ] `match_tuple_elements` (helper)
- [ ] `match_infer_pattern` (line ~3762) - This is the largest function
- [ ] `match_template_literal_string` (line ~5203)
- [ ] `match_template_literal_spans` (line ~5258)
- [ ] `match_template_literal_string_type` (line ~5315)
- [ ] `InferSubstitutor` struct and impl (line ~5344)

### apparent.rs (~150 lines)
- [ ] `apparent_literal_kind` (line ~2877)
- [ ] `apparent_primitive_shape_for_key` (line ~2886)
- [ ] `apparent_primitive_kind` (line ~2891)
- [ ] `apparent_primitive_shape` (line ~2912)
- [ ] `apparent_method_type` (line ~2955)
- [ ] `apparent_primitive_keyof` (line ~2974)
- [ ] `array_member_types` (helper)
- [ ] `array_member_kind` (helper)

## Public Constants

The following constants are public for use by modules:
- `ARRAY_METHODS_RETURN_ANY`
- `ARRAY_METHODS_RETURN_BOOLEAN`
- `ARRAY_METHODS_RETURN_NUMBER`
- `ARRAY_METHODS_RETURN_VOID`
- `ARRAY_METHODS_RETURN_STRING`

## Testing

After integration, run the full test suite:
```bash
cargo test
```

Pay special attention to:
- `src/solver/evaluate_tests.rs`
- Type evaluation in integration tests

## Benefits of This Structure

1. **Maintainability**: Each module focuses on one type of evaluation
2. **Testability**: Modules can be tested in isolation
3. **Documentation**: Each module is well-documented
4. **Code Navigation**: Easy to find relevant code by type category
5. **Reduced File Size**: `evaluate.rs` goes from ~5,800 lines to ~600 lines (coordinator only)

## Implementation Notes

- Follow the same pattern as `subtype_rules/` - extend `TypeEvaluator` via impl blocks
- Each module should have a clear, focused responsibility
- Use `super::super::evaluate::TypeEvaluator` to access the main struct
- Keep the main `evaluate()` method in `evaluate.rs` as the entry point
