# fix(checker): preserve computed unique-symbol source on TS2322 in index-signature assignability

- **Date**: 2026-05-11
- **Branch**: `fix/checker-index-signatures-unique-symbol-source-display-fingerprint-20260511`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance / checker)
- **SHA observed**: `ff905b4c0bb6edae6cea1bd113443a4856c71ed3`

## Summary

`TypeScript/tests/cases/conformance/types/members/indexSignatures1.ts` reports a single
fingerprint-only TS2322 mismatch in TS2322 output formatting. Both tools emit the same
diagnostic code and location, but TSZ currently renders the source key text correctly while
rendering the target index signature key kind as `string` instead of `symbol`:

`Type '{ [sym]: number; }' is not assignable to type '{ [key: string]: string; }'.`

while TypeScript reports:

`Type '{ [sym]: number; }' is not assignable to type '{ [key: symbol]: string; }'.`

The mismatch is currently user-visible only in diagnostics text; it does not introduce an
extra/fewer error code.

## Reproduction

```bash
./scripts/conformance/conformance.sh run --filter indexSignatures1 --verbose
```

Current summary from `/tmp/indexSignatures1_conformance_current.txt`:

- same error-code set (`[TS1268, TS1337, TS2322, TS2353, TS2374, TS2413, TS7053]`) in `expected` and `actual`
- Fingerprint-only mismatch count: `1`
- Extra fingerprint line:
  - `TypeScript/tests/cases/conformance/types/members/indexSignatures1.ts:11:5 TS2322`
  - TSZ: `Type '{ [sym]: number; }' is not assignable to type '{ [key: string]: string; }'.`
  - expected/tsc: `Type '{ [sym]: number; }' is not assignable to type '{ [key: symbol]: string; }'.`

Direct compiler-level comparison:

```bash
./scripts/node_modules/typescript/bin/tsc --pretty false --target es2015 --noEmit TypeScript/tests/cases/conformance/types/members/indexSignatures1.ts | grep -n "TS2322"
./.target/dist-fast/tsz --pretty false --target es2015 --noEmit TypeScript/tests/cases/conformance/types/members/indexSignatures1.ts | grep -n "TS2322"
```

Output line:

- tsc: `Type '{ [sym]: number; }' is not assignable to type '{ [key: symbol]: string; }'.`
- tsz: `Type '{ [sym]: number; }' is not assignable to type '{ [key: string]: string; }'.`

## Duplicate check

Searched in `docs/plan/claims` for all of:
`indexSignatures1`, `[sym]`, `__unique_`, and `symbol index`.

- No exact duplicate claim exists for the same fingerprint pair and source-vs-target
  display mismatch.
- There is an adjacent claim `fix-next-conformance-fingerprint-05032005.md`
  (`fix/next-conformance-fingerprint-05032005`) that addresses symbol index
  slots in object-shape evaluation, which appears related but broader (solver behavior
  rather than checker diagnostic-string rendering).

## Why this is happening

1. TSZ retains computed unique-symbol property atoms as synthetic `__unique_*` internals.
2. The assignability formatting path has two separate lanes: source-object rendering
   and target-index signature rendering.
3. Source rendering is now aligned to declaration-like syntax for this repro,
   but TSZ target-side rendering still shows `string` where tsc reports `symbol`.
4. That split suggests an additional fix point in symbol-index classification logic
   (likely in solver shape modeling), matching the existing `fix-next-conformance-fingerprint-05032005`
   claim.

## Fix guide

### Minimum change (safe, diagnostics-only)

In checker diagnostic formatting:

1. Route computed-member-name handling through declaration source text wherever available,
   including `{ [sym]: ... }` object-like displays.
2. Add/adjust tests for computed unique-symbol source display in `ts2322_literal_source_display_tests.rs`
   to assert the source contains `"{ [sym]: number; }"` and does not contain `"__unique_"`.

### Full-close path (this fixture + adjacent signature semantics)

1. Apply the solver-side symbol-index correction from `fix-next-conformance-fingerprint-05032005`
   (or equivalent): avoid conflating symbol keys with string slots in object shape modeling.
2. Re-run full `indexSignatures1` fingerprint comparison.
3. If TS2322 target becomes `'{ [key: symbol]: string; }'`, mark this item closed together with
   the solver fix.

## Local verification

- `cargo test -p tsz-checker ts2322_preserves_computed_unique_symbol_object_key_display -- --nocapture` (passes)
- `./scripts/conformance/conformance.sh run --filter indexSignatures1 --verbose`
  (currently: 1 fingerprint-only mismatch remains on that line)
- `rg -n "indexSignatures1|__unique_|\\[sym\\]" docs/plan/claims` for duplicate detection

## Notes

Existing in-progress implementation already includes:
- `crates/tsz-checker/src/error_reporter/core/diagnostic_source.rs`
- `crates/tsz-checker/tests/ts2322_literal_source_display_tests.rs`

## Closure criteria

- Source diagnostic text uses declaration-like syntax for computed keys in this repro.
  (currently verified by the new checker regression test.)
- TS2322 target in `indexSignatures1.ts` shows `symbol` instead of `string` when the
  expected TS fixture says `{ [key: symbol]: ... }`.
- No additional fingerprint mismatches are introduced elsewhere in
  `TypeScript/tests/cases/conformance/types/members/indexSignatures1.ts`.

## Easy-fix assessment

Not yet classified as a "super easy" single-file diagnostic fix:
- the source-side string rendering for computed keys is fixed by checker display changes already underway;
- the remaining target-side `symbol` vs `string` text comes from lower-layer index-signature
  classification and is tied to solver/object-shape behavior.

Once that shared layer is patched (same intent as
`fix-next-conformance-fingerprint-05032005`), this claim can be promoted to a ready PR.
