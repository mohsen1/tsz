# checker: refine generic call instantiated_params with Round 1 substitution (intra-expression inference)

- **Date**: 2026-05-04 03:35:00
- **Branch**: `claude/brave-thompson-qDjDh`
- **PR**: TBD
- **Status**: ready
- **Workstream**: conformance / wrong-code (TS2322 missing in intra-expression
  inference with homomorphic mapped + `infer` return positions)

## Intent

The checker's intra-expression two-pass logic (`call/inner.rs`) builds a
correct Round 1 substitution from non-sensitive object-literal contributors
(e.g., a `setup(): { outputs: O }` method that pins `O = { ... }`). The
solver's single-pass `resolve_call`, run after Round 2 with the refined
arg types, drops bindings when the same parameter appears in another
property's contextual signature through a homomorphic mapped + `infer`
shape (`map: (...) => Unwrap<O>`) where reverse inference fails. The
solver falls back to the parameter's constraint, so the post-call
assignability recheck accepts the callback body vacuously and a TS2322
that `tsc` emits goes missing.

This change overlays the checker's Round 1 substitution onto the solver's
`instantiated_params` whenever (a) at least one argument actually
contributed a Round 1 partial extraction (so we know the checker has
information the solver might lack), and (b) for the specific type
parameter we're improving, the solver's instantiation can be reproduced
by mapping that parameter to its constraint (i.e., the solver clearly
defaulted). The recheck then sees the tighter expected type and emits
the missing `TS2322`.

The refinement is gated:

- Only fires when the checker's value for a type parameter is concrete
  (no `infer`/type-parameter refs) and not equal to the parameter's
  constraint.
- Only fires when the call had at least one Round 1 partial extraction —
  i.e., a non-sensitive object-literal property fed inference. Calls
  whose arguments are all sensitive callbacks (e.g.,
  `Promise.then(() => x, () => 1)`) leave the solver's inference
  untouched: the checker's Round 1 substitution there is derived from
  the same arg types the solver sees at its boundary.
- Per type parameter, only swaps when (i) the solver's instantiation can
  be reproduced by `T => T.constraint` (modulo other tps' checker
  bindings) under mutual subtyping, *and* (ii) re-instantiating with the
  merged substitution does not leave bare type-parameter refs in the
  result, *and* (iii) produces a fresh subtype of the solver's
  already-instantiated param. Any condition failing means the solver's
  inference was at least as tight, so we leave it alone.

## Files Touched

- `crates/tsz-checker/src/types/computation/call/inner.rs` (~50 LOC:
  hoist Round 1 substitution snapshot gated on a Round 1 partial
  extraction, invoke refinement after the initial and retry
  `resolve_call`).
- `crates/tsz-checker/src/types/computation/call_inference.rs` (~110 LOC:
  new `refine_instantiated_params_with_checker_substitution` helper).
- `crates/tsz-checker/src/types/computation/intra_expression_inference_tests.rs`
  (un-ignore the existing regression test, add a renamed sibling per
  CLAUDE.md §25 anti-hardcoding).

## Verification

- `cargo nextest run -p tsz-checker --lib` (3278 / 3278 pass, including
  `intra_expression_inference_homomorphic_mapped_return_type` and the
  new `_renamed` sibling).
- `cargo nextest run -p tsz-solver --lib` (5603 / 5603 pass).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  (clean).
- `cargo fmt --all --check` (clean).
- `./scripts/conformance/conformance.sh run --filter intraExpressionInferences --verbose`:
  the missing `TS2322 test.ts:132:5` is now emitted (with a small
  printer-display delta `bool: error` vs. tsc's `bool: any`, and the
  optional target wrapped in `| undefined`). Two pre-existing
  `TS18046 't' is of type 'unknown'` extras in `whatIWant` /
  `nonObject` are unchanged — separate inference bug, not blocked by
  this PR.
- Targeted regression-set sanity (no regressions on
  `promiseChaining1`, `promiseChaining2`, `promiseTypeStrictNull`,
  `reverseMappedPartiallyInferableTypes`, `implicitAnyGenericTypeInference`,
  `declarationEmitUsingAlternativeContainingModules1/2`,
  `intraExpressionInferencesJsx`).
