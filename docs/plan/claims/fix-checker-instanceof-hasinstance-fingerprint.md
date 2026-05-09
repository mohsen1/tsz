# [WIP] fix(checker): align instanceof Symbol.hasInstance fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-instanceof-hasinstance-fingerprint`
- **PR**: https://github.com/mohsen1/tsz/pull/2755
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the fingerprint-only divergence in
`TypeScript/tests/cases/conformance/expressions/typeGuards/typeGuardsWithInstanceOfBySymbolHasInstance.ts`.
The picked test already emits the same diagnostic codes as `tsc`
(`TS2322`, `TS2339`, `TS2551`), so this PR will root-cause the remaining
message, display, count, or anchor mismatch around `instanceof` narrowing with
`Symbol.hasInstance`.

## Files Touched

- `docs/plan/claims/fix-checker-instanceof-hasinstance-fingerprint.md`
  (claim and verification record)
- `crates/tsz-solver/src/type_queries/flow.rs`
  (erase generic construct/predicate instance types to `any`)
- `crates/tsz-checker/src/flow/control_flow/core.rs`
  (preserve narrowing through property-assignment flow chains)
- `crates/tsz-checker/src/types/property_access_type/helpers.rs`
  (flow-narrow assignment receiver lookups)
- `crates/tsz-checker/src/types/property_access_type/resolve.rs`
  (use narrowed write receiver for missing-property diagnostics)
- `crates/tsz-checker/src/error_reporter/properties.rs`
  (avoid source annotation display when flow has narrowed from union/any)
- `crates/tsz-checker/tests/control_flow_type_guard_tests.rs`
  (regression for generic `[Symbol.hasInstance]` and `any` narrowing)

## Verification

- `cargo fmt --all`
- `cargo nextest run -p tsz-solver instance_type_from_constructor_erases_generic_construct_return_to_any instance_type_from_symbol_has_instance_erases_generic_predicate_to_any instance_type_from_constructor_uses_generic_construct_when_predicate_collapses_to_any`
- `cargo nextest run -p tsz-checker instanceof_symbol_hasinstance_generic_predicate_erases_to_any`
- `./scripts/conformance/conformance.sh run --filter "typeGuardsWithInstanceOfBySymbolHasInstance" --verbose`
- `cargo check --package tsz-checker`
- `cargo check --package tsz-solver`
- `./scripts/conformance/conformance.sh run --max 200`
