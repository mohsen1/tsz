# fix(emitter): emit callable expando functions as methods

Status: ready
Owner: Codex
Branch: `fix-dts-callable-expando-method-syntax`
Created: 2026-05-05 09:06:53 UTC

## Intent

Close the `declarationEmitExpandoWithGenericConstraint` declaration emit gap by
printing function-valued late-bound expando properties on callable exports with
method syntax.

## Planned Scope

- `crates/tsz-emitter/src/declaration_emitter/helpers/function_analysis.rs`
- `crates/tsz-emitter/src/declaration_emitter/tests/simple_declarations.rs`
- `docs/plan/claims/fix-dts-callable-expando-method-syntax.md`

## Verification Plan

- `cargo fmt --package tsz-emitter -- --check`
- `cargo check --package tsz-emitter`
- `cargo test --package tsz-emitter test_callable_export_expando_function_property_emits_method_signature --lib`
- `cargo test --package tsz-emitter test_ts_late_bound_arrow_assignments_preserve_key_text_and_types --lib`
- `./scripts/emit/run.sh --dts-only --filter=declarationEmitExpandoWithGenericConstraint --verbose --concurrency=1 --timeout=30000`
