# Implementation Claim: JSX multiple function children diagnostics

Status: claim

Branch: `fix/jsx-multiple-function-children-diagnostics`

Roadmap item: Workstream 1 — Diagnostic Conformance And Fingerprints

Scope:
- Investigate `checkJsxChildrenProperty4.tsx`, where tsz reports TS2746 for multiple JSX function children but tsc reports TS2322 per function child.
- Fix the root diagnostic selection so multiple children whose contextual child type is not array-like/ReactNode are assigned/checkable individually instead of collapsed to TS2746.
- Add one focused regression test covering the conformance shape.

Verification plan:
- Targeted checker regression test.
- `./scripts/conformance/conformance.sh run --filter "checkJsxChildrenProperty4" --verbose`.
