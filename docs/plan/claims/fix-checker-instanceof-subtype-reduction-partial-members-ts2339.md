# fix(checker): preserve instanceof union branch for partial member diagnostics

- **Date**: 2026-05-05
- **Branch**: `fix/checker-instanceof-subtype-reduction-partial-members-ts2339`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Close the fingerprint-only `typeGuardsWithInstanceOf` slice where `tsz`
reports the shared `TS2339` code but misses the two `v.onChanges` property
access diagnostics after `if (v instanceof C)`. `tsc` keeps the post-guard type
as `C | (Validator & Partial<OnChanges>)`, so accessing `onChanges` must report
against the `C` branch even though the original variable was subtype-reduced
back toward `Validator & Partial<OnChanges>`.

## Files Touched

- TBD

## Verification

- Planned: `cargo check --package tsz-checker`
- Planned: focused checker regression test
- Planned: `./scripts/conformance/conformance.sh run --filter "typeGuardsWithInstanceOf" --verbose`

`cargo nextest` is not installed in this environment; use targeted `cargo test`
commands for local verification.
