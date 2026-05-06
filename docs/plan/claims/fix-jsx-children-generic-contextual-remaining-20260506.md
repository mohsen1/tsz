---
name: JSX generic children contextual TS2322 fingerprints
status: claim
timestamp: 2026-05-06 08:30:14
branch: fix/conformance-next-20260506-083014
---

# Claim

Workstream 1 (Diagnostic Conformance) follow-up for
`TypeScript/tests/cases/compiler/jsxChildrenGenericContextualTypes.tsx`.

## Scope

Align the remaining TS2322 fingerprints for JSX generic children contextual
typing after PR #2518. That merged PR fixed the line-21 single-callable body
elaboration case, but current `origin/main` still has two display/anchor gaps:

- JSX body child `<ElemLit prop="x">{p => "y"}</ElemLit>` reports the whole
  function expression instead of elaborating at `"y"`.
- JSX body child `<ElemLit prop="x">{() => 12}</ElemLit>` anchors correctly
  but displays literal source `12` where tsc reports widened `number`.

## Verification Plan

- Focused Rust unit tests in the owning checker area.
- `cargo nextest run` for the affected checker tests.
- `./scripts/conformance/conformance.sh run --filter "jsxChildrenGenericContextualTypes" --verbose`

