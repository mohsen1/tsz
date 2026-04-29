# fix(solver): preserve constrained template literal patterns

- **Date**: 2026-04-29
- **Branch**: `fix/conformance-quick-pick-20260429-5`
- **PR**: #1811
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix two solver-side template literal pattern gaps exposed by the
`templateLiteralTypesPatterns.ts` quick pick: prefixed/suffixed `any`
placeholders now preserve fixed template text instead of widening the whole
template to `string`, and template holes backed by constrained intersections can
consume literal prefixes that satisfy the intersected pattern. The targeted file
still has unrelated alias-display and generic object variance fingerprints, but
this slice removes the `a${any}` prefix loss and the intersected-template false
positive while improving full conformance.

## Files Touched

- `crates/tsz-solver/src/intern/template.rs`
- `crates/tsz-solver/src/relations/subtype/rules/literals.rs`
- `crates/tsz-solver/tests/template_literal_subtype_tests.rs`
- `crates/tsz-solver/tests/isomorphism_validation.rs`

## Verification

- `cargo fmt --check`
- `cargo check --package tsz-solver`
- `cargo check --package tsz-checker`
- `cargo build --profile dist-fast --bin tsz`
- `cargo nextest run -p tsz-solver test_any_widening_in_template test_unknown_widening_in_template template_literal_subtype` (34 tests pass)
- `cargo nextest run --package tsz-solver --lib --hide-progress-bar` (5552 tests pass, 9 skipped)
- `./scripts/conformance/conformance.sh run --filter "templateLiteralTypesPatterns" --verbose` (target remains fingerprint-only; fewer extra/missing fingerprints)
- `./scripts/conformance/conformance.sh run --max 200` (200/200 pass)
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>&1 | grep FINAL` (`FINAL RESULTS: 12244/12582 passed (97.3%)`)
