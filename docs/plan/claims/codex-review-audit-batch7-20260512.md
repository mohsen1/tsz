# test(checker,emitter): review-audit batch7 follow-ups

- **Date**: 2026-05-12
- **Branch**: `codex/review-audit-batch7-20260512-170920`
- **PR**: #5919
- **Status**: ready
- **Workstream**: review audit follow-ups

## Intent

Close missed important review comments left on #5830.
Close missed important review comments left on #5743.
Close missed important review comments left on #5811.
Close missed important review comments left on #5753.
Close missed important review comments left on #5901.

This batch hardens regression tests to avoid false confidence (JSX/class validity and TS2786 anchoring, CommonJS export-equals declaration ordering) and fixes nested-union nullish parsing when deriving JS accessor setter types.

## Files Touched

- `crates/tsz-checker/tests/js_constructor_property_tests.rs`
- `crates/tsz-checker/tests/conditional_infer_tests.rs`
- `crates/tsz-checker/src/checkers/jsx/tests.rs`
- `crates/tsz-emitter/src/declaration_emitter/core/emit_members.rs`
- `crates/tsz-emitter/src/declaration_emitter/tests/simple_declarations.rs`
- `crates/tsz-wasm/src/wasm_tests.rs`
- `docs/plan/review-comment-audit-latest.json`
- `docs/plan/review-comment-audit-latest.md`

## Verification

- `cargo test -p tsz-checker --test js_constructor_property_tests -- --nocapture`
- `cargo test -p tsz-checker --test conditional_infer_tests conditional_keyof_pick_identity_assignable_to_type_parameter -- --nocapture`
- `cargo test -p tsz-wasm ts_program_accepts_nested_anonymous_object_literal_assignment -- --nocapture`
- `cargo test -p tsz-checker --lib jsx_union_component_with_invalid_return_emits_ts2786 -- --nocapture`
- `cargo test -p tsz-checker --lib jsx_union_component_all_valid_no_ts2786 -- --nocapture`
- `cargo test -p tsz-checker --lib jsx_user_named_component_type_alias_union_still_checks_returns -- --nocapture`
- `cargo test -p tsz-emitter --lib test_js_multiline_typedef_before_export_equals_function_variable_is_emitted -- --nocapture`
- `cargo test -p tsz-emitter --lib test_js_setter_does_not_lift_nested_nullish_from_array_element_union -- --nocapture`
- `cargo test -p tsz-emitter --lib test_js_reordered_accessor_comments_keep_backing_field_comment -- --nocapture`
- `cargo fmt --all --check`
