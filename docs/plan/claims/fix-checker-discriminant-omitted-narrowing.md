# fix(checker): narrow union via absent-required discriminator inference

- **Date**: 2026-04-26
- **Branch**: `fix/checker-tracked-symbols-helper`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Fix the false-positive TS7006 on `discriminantPropertyInference.ts`:
when an object-literal argument completely OMITS a discriminator
property, the contextual narrowing must eliminate union members that
*require* the discriminator and pick the arm where it is optional.
This is what gives the callback parameter `n` its contextual type.

`narrow_contextual_union_via_object_literal_discriminants` already had
the absent-required-property logic (with the comment explicitly
describing the `disc?: false` case), but PR #753 had wrapped it in a
`if unit_discriminants.is_empty() { true } else { ... }` guard that
disabled the check whenever the literal had no unit-typed discriminator
— which is *exactly* the omitted-discriminator case the comment
described.

The naïve un-guarding regressed `indirectDiscriminantAndExcessProperty.ts`,
where `type: foo1` (with `foo1: string`) supplies a discriminator slot
with a non-unit value. There the user is attempting a dynamic
discriminator and tsc keeps the diagnostic against the full union.

The fix narrows the guard: bail entirely when the literal supplies a
discriminator slot with a non-unit value. Otherwise run the
absent-required check unconditionally.

## Files Touched

- `crates/tsz-checker/src/types/computation/object_literal_context.rs`
  (~30 LOC: extract `is_discriminator_slot` closure, add
   `non_unit_named_properties` collection, add early-return when a
   discriminator slot is supplied with a non-unit value, drop the
   `unit_discriminants.is_empty()` guard on `absent_required_match`)
- `crates/tsz-checker/tests/contextual_typing_tests.rs`
  (+85 LOC, 2 new tests locking both halves)

## Verification

- `cargo nextest run -p tsz-checker --test contextual_typing_tests`
- `cargo nextest run -p tsz-checker --lib -E 'test(discriminat)'` (14 PASS, no regressions)
- Full conformance: net +1 from this fix (`discriminantPropertyInference.ts`
  flips PASS, `indirectDiscriminantAndExcessProperty.ts` stays PASS).
  Other listed improvements are stale-snapshot drift from already-merged PRs.
