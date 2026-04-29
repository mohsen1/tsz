# fix(conformance): restore JSDoc prefix/postfix parsing diagnostics

- **Date**: 2026-04-29
- **Branch**: `fix/conformance-quick-pick-20260429-7`
- **PR**: #1823
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Picked by `scripts/session/quick-pick.sh` on 2026-04-29 20:40:57 UTC.
Target `TypeScript/tests/cases/conformance/jsdoc/jsdocPrefixPostfixParsing.ts`
currently emits no diagnostics where `tsc` expects `TS1005`, `TS1014`,
`TS7006`, and `TS8024`. This PR will diagnose the parser/checker boundary
root cause, restore the missing diagnostics in the owning layer, and add a
focused Rust regression test.

## Files Touched

- `crates/tsz-checker/src/jsdoc/params.rs`
- `crates/tsz-checker/tests/jsdoc_readonly_tests.rs`

## Verification

- `cargo check --package tsz-checker`
- `cargo check --package tsz-solver`
- `cargo build --profile dist-fast --bin tsz`
- `cargo nextest run --package tsz-checker --lib` (3021 passed, 10 skipped)
- `./scripts/conformance/conformance.sh run --filter "jsdocPrefixPostfixParsing" --verbose` (1/1 passed)
- `./scripts/conformance/conformance.sh run --filter "jsdocPostfixEqualsAddsOptionality" --verbose` (1/1 passed)
- `./scripts/conformance/conformance.sh run --max 200` (200/200 passed)
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run` (12269/12582 passed, target listed as improved, net +34 vs stored snapshot)
