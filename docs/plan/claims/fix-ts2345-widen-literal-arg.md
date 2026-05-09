# fix(checker): widen literal argument display in TS2345 against primitive target

- **Date**: 2026-05-09
- **Branch**: `fix/ts2345-widen-literal-arg-2026-05-09`
- **PR**: TBD (will draft as WIP)
- **Status**: claim
- **Workstream**: type-display-parity (Tier 1 fingerprint campaign)

## Intent

`unionTypeInference.ts` repros tsc's literal-widening rule for TS2345
arg displays. When the parameter type is a primitive base (`string`,
`number`, `boolean`, `bigint`) and the argument is a literal of a
different primitive class, tsc widens the literal to its base for the
diagnostic:

```ts
declare function f1<T>(x: T, y: string | T): T;
const a7 = f1("hello", 1);
//                     ^ TS2345
//   tsc: Argument of type 'number' is not assignable to parameter of type 'string'.
//   tsz: Argument of type '1' is not assignable to parameter of type 'string'.
```

The TS2344 emitter already has `widen_literal_type_arg_for_constraint_display`
for this exact rule. The TS2345 emitter (`error_argument_not_assignable_at`)
needs an analog (or to share the same helper).

## Targeted tests

- `conformance/types/typeRelationships/typeInference/unionTypeInference.ts`
  (TS2345, single fingerprint diff)

## Files Touched (planned)

- `crates/tsz-checker/src/error_reporter/call_errors/error_emission.rs`
  (TS2345 call site)
- `crates/tsz-checker/src/error_reporter/generics.rs`
  (move `widen_literal_type_arg_for_constraint_display` to a shared location, OR add a dedicated TS2345 variant)
- New unit tests

## Verification

- `cargo nextest run -p tsz-checker --lib` clean
- `./scripts/conformance/conformance.sh run --filter unionTypeInference --verbose` flips
- Snapshot regen net-positive
