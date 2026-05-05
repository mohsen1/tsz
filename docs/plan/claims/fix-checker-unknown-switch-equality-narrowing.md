---
name: Preserve unknown equality narrowing through switch cases
status: claimed
timestamp: 2026-05-05 00:00:00
branch: fix/checker-unknown-equality-narrowing-fingerprint
pr: 3013
---

# Claim

Workstream 1 (Diagnostic Conformance) - finish the remaining
`unknownType2.ts` fingerprint gaps around `unknown` equality narrowing in
switch cases and grouped return clauses.

## Problem

The checker had three stale-context paths for `unknown` equality flow:

1. Explicit `unknown` flow results were cached globally under broad flow
   nodes. Later broad comparisons, such as `u === aUnion`, could poison
   earlier primitive/object equality branches and make them display
   `unknown`.
2. Switch case expressions did not always have `node_types` entries when
   flow checked a clause body. Identifier cases such as `case symb`,
   `case y`, and `case z` therefore lost their comparand type.
3. Return diagnostics for grouped switch cases could receive the correct
   narrowed source type but render it back as `unknown`, and then in a
   non-tsc member order.

## Fix

- Skip the shared flow-analysis cache for explicit `TypeId::UNKNOWN`
  starting types, while retaining it for other declared types.
- Resolve switch comparison types through literals, flow-aware node types,
  the type environment, lazy symbols, and object/function-like annotations.
- Avoid marking stable symbol-flow shortcuts for declared `unknown`, because
  a straight-line `unknown` result is not proof that later equality flow is
  also unchanged.
- In `NoUnionMemberMatches` diagnostics, recover the grouped switch-case
  source display so the nonmatching case is printed first and the matching
  cases keep source order.

## Tests

- Extended `equality_narrow_unknown_to_const_intrinsic_tests` with:
  - explicit-unknown cache isolation after a broad union comparison;
  - switch `unknown` narrowing for unique-symbol and object/function cases;
  - grouped switch return TS2322 display for `"maybe" | "yes" | "no"`.

## Verification

- `cargo nextest run -p tsz-checker --test equality_narrow_unknown_to_const_intrinsic_tests`
  - 9 passed.
- `./scripts/conformance/conformance.sh run --filter "unknownType2" --verbose`
  - 1/1 passed.
- `./scripts/conformance/conformance.sh run --max 200`
  - 200/200 passed.
- `CARGO_INCREMENTAL=0 cargo nextest run -p tsz-checker --lib`
  - 3399 passed, 1 failed: existing `repro_parserreal::repro_parser_harness_type_ids`
    aborts with stack overflow when run alone as well.

## Conformance impact

`TypeScript/tests/cases/conformance/types/unknown/unknownType2.ts` now
matches tsc fingerprints for the selected case: the only expected TS2322
remains at the grouped `return x`, with source display
`"maybe" | "yes" | "no"`.
