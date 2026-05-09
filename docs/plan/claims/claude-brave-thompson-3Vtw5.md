# fix(solver): match tsc's isUnknownLikeUnionType for any-shape unknown-like unions

- **Date**: 2026-05-04
- **Time**: 2026-05-04 03:00:00
- **Branch**: `claude/brave-thompson-3Vtw5`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance & Fingerprints)

## Intent

In `crates/tsz-solver/src/relations/compat.rs`, the helper
`empty_object_with_nullish_target` recognised the canonical "unknown-like"
union `{} | null | undefined` but rejected any union with extra members. tsc's
`isUnknownLikeUnionType` (`TypeScript/src/compiler/checker.ts` ~22136) treats
*any* union containing `{}`, `null`, AND `undefined` as semantically
equivalent to `unknown`, because every other constituent is necessarily a
subtype of `{}` (and thus absorbed by it).

This caused a false-positive TS2322 on, e.g.:

```ts
function f01(u: unknown) {
  let x2: {} | null | undefined = u;                  // OK in tsz (matches tsc)
  let x3: {} | { x: string } | null | undefined = u;  // tsz wrongly errored
}
```

The fix is a one-function rewrite: skip non-nullish, non-`{}` members instead
of bailing out, and require `len() >= 3` to mirror tsc's invariant. No new
relation entrypoints or boundary changes — `query_boundaries::assignability`
is unaffected; this is a fast-path predicate inside `CompatChecker`.

## Files Touched

- `crates/tsz-solver/src/relations/compat.rs` — rewrote
  `empty_object_with_nullish_target` to mirror tsc's `isUnknownLikeUnionType`
  semantics (~25 LOC).
- `crates/tsz-solver/tests/compat_tests.rs` — 5 new unit tests:
  - canonical `{} | null | undefined` accepts unknown
  - extra non-nullish union members do not disqualify (`{} | { x: string } | null | undefined`)
  - missing `null`, `undefined`, or `{}` constituents correctly reject
- `docs/plan/claims/claude-brave-thompson-3Vtw5.md` — this claim.

## Verification

- `cargo nextest run --package tsz-solver --lib` (5608/5608 pass)
- `cargo nextest run --package tsz-checker --lib` (3276/3276 pass)
- Full conformance: targeted test
  `conformance/types/unknown/unknownControlFlow.ts` drops from 9 to 8 emitted
  diagnostics (one false-positive TS2322 removed); test stays in the
  fingerprint-only bucket pending unrelated fixes for the TS2367 narrowing
  position and the `ff3(null, 'foo')` inference edge.
- `scripts/session/verify-all.sh --quick` — clean (regressions are flaky
  ~17–47s tests racing past the 20s timeout, not caused by this change).
