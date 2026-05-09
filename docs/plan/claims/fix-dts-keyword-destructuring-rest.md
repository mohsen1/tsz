# fix(checker): omit keyword destructuring keys from rest types

## Claim

Object rest bindings now omit destructured keyword property names from inferred rest return types in declaration emit.

## Evidence

- `cargo check --package tsz-checker --package tsz-emitter --package tsz-cli`
- `cargo test --package tsz-emitter test_object_rest_with_keyword_property_names_omits_destructured_key --lib`
- `cargo test --package tsz-cli declaration_emit_keyword_destructuring_rest_omits_keyword_key --test tsc_compat_tests`
