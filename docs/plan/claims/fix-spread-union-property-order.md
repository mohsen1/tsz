# fix(printer): preserve spread-result property declaration order

- **Date**: 2026-05-09
- **Branch**: `fix/spread-union-property-order-2026-05-09`
- **PR**: #4891
- **Status**: ready for review
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

## Files Touched

- `crates/tsz-checker/src/types/computation/object_literal/...` (spread eval)
- `crates/tsz-solver/src/types.rs` (display-order-sensitive interning for spread objects)

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
shape's stored properties.

### Attempted partial fix (2026-05-09)

Adding `store_display_properties(type_id, properties_in_spread_order)`
after `factory().object(properties)` in the non-union `has_spread`
branch made `{ ...y, ...x }` (where x={a}, y={b}) emit `{ b, a }` —
but only for the non-union case. **The union-spread branches at
computation.rs:2696-2722 build types via a separate code path** (one
`factory().object(branch_props)` per cross-product branch) and do NOT
go through the same store_display_properties wiring. So
`{ ...undefinedUnion, ...nullUnion }` (where each spread is itself a
union containing an object) emits the same `{ b?, a? }` regardless
of spread order — the fix needs to be applied to the union-branch
loop too.

### Concrete next steps

1. Apply the `store_display_properties` call inside the inner loop at
   `computation.rs:2696-2722` (after each `factory().object(branch_props)`).
2. Verify both non-union spread (`{...x, ...y}`) and union spread
   (`{...optionalA, ...optionalB}`) cases respect spread order in
   the diagnostic display.
3. Run `cargo nextest run -p tsz-checker --lib` to catch any places
   that depend on alphabetic display order.

## Verification

- `cargo fmt --all --check`
- `./scripts/conformance/conformance.sh run --filter spreadUnion2 --test-dir /Users/mohsen/.codex/worktrees/parser-scanner-conformance-low-risk-20260508/TypeScript/tests/cases --verbose`
