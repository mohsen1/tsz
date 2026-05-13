# Shared Checker-Test Diagnostic Code/Message Helper

Issue: https://github.com/mohsen1/tsz/issues/6349
Branch: `codex/cleanup-diagnostic-code-messages-helper-20260513`
Status: Ready

## Scope

Add a shared diagnostic `(code, message_text)` projection helper to
`crates/tsz-checker/src/test_utils.rs` and migrate a larger cluster of
checker display tests that currently hand-roll the same projection locally.

This is behavior-preserving test cleanup. It does not change roadmap metrics,
compiler behavior, or implementation direction.

## Verification

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --test ts2322_literal_source_display_tests --test ts2345_instanceof_narrowed_union_display_tests --test ts2322_undefined_null_target_literal_display_tests --test ts2344_literal_type_message --test ts2344_widen_literal_arg_display_tests --test ts2344_accessor_constraint --test ts2344_class_constructor_constraint --test ts2344_self_referential_interface_constraint --test ts2344_typeof_merged_tests --test ts2344_infer_conditional_constraint --test ts2344_generic_ref_scoped_param_concrete_constraint --test ts2340_super_accessor_tests --test jsdoc_class_property_target_display --test jsdoc_constructor_typeof_source_display_tests --test jsdoc_augments_empty_tests --test jsdoc_extends_constraint_tests --test jsdoc_class_template_arena_direct_tests --no-fail-fast`
- `cargo clippy --profile ci-lint -p tsz-checker --all-targets -- -D warnings`
- Full PR CI after marking ready.
