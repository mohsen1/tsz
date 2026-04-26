# fix(solver): preserve union origin for anonymous-object member reorder

- **Date**: 2026-04-26
- **Branch**: `fix/union-origin-anon-object-order`
- **PR**: TBD
- **Status**: ready
- **Workstream**: conformance / type-display-parity

## Intent

When a type annotation declares a union of anonymous object members like
`{} | { a: number }`, the interner sorts members by `ShapeId` (allocation
order). For test fixtures where `{ a: number }` was interned earlier than
`{}` (e.g., from an earlier `declare const` in the same file), the
canonical sort produces `{ a: number; } | {}` — but tsc displays the
declared order verbatim.

`store_union_origin` previously gated origin storage behind a "flattening
occurred" check (resulting union strictly larger than input). For
anonymous-object unions of equal length, the origin was discarded and the
canonical sort showed through to diagnostics.

This PR extends the guard: when no flattening occurred but the union
contains an anonymous Object/ObjectWithIndex member and the resulting
order differs from the input, we still record the origin so the printer
can emit the source order in TS2403 / TS2322 / TS2345 messages.

Fingerprint impact verified on `spreadUnion2.ts`: 5 of 6 mismatched
diagnostic positions now match tsc.

## Files Touched

- `crates/tsz-solver/src/intern/core/interner.rs` (~50 LOC change)
- `crates/tsz-solver/src/diagnostics/format/tests.rs` (+~70 LOC, two new
  unit tests locking in the new behavior and the negative case for
  non-anonymous members)

## Verification

- `cargo nextest run -p tsz-solver --lib` (5516 tests pass)
- `cargo nextest run -p tsz-checker --lib` (2888 tests pass)
- Targeted conformance: `spreadUnion2.ts` reduces from 6 fingerprint
  mismatches to 1 (the remaining case is property-order in the spread
  result type, not union-member order — separate concern).
- Sample conformance run (500 tests): 497/500 pass — no regressions
  attributable to this change.
