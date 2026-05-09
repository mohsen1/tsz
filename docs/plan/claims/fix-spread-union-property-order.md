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
time `format_object` runs, properties have `declaration_order = 1, 2`.

**Root cause** (found): `PropertyInfo`'s `Hash` and `PartialEq` impls in
`crates/tsz-solver/src/types.rs:1066-1095` deliberately *exclude*
`declaration_order` (so structurally-identical shapes intern to the same
TypeId). Consequence: when the spread result `{ a: number, b: number }`
is interned, the interner returns an *existing* shape that was first
seen via the type annotation `{ a: number, b: number }` (declaration_order
1, 2 from source). The spread's 1M / 1.01M tags are dropped on the floor.

## Proposed fix (next agent)

Store the spread-result property order as a side-table keyed on TypeId,
similar to the existing `display_properties` mechanism for fresh object
literals. Wire `format_object` to consult it before falling back to the
shape's stored properties. Specifically:

- After `factory().object(properties)` in `finalize_object_literal_type`'s
  `has_spread` branch, also call
  `interner.store_display_properties(type_id, properties_in_spread_order)`.
- The existing `format_object` code path at
  `crates/tsz-solver/src/diagnostics/format/mod.rs:1404-1408` already
  uses `display_properties` when `use_display_properties` is true; that
  flag already fires for diagnostic-mode formatters.

Caveat: `display_properties` is currently FRESH_LITERAL-only by design;
adding spread-result usage may need to gate with a separate flag to
avoid affecting unrelated formatters.

## Verification

- `cargo nextest run -p tsz-checker --lib` clean
- `./scripts/conformance/conformance.sh run --filter spreadUnion2 --verbose` flips
- Snapshot regen net-positive
