# fix(solver): match tsc's logical-or union display order

- **Date**: 2026-05-09
- **Branch**: `fix/logical-or-union-display-order-2026-05-09`
- **PR**: TBD (will draft as WIP)
- **Status**: claim
- **Workstream**: type-display-parity (Tier 1 fingerprint campaign)

## Intent

`crates/tsz-solver/src/operations/binary_ops.rs:888` records the union-origin
for `||` results as `vec![right, truthy_left]`. tsc, however, displays the
truthy-narrowed left operand first, then the right operand:

```ts
function foo(options?: { a: string; b: number }) {
    (options || {}).a;  // Property 'a' does not exist on type
                        //   tsc: '{ a: string; b: number; } | {}'
                        //   tsz: '{} | { a: string; b: number; }'
}
```

The current order was introduced by commit `a8c0ea158e` ("fix(solver):
preserve logical-or diagnostic display order"). The structural rule should
be `[truthy_left, right]` to match tsc's TS2339/TS2353 display.

## Targeted tests

- `conformance/expressions/propertyAccess/propertyAccessWidening.ts` (TS2339)
- (others will be discovered during snapshot regen)

## Files Touched

- `crates/tsz-solver/src/operations/binary_ops.rs` (1 line change)
- New unit test in `crates/tsz-solver/tests/binary_ops_*` to lock the order
- Refreshed snapshot files

## Verification

- `cargo nextest run -p tsz-solver --lib` clean
- `cargo nextest run -p tsz-checker --lib` clean
- `./scripts/conformance/conformance.sh run --filter propertyAccessWidening --verbose` flips
- Existing `logical_or_type_parameter_assignment_reports_whole_expression`
  accepts EITHER order, so flipping won't break it
- Snapshot regen net-positive
