# fix(checker): preserve instance type after `new` arg mismatch and emit TS2339 in constructor binding patterns

- **Date**: 2026-04-26
- **Branch**: `fix/solver-new-arg-mismatch-preserves-instance-type`
- **PR**: #1345
- **Status**: ready
- **Workstream**: 1 (Conformance fixes)

## Intent

Two coupled checker-side fixes that flip `destructuringParameterProperties5`
plus a broader cluster of property-after-mismatched-call scenarios:

1. **`new`-expression `ArgumentTypeMismatch` recovery**: when the solver
   returns `ArgumentTypeMismatch { fallback_return: TypeId::ERROR }` for a
   `new C(<bad-args>)` call, recover the constructor's instance type via
   `construct_return_type_for_type` instead of decaying to `TypeId::ERROR`.
   This mirrors the existing call-expression fallback in `handle_call_result`
   (which uses `get_function_return_type`). Without this, `var a = new C(<bad>)`
   left `a` typed as `error`, silencing every TS2339 on subsequent
   `a.<missing-prop>` access.
2. **Constructor binding-pattern checks**: `check_constructor_declaration_with_request`
   now invokes `check_parameter_binding_pattern_defaults`, mirroring the
   call already present for regular function declarations. This descends
   into nested binding patterns on constructor parameters (e.g.
   `constructor([{ x1, x2 }, y]: [ObjType1, number])`) and emits TS2339
   for properties that don't exist on the source type.

```ts
// Bug 1: silenced TS2339 after `new` with bad args
class C { constructor(n: number) {} }
var a = new C("bad");  // tsz emitted only TS2345
a.foo;                 // tsc emits TS2339; tsz had nothing

// Bug 2: silenced TS2339 inside constructor binding pattern
type T = [{ x: number }, number]
class C2 {
  constructor([{ a, b }, n]: T) {}  // tsc TS2339 on a, b; tsz had nothing
}
```

## Files Touched

- `crates/tsz-checker/src/state/state_checking_members/ambient_signature_checks.rs` (~7 LOC)
- `crates/tsz-checker/src/types/computation/complex.rs` (~14 LOC + ~28 LOC unit test)
- `crates/tsz-checker/src/checkers/parameter_checker.rs` (~22 LOC unit test)

## Verification

- `cargo nextest run --package tsz-solver --lib` (5449 tests pass)
- `cargo nextest run --package tsz-checker --lib` (2823 tests pass + 2 new)
- `./scripts/conformance/conformance.sh run --filter "destructuringParameterProperties5" --verbose` — passes
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run` —
  conformance Net: 12144 → 12156 (+12), zero regressions
