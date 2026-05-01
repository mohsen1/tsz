# fix(checker): narrow union excess-property check past partial-prop members

- **Date**: 2026-05-01
- **Branch**: `claude/brave-thompson-tnmOm`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

`discriminant_matching_union_member_indices` (used by the union excess-property
path) was bailing the moment any *currently-active* union member lacked a
discriminator property. That over-conservative early exit hid real excess
properties when the source narrowed cleanly to a single member but other
members in the union didn't carry the discriminator.

This PR aligns the helper with tsc's `discriminateTypeByDiscriminableItems`:
it now treats a property as a discriminator when it appears in the *target
union* with at least one unit type and a non-uniform set of types, and it
filters out members that lack the property (matching tsc's
`getTypeOfPropertyOfType` returning `undefined` for missing keys). When a
filter would empty the candidate set, the helper falls back to the prior
narrowed set instead of resetting it.

Flips `excessPropertyCheckWithUnions.ts` from FAIL → PASS without affecting
the existing `Ambiguous` shape (where only the `tag` discriminator narrows
to a multi-member union).

## Files Touched

- `crates/tsz-checker/src/state/state_checking/property.rs` (~60 LOC change in
  `discriminant_matching_union_member_indices`)
- `crates/tsz-checker/tests/ts2353_tests.rs` (+71 LOC; two structural lock
  tests, including a renamed-discriminator variant per §25 anti-hardcoding
  directive)
- `scripts/session/pick-random-failure.sh` (new — thin wrapper around
  `scripts/session/pick.py quick`)

## Verification

- `cargo test --package tsz-checker --test ts2353_tests` — 33 tests pass
  (including the two new locks).
- `cargo test --package tsz-checker --test ts2322_tests` — 141 tests pass.
- `cargo test --package tsz-checker --lib` — 3075 tests pass.
- `cargo fmt --all --check` — clean.
- `cargo clippy --package tsz-checker --all-targets --all-features -- -D warnings` —
  clean.
- Targeted conformance: `excessPropertyCheckWithUnions.ts` now PASS (was
  fingerprint-only FAIL).
- Smoke conformance: `--filter excessProperty` (9/11 pass — same baseline; +1
  improvement for the targeted test, no regressions on the other two
  fingerprint-only failures), `--filter discriminat` (16/16 pass),
  `--filter union` (55/60 — all 5 failures pre-existing in baseline),
  `--max 500` and `--offset 500 --max 500` smoke samples (499/500 each, no
  new fingerprint-only failures introduced).
