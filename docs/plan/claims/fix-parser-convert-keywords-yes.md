# [WIP] fix(parser): align convertKeywordsYes diagnostics

- **Date**: 2026-04-29
- **Branch**: `fix/parser-convert-keywords-yes`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

This PR targets the random conformance pick `convertKeywordsYes.ts`.
Current TSZ output misses `TS1213` and emits extra `TS1139`, `TS2300`, and
`TS2749` fingerprints compared with `tsc`. The slice will diagnose the root
cause in parser/checker keyword-conversion handling and land the smallest
architecture-aligned fix with an owning Rust regression test.

## Files Touched

- `docs/plan/claims/fix-parser-convert-keywords-yes.md` (claim)

## Verification

- Planned: `./scripts/conformance/conformance.sh run --filter "convertKeywordsYes" --verbose`
- Planned: owning-crate `cargo nextest run` for changed Rust code
