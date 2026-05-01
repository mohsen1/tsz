# fix(checker): unfold alias-of-application source for TS2739

- **Date**: 2026-05-01
- **Branch**: `fix/checker-ts2739-alias-of-application-source-display`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance — fingerprint parity)

## Intent

`compiler/objectTypeWithStringAndNumberIndexSignatureToAny.ts` and similar
fingerprint-only failures expect `Type 'NumberTo<number>' is missing the
following properties from type 'Obj': hello, world` for a source typed
`nToNumber: NumberToNumber` where `type NumberToNumber = NumberTo<number>`.
tsz currently shows the wrapper alias `NumberToNumber` because the
assignment-source formatter prefers the declared annotation text.

For TS2739 specifically, tsc unfolds one alias level when the alias body
is a generic Application of a different named type — the application form
names both the underlying generic and its arguments, which is the
structural information the "is missing the following properties" message
is meant to expose. tsc preserves alias names for TS2322 (target context)
and TS2339 (receiver), so this fix is scoped to the four TS2739/TS2741
source-rendering sites in the checker.

## Files Touched

- `crates/tsz-checker/src/error_reporter/render_failure.rs`
  (helper `ts2739_alias_of_application_source_display` + 2 source sites)
- `crates/tsz-checker/src/error_reporter/render_failure/type_mismatch.rs`
  (1 source site)
- `crates/tsz-checker/src/error_reporter/assignability.rs`
  (2 source sites in the index-signature-source TS2739/TS2741 paths)
- `crates/tsz-checker/src/tests/ts2739_alias_unfold_display_tests.rs`
  (3 unit tests; one negative cover plus two name-renamed positive covers
  to satisfy the anti-hardcoding rule from §25)
- `crates/tsz-checker/src/lib.rs` (test module wiring)

## Verification

- `cargo nextest run -p tsz-checker -p tsz-solver --lib` — 8645 tests pass.
- Target conformance: `./scripts/conformance/conformance.sh run --filter "objectTypeWithStringAndNumberIndexSignatureToAny"` → 1/1 PASS.
- Smoke: `--filter alias` → 31/31 PASS; `--filter "types/members"` → 33/34 PASS (the one failure is the pre-existing `indexSignatures1.ts` unrelated to this change); `--filter "typeAlias"` → 22/23 PASS (the one failure is the pre-existing `intrinsicTypes.ts` unrelated to this change).
