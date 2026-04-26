# fix(checker): select tagged template overload by arity for contextual typing

- **Date**: 2026-04-26
- **Branch**: `fix/checker-tagged-template-overload-arity-contextual`
- **PR**: #1326
- **Status**: ready
- **Workstream**: Conformance / TS2345 false positive

## Intent

Tagged template type computation called `get_contextual_signature` without
threading the effective argument count, so mixed-arity overload sets returned
`None` from `combine_contextual_signatures` and the path fell through to a
signature-less single pass. The single pass left the type parameter `T`
un-inferred, so later concrete substitutions (e.g. `${ 10 }`) tripped a
spurious `TS2345 'number' is not assignable to 'T'`. This change threads the
tagged template's effective arg count (`1 + substitution_count`) into
contextual signature selection, mirroring the regular call expression path
in `call/inner.rs`.

## Files Touched

- `crates/tsz-checker/src/types/computation/tagged_template.rs` (~12 LOC)
- `crates/tsz-checker/tests/conformance_issues/features/templates.rs` (regression test, ~38 LOC)

## Verification

- `cargo nextest run -p tsz-checker --lib` (2793 tests pass)
- `cargo nextest run -p tsz-checker -- test_tagged_template` (6/6 pass, including new regression test)
- `./scripts/conformance/conformance.sh run --filter "parenthesizedContexualTyping"` — `parenthesizedContexualTyping3.ts` now PASSES (was: `expected []` vs `actual [TS2345]`)
- `./scripts/conformance/conformance.sh run --filter "templateString"` — 151/151 pass
- `./scripts/conformance/conformance.sh run --filter "overload"` — 62/62 pass
- `./scripts/conformance/conformance.sh run --filter "tag"` — no new error-code-level failures (pre-existing fingerprint-only and wrong-code failures unchanged)
