# fix(narrowing): `===` against an anonymous object narrows `unknown` to that shape

- **Date**: 2026-05-09
- **Branch**: `fix/unknown-equality-narrows-to-anonymous-object-2026-05-09`
- **PR**: TBD (will draft as WIP)
- **Status**: claim
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

Without the second block, tsz narrows `u` correctly. The bug appears to
be flow-graph state shared across sibling `===`-narrowing blocks when
the comparand on one of them is itself a union. The fix needs to
isolate per-block narrowing state so a later block does not retro-
actively un-narrow `u` at an earlier `if`.

## Targeted tests

- `conformance/types/unknown/unknownType2.ts` (TS2322 false positive)

## Files Touched (planned)

- `crates/tsz-checker/src/...` (narrowing for `===` operator)
  OR `crates/tsz-solver/src/narrowing/...`
- New unit tests in the owning crate

## Verification

- `cargo nextest run -p tsz-checker --lib` clean
- `cargo nextest run -p tsz-solver --lib` clean
- `./scripts/conformance/conformance.sh run --filter unknownType2 --verbose` flips
- Snapshot regen `scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot` net-positive
