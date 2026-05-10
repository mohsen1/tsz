# fix(narrowing): `===` against an anonymous object narrows `unknown` to that shape

- **Date**: 2026-05-09
- **Branch**: `fix/unknown-equality-narrows-to-anonymous-object-2026-05-09`
- **PR**: #4886
- **Status**: ready for review
- **Workstream**: narrowing-flow (Tier 2 wrong-code campaign)

## Intent

`unknownType2.ts` exercises tsc's `===`-narrowing on `u: unknown` against
typed declarations. tsz emits a false-positive TS2322 inside the first
`if (u === anObjectLiteral)` block, but **only** when a subsequent
`if (u === aUnion)` block is also present in the same file. Minimal repro:

```ts
declare const anObjectLiteral: { x: number };
declare const aUnion: { x: number } | { y: string };
const u: unknown = undefined;
if (u === anObjectLiteral) {
    let uObjectLiteral: object = u;  // tsc: OK; tsz: TS2322
}
if (u === aUnion) {                  // <- presence of this block
    type x = typeof u;               //    breaks narrowing in the
}                                    //    EARLIER block above
```

Without the second block, tsz narrows `u` correctly. The narrowed
`unknown` path was missing the annotation-comparison fallback already
used by other flow-comparison paths, so object-literal typed comparands
could fail to produce the `object` narrowing type.

## Targeted tests

- `conformance/types/unknown/unknownType2.ts` (TS2322 false positive)

## Files Touched

- `crates/tsz-checker/src/flow/control_flow/condition_narrowing.rs`
  (use annotation comparison types for `unknown` equality comparands)
- `crates/tsz-checker/src/flow/control_flow/narrowing_helpers.rs`
  (map object-like const annotations to `object`)
- `crates/tsz-checker/tests/equality_narrow_unknown_to_const_intrinsic_tests.rs`
  (regression coverage)

## Verification

- `cargo test -p tsz-checker --test equality_narrow_unknown_to_const_intrinsic_tests unknown_type2_object_literal_repro_with_initialized_unknown -- --nocapture`
- `./scripts/conformance/conformance.sh run --filter unknownType2 --verbose --workers 1`
