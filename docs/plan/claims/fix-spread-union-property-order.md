# fix(printer): preserve spread-result property declaration order

- **Date**: 2026-05-09
- **Branch**: `fix/spread-union-property-order-2026-05-09`
- **PR**: TBD (will draft as WIP)
- **Status**: claim
- **Workstream**: type-display-parity (Tier 1 fingerprint campaign)

## Intent

`spreadUnion2.ts` repros a TS2403 fingerprint where the displayed object
type has properties in REVERSE declaration order:

```ts
declare const undefinedUnion: { a: number } | undefined;
declare const nullUnion: { b: number } | null;

var o3: {} | { a: number } | { b: number } | { a: number, b: number };
var o3 = { ...undefinedUnion, ...nullUnion };
//        ^ TS2403
//   tsc: must be of type ... but here has type '{ a?: number | undefined; b?: number | undefined; }'.
//   tsz: must be of type ... but here has type '{ b?: number | undefined; a?: number | undefined; }'.
```

The spread `{ ...undefinedUnion, ...nullUnion }` should produce `{ a?, b? }`
(properties in spread order), but tsz outputs `{ b?, a? }`. Likely the
spread-evaluation path constructs the object with members in reverse
order.

## Targeted tests

- `conformance/types/spread/spreadUnion2.ts` (TS2403, single fingerprint diff)

## Files Touched (planned)

- `crates/tsz-checker/src/types/computation/object_literal/...` (spread eval)
- New unit test asserting property order matches spread order

## Investigation notes (2026-05-09)

`SPREAD_DISPLAY_ORDER_OFFSET = 1_000_000` decremented by `STRIDE = 10_000`
per spread, so later spreads sort BEFORE earlier ones (reverse). Flipping
to `saturating_add` correctly tags properties at finalize time
(verified via debug print: `b → 1_000_000`, `a → 1_010_000` for
`{ ...nullUnion, ...undefinedUnion }`).

However the printer still emits properties in alphabetic order: by the
time `format_object` runs, properties have `declaration_order = 1, 2`
(small alphabetic-tier numbers), not the 1M/1.01M spread tags. Some
intermediate widening or normalization path discards the spread tags
and re-assigns ordinals via `object_with_flags_and_symbol`'s `i + 1`
fallback. Three format_object calls all see `a=1, b=2` regardless of
spread direction.

Likely culprit: a widening or normalization step that constructs a
fresh PropertyInfo with `declaration_order = 0` (so the `i + 1`
fallback fires) instead of cloning the original. Needs deeper trace
to find the specific path.

## Verification

- `cargo nextest run -p tsz-checker --lib` clean
- `./scripts/conformance/conformance.sh run --filter spreadUnion2 --verbose` flips
- Snapshot regen net-positive
