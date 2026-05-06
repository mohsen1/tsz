---
name: JSX generic children contextual TS2322 fingerprints
status: ready
timestamp: 2026-05-06 08:30:14
branch: fix/conformance-next-20260506-083014
---

# Claim

Workstream 1 (Diagnostic Conformance) follow-up for
`TypeScript/tests/cases/compiler/jsxChildrenGenericContextualTypes.tsx`.

## Scope

Align the remaining TS2322 fingerprints for JSX generic children contextual
typing after PR #2518. That merged PR fixed the single-callable body
elaboration case, but current `origin/main` still has two display/anchor gaps:

- JSX `children={p => "y"}` reports the whole function expression instead of
  elaborating at `"y"`.
- JSX body child `<ElemLit prop="x">{() => 12}</ElemLit>` anchors correctly
  but displays literal source `12` where tsc reports widened `number`.

## Verification Plan

- Focused Rust unit tests in the owning checker area.
- `cargo nextest run` for the affected checker tests.
- `./scripts/conformance/conformance.sh run --filter "jsxChildrenGenericContextualTypes" --verbose`

## Verification

- `cargo nextest run -p tsz-checker -E 'test(jsx_children_attribute_callback_with_mismatched_literal_return_elaborates_at_body) or test(jsx_zero_param_child_callback_with_mismatched_literal_return_widens_numeric_display) or test(jsx_single_callable_child_body_mismatch_elaborates_at_return) or test(jsx_union_children_single_child_emits_ts2322_without_return_type_elaboration)'`
- `./scripts/conformance/conformance.sh run --filter "jsxChildrenGenericContextualTypes" --verbose` -> 1/1 passed
