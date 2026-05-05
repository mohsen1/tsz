# fix(checker): parenthesized contextual implicit-any diagnostic

- **Date**: 2026-05-05
- **Branch**: `fix/checker-parenthesized-contextual-implicit-any`
- **PR**: #3020
- **Status**: ready
- **Workstream**: conformance / diagnostic fingerprints

## Intent

Random conformance pick selected
`TypeScript/tests/cases/conformance/expressions/contextualTyping/parenthesizedContexualTyping2.ts`.
Claim-only PR #2938 recorded the WIP claim; implementation is tracked in
#3020.
The failure covered parenthesized conditional callbacks passed to overloaded
calls. The root causes were:

- `typeof x` in function type return annotations resolved the parameter as
  `any`, allowing callbacks that returned `undefined` to match `FuncType`.
- Overload mismatch recovery discarded diagnostics inside the mismatched
  conditional argument and kept later callback arguments contextually typed
  after an earlier argument had already failed.
- The recovered outer `TS2345` used the parenthesized wrapper and alias display
  instead of tsc's inner conditional anchor and expanded callable-union display.

## Files Touched

- `crates/tsz-lowering/src/lower/core.rs`
- `crates/tsz-lowering/src/lower/advanced.rs`
- `crates/tsz-solver/src/relations/subtype/rules/functions/checking.rs`
- `crates/tsz-checker/src/checkers/call_checker/overload_resolution.rs`
- `crates/tsz-checker/src/types/computation/call/inner.rs`
- `crates/tsz-checker/src/types/computation/call_result.rs`
- `crates/tsz-checker/src/error_reporter/call_errors/display_formatting.rs`
- `crates/tsz-checker/tests/contextual_typing_tests.rs`

## Verification

- Baseline target command:
  `./scripts/conformance/conformance.sh run --filter "parenthesizedContexualTyping2" --verbose`
- `cargo nextest run -p tsz-checker --test contextual_typing_tests function_type_return_type_query_resolves_parameter_type test_parenthesized_conditional_callbacks_preserve_contextual_typing --no-fail-fast`
  - Result: 2/2 passed.
- `./scripts/conformance/conformance.sh run --filter "parenthesizedContexualTyping2" --verbose`
  - Result: 1/1 passed (100.0%), no fingerprint-only failures.
