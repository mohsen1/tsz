# fix(checker): emit TS1239 for parameter decorator signature mismatch

- **Date**: 2026-05-12
- **Branch**: `fix/checker-parameter-decorator-ts1239-2026-05-12`
- **Base**: `main`
- **Issue**: [#5890](https://github.com/mohsen1/tsz/issues/5890)
- **Status**: claim
- **Labels**: `bug`, `missing-diagnostic`

## Intent

Closes #5890. tsz currently emits no diagnostic when a parameter
decorator's signature is incompatible with `(target: Object, key:
string, parameterIndex: number) => void`. tsc emits TS1239:

> Unable to resolve signature of parameter decorator when called as
> an expression.

## Approach

TS1238 (class decorator) and TS1240 (ES field decorator) and the
TS1241 method-decorator arity check already exist in
`crates/tsz-checker/src/state/state_checking_members/decorator_signature_checks.rs`.
The missing case is parameter decorators.

The structural rule:

> When `experimentalDecorators` is on and a parameter-position
> decorator's resolved signature does not accept the runtime
> calling convention `(target: Object, key: string | symbol,
> parameterIndex: number) => void`, the checker emits TS1239 at the
> decorator expression's anchor.

Parallel to:
- `check_method_decorator_arity` (TS1241) for method decorators.
- `check_class_decorator_call_signature` (TS1238) for class decorators.
- `check_es_property_decorator_call_signature` (TS1240) for ES fields.

## Files Touched (estimated)

- `crates/tsz-checker/src/state/state_checking_members/decorator_signature_checks.rs`
  — add `check_parameter_decorator_call_signature` modeled on the
  existing methods (~50 LOC additive).
- The walker / dispatch site that calls the method-decorator check
  also needs a sibling call for parameter decorators (TBD: which
  file walks parameter nodes).
- New test in `crates/tsz-checker/tests/ts1239_parameter_decorator_tests.rs`
  with two name-choice cases per CLAUDE.md §25.

## Out of scope

- Stage-3 / ES decorator semantics (issue is for
  `experimentalDecorators`).
- Validating other decorator kinds (TS1238/1240/1241 are already
  handled).

## Verification

- Unit test that locks the new TS1239 emission on the issue's
  exact repro + a renamed variant.
- Regression: ensure `cargo nextest run -p tsz-checker --lib` stays
  green and the related ts1238/1240/1241 tests don't regress.
- Targeted conformance smoke: any TS1239-expecting baseline test
  should now pass.

## Risk

Low. Pure additive diagnostic; no semantic change. Worst case is
a false positive that suppresses valid code — the test pair (the
issue's repro + a structural variant) guards against
identifier-spelling-sensitive bugs.
