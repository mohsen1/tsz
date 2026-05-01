# test(checker): document intra-expression inference precedence for homomorphic mapped return types

- **Date**: 2026-05-01
- **Branch**: `claude/brave-thompson-OFMJm`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance and Fingerprints) — investigation + locked test for a future fix

## Intent

Adds an `#[ignore]`d Rust unit test in `tsz-checker` capturing the desired
behaviour for `conformance/types/typeRelationships/typeInference/intraExpressionInferences.ts`,
which currently misses one TS2322 fingerprint at `test.ts(131,5)`.

The investigation in this branch isolates the bug to the interaction between:

1. The non-sensitive `setup()` method's Round-1 partial type (which correctly
   carries `O = { str: Wrapper<string> }`), and
2. The sensitive `map: (inputs: Unwrap<I>) => Unwrap<O>` callback, whose
   homomorphic mapped + conditional + infer return type is reverse-inferred
   from the callback body in Round 2 and overrides the Round-1 binding for
   `O` even when the new value is incompatible with the existing concrete
   inference.

When the override happens with the constrained `extends WrappedMap` form,
both `I` and `O` fall back to `Record<string, Wrapper>` (the constraint),
which makes `Unwrap<O>` widen to `Record<string, any>`. The map body's
return then becomes vacuously assignable and the TS2322 is silently lost.

The fix path (multi-day) routes through `query_boundaries`:

- Tighten the Round-2 substitution refinement so concrete Round-1 inferences
  are not overwritten by callback-body re-inference (call/inner.rs around
  line 1418 and 1480 — the `should_update` predicate currently drops Round-1
  inferences when the refined value differs).
- Add solver-side reverse mapped-type inference for `Unwrap<O>`-style
  homomorphic mapped templates so the Round-2 contextual return type
  resolves to a structural shape (`{ str: string }`) instead of falling
  back to the constraint, even when the callback body provides
  conflicting evidence.

This PR does **not** attempt the fix; it locks the desired diagnostic so
the next agent picking up this slice has a passing target.

## Files Touched

- `crates/tsz-checker/src/types/computation/helpers.rs` (~60 LOC: new
  `#[ignore]` test + comment block locating the failure surface)
- `docs/plan/claims/claude-brave-thompson-OFMJm.md` (this claim)

## Verification

- `cargo fmt --all --check` — clean
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean
- `cargo nextest run --package tsz-checker --lib` — full suite green (the
  added test is `#[ignore]`'d so it does not run; remove the attribute once
  the Round 1 / Round 2 precedence fix lands).
- `./scripts/conformance/conformance.sh run --filter intraExpressionInferences --verbose`
  — unchanged (still 1/2; this PR documents the gap, does not close it).
