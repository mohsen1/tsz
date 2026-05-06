# fix(checker): report find callback implicit this

- **Date**: 2026-05-06
- **Branch**: `fix/this-in-function-call-regression-20260506-193529`
- **PR**: TBD
- **Status**: implemented
- **Workstream**: 1 (Conformance fixes)

## Intent

Target `TypeScript/tests/cases/compiler/thisInFunctionCall.ts`.
The canonical picker reports a fingerprint-only TS2683 mismatch. This slice
will identify the current source/span/message difference for implicit `this`
diagnostics in function-call contexts and realign checker diagnostic rendering
without changing the diagnostic code set.

Root cause: `Array.prototype.find` callbacks are typed through a contextual
predicate path that can skip the usual callback body TS2683 emission. The
checker needs a source-context pass at the `find(...)` call boundary so callback
`this` references still report noImplicitThis diagnostics, while JS `@this`
continues to suppress the diagnostic.

## Files Touched

- `crates/tsz-checker/src/dispatch.rs`
- `crates/tsz-checker/src/symbols/scope_finder.rs`
- `crates/tsz-checker/src/state/state_checking_members/function_declaration_checks.rs`
- `crates/tsz-checker/tests/conformance_issues/core/helpers.rs`

## Verification

- `cargo fmt --check`
- `CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=.target/nextest-local cargo nextest run -p tsz-checker --test conformance_issues -E 'test(this_in_array_find_callback_emits_ts2683) | test(this_in_js_array_find_callback_emits_ts2683_without_this_jsdoc) | test(contextual_generic_callback_this_survives_ts2454_receiver_reads)'`
- `./scripts/conformance/conformance.sh run --filter "thisInFunctionCall" --verbose`
