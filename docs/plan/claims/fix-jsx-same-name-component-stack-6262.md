# Fix JSX same-name component/interface stack overflow

- **Date**: 2026-05-13
- **Branch**: `fix-jsx-same-name-component-stack-6262`
- **PR**: TBD
- **Status**: claim
- **Workstream**: checker crash / urgent public issue #6262

## Intent

Close #6262 by preventing stack overflow when a JSX function component shares
its name with its props interface. The slice will add a crash regression for the
reported TSX pattern, then fix the narrow recursion path in JSX/component type
resolution without changing normal JSX prop checking.

## Files Touched

- `docs/plan/claims/fix-jsx-same-name-component-stack-6262.md`
- JSX/checker files TBD after localizing the recursion.
- Focused checker regression test TBD.

## Verification

- Pending.
