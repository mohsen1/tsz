# fix(checker): preserve keyof T in property-receiver type-application display

- **Date**: 2026-04-26
- **Branch**: `fix/ts2551-property-receiver-preserves-keyof-arg-display`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (conformance fingerprint parity)

## Intent

`normalize_property_receiver_application_display_arg` was eagerly calling
`evaluate_type_with_env` on every type-argument node before recursing
structurally. For arguments containing generic operator types like
`keyof Shape`, evaluation expanded the operator into its evaluated structural
form (e.g. `keyof Record<string, string>` → `string | number`), erasing the
syntactic identity tsc preserves in property-receiver diagnostics. Restrict
the evaluation step to `Lazy(DefId)` references so the structural recursion
below handles applications/unions/intersections/objects directly while
keyof/index-access/conditional types stay intact in the printed message.

Fixes the `mappedTypeGenericWithKnownKeys.ts` fingerprint-only failure where
TS2551 currently reads
`Record<string | number, number>` instead of
`Record<keyof Shape | "knownLiteralKey", number>`.

## Files Touched

- `crates/tsz-checker/src/error_reporter/core/type_display.rs` (~14 LOC change)
- `crates/tsz-checker/tests/conformance_issues/errors/error_cases.rs` (+15 LOC test)

## Verification

- `cargo nextest run -p tsz-checker --lib` (2918/2918 pass)
- `cargo nextest run -p tsz-checker --test conformance_issues` (791/791 pass)
- `./scripts/conformance/conformance.sh run --filter "mappedTypeGenericWithKnownKeys"` (1/1 pass)
