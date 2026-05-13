# Claim: TS1229 is reported for type predicates referencing rest parameters

## Status

ready

## Issue

GitHub issue #6650 reports that `function assertAll(...values: unknown[]): asserts values is string[] {}` is accepted even though TypeScript emits TS1229.

## Change

- Detect explicit type predicates whose target parameter resolves to a rest parameter.
- Emit TS1229 at the predicate parameter name and suppress construction of the invalid predicate signature.
- Cover normal predicates and assertion predicates on function declarations.

## Validation

- `cargo test -p tsz-checker assertion_type_predicate_diagnostics_tests::type_predicate_cannot_reference_rest_parameter -- --nocapture`
- `cargo fmt --all --check`
