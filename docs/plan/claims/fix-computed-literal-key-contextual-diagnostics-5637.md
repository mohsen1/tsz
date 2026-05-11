# fix(checker): normalize computed literal keys in contextual object type diagnostics

- **Date**: 2026-05-12
- **Branch**: `fix/computed-literal-key-contextual-diagnostics-5637`
- **PR**: #5644
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Fix issue #5637, where TS2322 source rendering for contextual object literals
prints expanded computed literal expressions such as `[""+"foo"]` and
`[+"foo"]` instead of tsc-style normalized index signatures. The change should
stay in diagnostic source rendering and preserve detailed computed-property
display outside the contextual assignment fingerprint path.

## Files Touched

- `crates/tsz-checker/src/error_reporter/core/diagnostic_source.rs`
- `crates/tsz-checker/tests/ts2322_literal_source_display_tests.rs`

## Verification

- `cargo fmt --all --check`
- `cargo test -p tsz-checker --test ts2322_literal_source_display_tests` (5 passed, 1 ignored)
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run --filter computedPropertyNamesContextualType --verbose` (20/20 passed)
- `git diff --check`
