# fix(resolver): suppress bundler TS extension fallback TS2307

- **Date**: 2026-05-06
- **Branch**: `fix/bundler-import-ts-extensions-extra-ts2307-20260506-151625`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Conformance fixes)

## Intent

Target `TypeScript/tests/cases/conformance/moduleResolution/bundler/bundlerImportTsExtensions.ts`.
tsz currently emits one extra TS2307 while tsc only reports the bundler/extension
diagnostics TS2846, TS5024, TS5097, and TS6142. This PR will identify the
module-resolution or import-diagnostic path that lets an unresolved extension
candidate fall through to TS2307.

## Files Touched

- TBD after investigation.

## Verification

- Focused Rust regression in the owning resolver/checker area.
- `./scripts/conformance/conformance.sh run --filter "bundlerImportTsExtensions" --verbose`.
