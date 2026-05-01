# Implementation Claim: JSX ReactChild alias display in TS2322 fingerprints

**2026-04-29 12:00:00**

Status: shipped (PR #1818 — `fix(checker): preserve ReactChild alias in JSX TS2322 target-type messages`, merged 2026-04-28)

Branch: `claude/exciting-keller-nJTAE`

Roadmap item: Workstream 1 — Diagnostic Conformance And Fingerprints

Scope:
- `checkJsxChildrenProperty4.tsx` is fingerprint-only: codes match (TS2322+TS2551) but
  the TS2322 target type display differs.
  - tsc: `'boolean | any[] | ReactChild'` (ReactChild alias preserved)
  - tsz: `'string | number | boolean | any[] | Element'` (ReactChild expanded to constituents)
- Root cause: `jsx_multiple_children_element_type_without_empty_object` recursively processes
  `ReactNode`'s union members. When it encounters `ReactChild` (a Lazy alias), it evaluates
  and recurses into its constituent union `ReactElement<any> | string | number`, contributing
  individual members instead of the unified `ReactChild` alias TypeId.
- Fix: add a Lazy alias guard at the top of the function. When the input is a named Lazy alias
  whose evaluated body is a union without arrays or empty-object members (i.e., a "leaf" alias
  like ReactChild), return the original Lazy TypeId unchanged so the formatter displays the
  alias name.

Verification plan:
- Unit test in `tsz-checker` locking the invariant.
- `./scripts/conformance/conformance.sh run --filter "checkJsxChildrenProperty4" --verbose` → PASS.
- No regression on other JSX conformance tests.
