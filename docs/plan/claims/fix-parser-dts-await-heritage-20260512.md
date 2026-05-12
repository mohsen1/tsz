# fix(parser): respect virtual .d.ts filenames for await heritage clauses

- **Date**: 2026-05-12
- **Branch**: `fix/parser-dts-await-heritage-20260512`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance and fingerprints)

## Intent

Fix the `topLevelAwait.3.ts` conformance false positive where TSZ emits an
extra TS1109 for `declare class C extends await {}` even though the test's
virtual file is `index.d.ts`. The parser already suppresses this diagnostic
when the parser filename is `.d.ts`; this slice will route the conformance/CLI
path through the virtual declaration filename so the parser and tsc agree.

## Files Touched

- `docs/plan/claims/fix-parser-dts-await-heritage-20260512.md`

## Verification

- Pending
