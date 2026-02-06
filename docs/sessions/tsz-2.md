# Fix unite tests

Run the following command:

```
cargo nextest run --partition count:2/4
```

Your task is to make this pass 100%.

Once done, look into ignored tests in this batch.

While working towards making all tests pass, you can use `--no-verify` to make atomic and meaningful commits

## Session logs

### 2025-02-06 (Current Session)

#### Fixed Tests:
1. ✅ **test_import_alias_non_exported_member** - Fixed TS2694 error for import aliases of non-exported namespace members
   - Modified `state_type_analysis.rs` to emit error when importing non-exported namespace members
   - Added check after `resolve_qualified_symbol` returns None to detect missing exports
   - Uses `report_type_query_missing_member` to emit TS2694

#### Partial Progress:
2. ⚠️ **test_static_private_field_access_no_ts2339** - Stack overflow issue (IN PROGRESS)
   - Added DefId-based identity check in `evaluate_type_with_resolution` (state_type_environment.rs)
   - The fix compares def_id to detect circular Lazy type references
   - Issue persists - recursion happening through a different code path
   - Need deeper investigation with tracing to identify exact recursion cycle

#### Remaining Failing Tests in Partition 2/4:
- test_indexed_access_resolves_class_property_type
- test_overload_call_handles_tuple_spread_params
- test_readonly_method_signature_assignment_2540
- test_ts2339_computed_name_this_in_class_expression
- test_use_before_assignment_try_catch
- compile_function_call_spread
- compile_generic_utility_library_type_utilities
- test_instantiate_mapped_type_shadowed_param
- test_instantiate_template_literal_in_mapped_type_template

#### Recommendations from Gemini:
Based on NORTH_STAR.md, priority should be:
1. Fix stack overflow first (compiler crash is highest priority)
2. Then focus on core type system tests (mapped types, template literals)
3. Finally address CFA and assignment logic tests

#### Commits:
- f477ad176 feat(checker): add DefId-based identity check for Lazy type resolution
- 7c303cecf feat(checker): emit TS2694 for import alias of non-exported namespace member
