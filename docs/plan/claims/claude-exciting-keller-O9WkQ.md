# fix(checker/jsx): JSX generic callback prop TS2322 fingerprint parity

- **Date**: 2026-04-29
- **Branch**: `claude/exciting-keller-O9WkQ`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Conformance)

## Intent

`checkJsxGenericTagHasCorrectInferences.tsx` fails with a fingerprint mismatch:
tsc anchors TS2322 at the attribute NAME (`nextValues`, col 54) while TSZ
anchors it at the return expression inside the callback (`a.x`, col 71), because
`check_assignable_or_report_at` triggers source-elaboration that drills into
the lambda body. Additionally, tsc widens literal types during generic
inference (so `{ x: "y" }` becomes `Values = { x: string }`) whereas TSZ
preserves literals through `preserve_literal_types = true`, producing a wrong
inferred constraint and wrong mismatch message.

## Files Touched

- `crates/tsz-checker/src/checkers/jsx/props/resolution.rs`
- `crates/tsz-checker/src/checkers/jsx/orchestration/component_props.rs`
- `crates/tsz-checker/tests/` (new test)

## Verification

- `./scripts/conformance/conformance.sh run --filter "checkJsxGenericTag" --verbose`
- `cargo nextest run -p tsz-checker --lib`
