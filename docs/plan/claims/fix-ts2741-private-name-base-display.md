# fix(checker): display private-name missing property source base

- **Date**: 2026-05-05
- **Branch**: `fix-ts2741-private-name-base-display`
- **PR**: #2780
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the refreshed random conformance pick `privateNamesUnique-4.ts`, where
`tsc` and `tsz` both emit TS2564 and TS2741 but disagree on the TS2741
fingerprint. For assignment `const c: C = a` where `A2 extends A1`, tsc reports
private `#something` missing in source type `A1`; tsz currently reports `A2`.

## Files Touched

- `docs/plan/claims/fix-ts2741-private-name-base-display.md`
- `crates/tsz-checker/src/error_reporter/render_failure.rs`
- `crates/tsz-checker/tests/private_brands.rs`

## Verification

- `cargo nextest run -p tsz-checker --test private_brands private_name_missing_property_source_display_uses_base_class` - passed.
- `cargo nextest run -p tsz-checker --test private_brands` - passed, 28/28.
- `cargo check -p tsz-checker` - passed.
- `./scripts/conformance/conformance.sh run --filter "privateNamesUnique-4" --verbose` - passed, 1/1.
- `./scripts/conformance/conformance.sh run --max 200` - passed, 200/200.
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>&1 | grep -E "FINAL RESULTS|Fingerprint-only|Known failures|Crashed|Timeout|passed"` - passed, 12,444/12,582 (98.9%), fingerprint-only 95.
