# fix(checker): align template literal pattern fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-template-literal-patterns-fingerprint`
- **PR**: #3099
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Follow up on PR #1811's partial `templateLiteralTypesPatterns.ts` improvement
and close the remaining fingerprint drift in the same fixture. The current
random pick shows matching TS2322/TS2345 code families in the snapshot, while a
fresh verbose run on `origin/main` exposes remaining alias-display,
template-number-pattern, generic variance, and duplicate-declaration drift.
This PR will keep the slice scoped to the picked fixture and fix root causes in
solver/query or diagnostic formatting layers rather than adding checker-local
suppression.

## Files Touched

- `crates/tsz-solver/src/diagnostics/format/compound.rs`
- `crates/tsz-solver/src/diagnostics/format/mod.rs`
- `crates/tsz-solver/src/diagnostics/format/tests.rs`
- `crates/tsz-solver/src/evaluation/evaluate_rules/template_literal.rs`
- `crates/tsz-solver/src/intern/normalize.rs`
- `crates/tsz-solver/src/intern/template.rs`
- `crates/tsz-solver/src/relations/subtype/rules/literals.rs`
- `crates/tsz-solver/src/relations/variance.rs`
- `crates/tsz-solver/tests/template_literal_subtype_tests.rs`
- `crates/tsz-solver/tests/variance_tests.rs`

## Verification

- `cargo fmt --all`
- `cargo fmt --all --check`
- `cargo check --package tsz-checker --package tsz-solver`
- `cargo build --profile dist-fast --bin tsz`
- `cargo nextest run -p tsz-solver template_literal --hide-progress-bar` (247 passed)
- `cargo nextest run --package tsz-solver --lib --hide-progress-bar` (5655 passed, 9 skipped)
- `./scripts/conformance/conformance.sh run --filter "templateLiteralTypesPatterns" --verbose` (2/2 passed)
- `./scripts/conformance/conformance.sh run --max 200` (200/200 passed)
- `git diff --check`
- pre-commit hook (21000 passed, 61 skipped)
