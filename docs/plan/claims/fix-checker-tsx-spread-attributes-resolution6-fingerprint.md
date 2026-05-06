# fix(checker): align TSX spread attributes resolution6 fingerprint

- **Date**: 2026-05-06
- **Branch**: `fix/checker-tsx-spread-attributes-resolution6-fingerprint`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the fingerprint-only conformance drift in
`TypeScript/tests/cases/conformance/jsx/tsxSpreadAttributesResolution6.tsx`.
The error-code set already matches `tsc` (`TS2322`), so this slice will focus
on the diagnostic anchor or message rendering path for JSX spread attribute
assignability.

## Files Touched

- TBD

## Verification

- `cargo nextest run` for the owning crate test added with the fix.
- `./scripts/conformance/conformance.sh run --filter "tsxSpreadAttributesResolution6" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
