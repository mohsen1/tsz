# Investigation: top-level-return-var literal preservation

- **Date**: 2026-05-04
- **Branch**: `claude/brave-thompson-cj2vT`
- **PR**: TBD
- **Status**: claim
- **Workstream**: TS2322 / fingerprint parity (call-argument display)

## Intent

Document a tsc-parity issue uncovered while investigating
`tests/cases/conformance/types/typeRelationships/typeInference/genericClassWithFunctionTypedMemberArguments.ts`.
The test fails with a single fingerprint diff at line 62:30:

```
- expected: TS2345 ... parameter of type '(a: number) => 1'
- actual:   TS2345 ... parameter of type '(a: number) => number'
```

The triggering call is

```ts
class C3<T, U> {
    foo3<T, U>(x: T, cb: (a: T) => U, y: U) { return cb(x); }
}
declare var c3: C3<number, string>;
function other<T, U>(t: T, u: U) {
    var r12 = c3.foo3(1, function (a) { return '' }, 1); // TS2345
}
```

tsc preserves `U = 1` here because:

1. `U` appears at the **top level** of `foo3`'s return type (the body returns `cb(x): U`).
2. The third argument `y: U <- 1` adds a `NakedTypeVariable` candidate
   for `U` whose surface is the fresh literal `1`.
3. `getCovariantInference` evaluates
   `widenLiteralTypes = !primitive && topLevel && (isFixed || !isTypeParameterAtTopLevel(returnType, U))`.
   For the round-1 fix-step (`isFixed = false`) with `isTypeParameterAtTopLevel(U, U) = true`,
   that condition is `false`, so the literal `1` survives.
4. The cb is then contextually typed against `(a: number) => 1`, and the
   function returning `''` (a fresh string) trips TS2345 with the
   literal-preserving message.

tsz currently widens `U` to `number`, which produces the wrong fingerprint.

## Why a one-shot fix is non-trivial

A direct port of tsc's rule into `resolve_from_candidates` (gated by a
new `top_level_return_vars` set) is architecturally clean but does not
land the test in isolation, because the upstream pipeline pre-widens
the candidate before fix-time observes it:

- `compute_contextual_types` (in `crates/tsz-solver/src/operations/generic_call/normalization.rs`)
  runs an early Round 1 inference whose result is fed back to the
  checker as the contextual type for callbacks. With the rule applied
  there, sibling calls like
  `c.foo2(1, function <Z>(a: Z) { return '' })` regress because the
  callback-return-only candidate gets preserved at literal `''`,
  whereas tsc widens that path during expression checking before
  inference observes it.
- A `surviving_priority_is_naked` gate (i.e. only preserve when the
  surviving candidate is `NakedTypeVariable`, not `ReturnType`)
  resolves the sibling regression but does not flip the original test.
  By the time the canonical `resolve_generic_call_inner` reaches the
  fix-step for `r12`, the `NakedTypeVariable` candidate for `U` is
  already `number` (`is_fresh_literal = false`); the literal `1` was
  widened upstream of the inference pass — apparently inside the
  constraint walker or expression-type-of for the third argument, fed
  back through compute_contextual_types.
- Preserving the literal at the display layer (the
  `format_type_for_assignability_message` path in
  `crates/tsz-checker/src/error_reporter/core/type_display.rs` widens
  function return types unconditionally for display) flips the
  fingerprint when the underlying solver type is preserved, but
  regresses TS2322-side tests like
  `function_expression_assignment_reports_outer_signature_mismatch`
  (the assignability message there expects the widened
  `(x: number) => number` form for an arrow expression assigned to an
  interface call signature).

The end-to-end fix appears to require three coordinated changes:

1. Plumb `isTypeParameterAtTopLevel(returnType, T)` info into both
   `compute_contextual_types` and `resolve_generic_call_inner` so that
   the round-1 fix step preserves fresh-literal `NakedTypeVariable`
   candidates.
2. Stop the upstream widening of fresh primitive-literal expression
   types when their target is a `__placeholder_*` for a
   top-level-return type-parameter that would benefit from
   preservation.
3. Make the assignability-display widening conditional on the SOURCE
   side being a fresh function expression (TS2322 keep-widening) vs.
   the TARGET side being an inference-produced function type (TS2345
   preserve). This requires either a role-aware
   `format_type_for_assignability_message` or a freshness flag on
   function shapes.

Steps (2) and (3) cross several pre-existing assumptions (constraint
walker widening, formatter widening for arrow returns), and either one
in isolation introduces regressions in unrelated tests
(`test_generic_call_widens_fresh_object_union_inferred_type`,
`function_expression_assignment_reports_outer_signature_mismatch`,
`missing_property_message_uses_contextual_function_parameter_types`).

## Files Touched

- `docs/plan/claims/claude-brave-thompson-cj2vT.md` (this file)

## Verification

- `scripts/session/verify-all.sh` — green (no code changes shipped).

## Next steps

A follow-up PR should sequence the three coordinated changes and ship
unit tests for the round-1 fix-step preservation rule, the
constraint-walker widening exception for top-level-return placeholders,
and the source-vs-target assignability-display widening role.
