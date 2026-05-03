# fix(checker): preserve `| undefined` in TS2322 target display for optional-property literal-union targets

- **Date**: 2026-05-03
- **Branch**: `fix/optional-prop-target-undefined-display`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance — TS2322 fingerprint parity for optional properties at object-literal call sites)

## Intent

For an object-literal property failing assignability against an *optional*
target property under `--strict` (strictNullChecks), tsc preserves the
synthesized `| undefined` arm in the TS2322 target display **only when the
stripped form is a multi-member union** — e.g.

```ts
declare function f(x: { method?: 'x' | 'y' }): void;
f({ method: 'z' });
// tsc: Type '"z"' is not assignable to type '"x" | "y" | undefined'.
// tsz before: Type '"z"' is not assignable to type '"x" | "y"'.   ← drops undefined
// tsz after:  Type '"z"' is not assignable to type '"x" | "y" | undefined'.
```

For an optional plain-primitive property like `name?: string`, tsc strips
`| undefined` because the stripped form is a single primitive — the noisy
`| undefined` would be redundant against the surrounding context. Both
shapes are now correctly reproduced by tsz.

The bug was in `object_literal_property_contextual_target_for_diagnostic`
(`crates/tsz-checker/src/error_reporter/core/diagnostic_source/object_literal_targets.rs`).
The function correctly computes the optional-aware contextual target
(`'x' | 'y' | undefined`) but the gate `(contextual_target == current_target
&& !same_target_but_recoverable)` returned `None` for plain-union targets,
causing the caller to fall through to `strip_nullish_for_assignability_display`
which strips `| undefined` from any nullable target when the source is
non-nullable. The fix relaxes the gate to short-circuit past the strip
when the contextual target is the optional read-side form whose stripped
shape is a multi-member union.

A new helper `target_property_is_optional_in_object_literal_context`
walks from the property node up to the enclosing object literal and
queries the contextual target's shape for the property's `optional` flag.
This keeps the fix structural rather than provenance-plumbed (no new
field on `DiagnosticTypeDisplayRole`).

## Files Touched

- `crates/tsz-checker/src/error_reporter/core/diagnostic_source/object_literal_targets.rs`
  (+86, -2) — relaxed gate, new helper, two new conditions
  (`optional_property_target_with_undefined`,
  `contextual_adds_only_undefined`) gated on
  `stripped_is_multi_member_union`.
- `crates/tsz-checker/src/lib.rs` (+3) — register the new test module.
- `crates/tsz-checker/tests/optional_property_target_undefined_display_tests.rs`
  (new file, +96) — 4 unit tests with two distinct property/literal name
  choices (`method`/`'x'|'y'` and `flag`/`'on'|'off'`) for the
  literal-union case, plus locks for optional-primitive (no-undefined) and
  required-property (no-undefined).

## Verification

- `cargo nextest run -p tsz-checker -E 'test(optional_property_target_undefined_display)'`
  — 4/4 new tests pass.
- `cargo nextest run -p tsz-checker --lib` — 3157 tests pass (no regressions).
- `./scripts/conformance/conformance.sh run --filter "destructuringParameterDeclaration8" --verbose`
  — `destructuringParameterDeclaration8.ts` moves from 5 fingerprint
  mismatches (2 missing, 3 extra) to 1 fingerprint mismatch (0 missing, 1
  extra — the remaining issue is a separate diagnostic-suppression cascade
  bug at line 5:15, out of scope).
- Full conformance: identical net delta (`12346 → 12352, +6, 7 wins, 1
  regression`) with and without the patch on the same `main` HEAD,
  confirming the fix is conformance-neutral with **no regressions**. The
  +6 / 7-up / 1-down deltas are pre-existing snapshot drift unrelated to
  this PR. The fingerprint-quality improvement
  (`destructuringParameterDeclaration8.ts` and likely similar tests with
  the same pattern) is a Tier 1 fingerprint-parity contribution that
  doesn't flip individual tests but reduces the per-test fingerprint-mismatch
  count.

## Notes

The remaining `destructuringParameterDeclaration8.ts` extra fingerprint
(line 5:15 `Type '"c"' is not assignable to type '"a" | "b"'`) is a
**separate** diagnostic-suppression cascade bug: tsc emits TS2339 only
for the `nested: { p = 'c' }` pattern (because `nested?: ...` is
optional/undefined and `.p` access fails), suppressing the secondary
TS2322 default-value mismatch; tsz emits both. Tracking as a follow-up.
