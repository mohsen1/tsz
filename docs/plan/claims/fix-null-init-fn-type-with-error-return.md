# fix(checker): report nullish assignment through nested error targets

Status: ready

Owner: Codex
Branch: `fix/null-init-fn-type-with-error-return`
Created: 2026-05-04 17:41:17 UTC

## Intent

Restore TS2322 cascades when a literal `null` / `undefined` value is assigned
to a non-nullish declared target whose nested structure contains an unresolved
type, matching `parserRealSource6.ts`.

## Planned Scope

- `crates/tsz-checker/src/assignability/assignability_diagnostics.rs`
- `crates/tsz-checker/src/assignability/nullish_error_targets.rs`
- `crates/tsz-checker/src/state/state_checking_members/ambient_signature_checks.rs`
- `crates/tsz-checker/src/types/type_checking/core_statement_checks.rs`
- `crates/tsz-checker/tests/ts2322_property_decl_annotation_tests.rs`

## Verification Plan

- `cargo nextest run -p tsz-checker --test ts2322_property_decl_annotation_tests`
- `./scripts/conformance/conformance.sh run --filter "parserRealSource6" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
