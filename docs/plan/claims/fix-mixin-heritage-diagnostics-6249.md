# Fix mixin heritage diagnostics for computed class bases

- **Date**: 2026-05-13
- **Branch**: `fix-mixin-heritage-diagnostics-6249`
- **PR**: TBD
- **Status**: claim
- **Workstream**: checker correctness / public issue #6249

## Intent

Close #6249 by restoring TypeScript-compatible diagnostics for classes that
extend a mixin function result. The initial slice will add focused regression
coverage for the reported TS2417/TS2510/TS2339 gaps, then fix the smallest
checker root cause needed without weakening the existing TS2545 behavior.

## Files Touched

- `docs/plan/claims/fix-mixin-heritage-diagnostics-6249.md`
- Checker class heritage or call-expression base handling files TBD after the
  regression localizes the failure.
- Focused checker regression test file TBD.

## Verification

- Pending.
