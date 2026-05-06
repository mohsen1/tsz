# fix(checker): align TSX spread attributes resolution6 fingerprint

- **Date**: 2026-05-06
- **Branch**: `fix/checker-tsx-spread-attributes-resolution6-fingerprint`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fixes the fingerprint-only conformance drift in
`TypeScript/tests/cases/conformance/jsx/tsxSpreadAttributesResolution6.tsx`.
The error-code set already matched `tsc` (`TS2322`), but the JSX union-props
diagnostic rendered the component constructor as the source and expanded the
class props union instead of using the synthesized attributes object and JSX
component target display.

## Files Touched

- `crates/tsz-checker/src/checkers/jsx/diagnostics.rs`
- `crates/tsz-checker/src/checkers/jsx/props/resolution.rs`
- `crates/tsz-checker/src/checkers/jsx/tests.rs`

## Verification

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --lib jsx_class_union_props_ts2322_uses_attrs_source_display`
- `./scripts/conformance/conformance.sh run --filter "tsxSpreadAttributesResolution6" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
