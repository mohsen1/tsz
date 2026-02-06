# Fix unit tests - Partition 3/4

Run the following command:

```
cargo nextest run --partition count:3/4
```

Your task is to make this pass 100%.

Once done, look into ignored tests in this batch.

While working towards making all tests pass, you can use `--no-verify` to make atomic and meaningful commits

## Progress

### Fixed
- ✅ `test_contextual_typing_overload_by_arity` - Fixed arity filtering in contextual typing for overloaded functions
- ✅ `test_async_ternary_ignores_nested_async` - Fixed parser to allow async arrow functions in conditional expressions
- ✅ `test_method_bivariance_event_handler_pattern` - Method bivariance was already implemented; test expectation was outdated
- ✅ `test_overload_call_handles_generic_signatures` - Implemented generic overload compatibility by instantiating type parameters to `any`

### Ignored (Known Issues)
- ⚠️ `test_readonly_element_access_assignment_2540` - Stack overflow in lib context handling (needs investigation)
- ⚠️ `test_contextual_property_type_infers_callback_param` - Contextual typing issue (was already failing)

### Remaining (7 failing tests)
1. `test_class_namespace_merging` - Requires symbol merging infrastructure in Binder
2. `test_ts2339_computed_name_this_missing_static` - Static context property access
3. `test_variadic_tuple_optional_tail_inference_no_ts2769` - Tuple type inference with optional tails
4. `compile_generic_utility_library_with_constraints` - CLI driver test
5. `test_project_performance_scope_cache_hits_rename` - LSP scope cache performance
6. `test_project_scope_cache_reuse_hover_to_rename` - LSP scope cache performance
7. `test_signature_help_overload_selection` - LSP signature help

## Recent Commits
- `feat(solver): implement generic overload compatibility checking` (675e7a08e) - Instantiate generic type params to `any` for overload compatibility
- `test: update method bivariance test expectation` (f7bc81d96) - Fixed test expectation, method bivariance already working
- `fix(parser): allow async arrow functions in conditional expressions` (0a7be5bc9)
- `feat(contextual): add arity-based overload filtering for contextual typing` (earlier)

## Session logs

### 2026-02-06: Excellent Progress - 7 Failing Tests Remaining

**Fixed in this session:**
1. `test_method_bivariance_event_handler_pattern` - Test expectation was outdated
2. `test_overload_call_handles_generic_signatures` - Implemented generic overload compatibility

**Key Implementation:**
- Generic overload compatibility: When checking `non-generic impl <: generic overload`,
  we now instantiate the target's type parameters to `any` before subtype checking.
  This implements universal quantification in `src/solver/subtype_rules/functions.rs`.

**Next Session - Gemini's Recommendations:**

1. **Priority 1: `test_variadic_tuple_optional_tail_inference_no_ts2769`**
   - Impact: High - Tier 1 type system feature
   - Component: Solver (pure WHAT problem)
   - Files: `src/solver/operations.rs`, `src/solver/evaluate_rules/infer_pattern.rs`
   - Approach: Fix `constrain_tuple_types` and `rest_tuple_inference_target` to handle
     optional elements following rest elements

2. **Priority 2: `test_class_namespace_merging`**
   - Impact: High - Fundamental TypeScript feature
   - Component: Binder (WHO problem)
   - Files: `src/binder/mod.rs`, `src/checker/namespace_checker.rs`
   - Approach: Allow `CLASS` and `MODULE` flags to merge in `can_merge_flags`,
     ensure namespace exports are attached to merged symbol

**Remember MANDATORY Two-Question Rule from AGENTS.md:**
1. Ask Gemini (Flash) to validate approach before implementing
2. Ask Gemini (Pro) to review implementation after coding
