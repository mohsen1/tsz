# fix(checker): preserve literal tuples inferred from generic rest parameters

- **Owner**: Codex
- **Issue**: #6901
- **Workstream**: Conformance - generic rest parameter inference
- **Branch**: `codex/issue-6901-generic-rest-literal-tuple-20260514`
- **Status**: Ready for review

## Scope

Fix the TS2322 false positive where `function typed<T extends string[]>(...args: T): T` widens direct string literal rest arguments to `[string, string, string]` instead of inferring `["a", "b", "c"]`.

## Validation

- `cargo test -p tsz-checker --test generic_call_inference_tests generic_rest_parameter_infers_literal_tuple_under_primitive_array_constraint -- --nocapture`
- `cargo test -p tsz-checker --test generic_call_inference_tests variadic_tuple_spread_without_assertion_widens_to_primitives -- --nocapture`
