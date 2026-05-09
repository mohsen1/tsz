# [WIP] fix(checker): align JSX overload diagnostic anchor

- **Date**: 2026-05-05
- **Branch**: `conformance/quick-pick-20260505-next19`
- **PR**: #3262
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the quick-picked fingerprint-only mismatch in
`tsxStatelessFunctionComponentOverload4.tsx`. The diagnostic code set already
matches tsc (`TS2769`), but the overload diagnostic fingerprint differs for the
`TestingOptional` declaration surface around line 38.

## Files Touched

- `crates/tsz-checker/src/checkers/jsx/orchestration/resolution.rs`
- `crates/tsz-checker/src/checkers/jsx/overloads.rs`
- `crates/tsz-checker/tests/jsx_overload_anchor_literal_attr_tests.rs`

## Verification

- `scripts/session/quick-pick.sh --run` selected and reproduced
  `TypeScript/tests/cases/conformance/jsx/tsxStatelessFunctionComponentOverload4.tsx`.
- `cargo fmt --check`
- `CARGO_TARGET_DIR=/tmp/tsz-next19-target cargo nextest run -p tsz-checker --test jsx_overload_anchor_literal_attr_tests`
- `CARGO_TARGET_DIR=/tmp/tsz-next19-target cargo nextest run -p tsz-checker --lib jsx_class_overload_synthesized_children_not_excess`
- `./scripts/conformance/conformance.sh run --filter "tsxStatelessFunctionComponentOverload4" --verbose`
