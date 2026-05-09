# fix(checker): align control-flow optional-chain fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-control-flow-optional-chain-fingerprints`
- **PR**: #3054
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the fingerprint-only conformance drift in
`TypeScript/tests/cases/conformance/controlFlow/controlFlowOptionalChain.ts`.
The error-code set already matches `tsc` (`TS2454`, `TS2722`, `TS18048`), so
this slice is expected to focus on diagnostic anchoring or message rendering
for optional-chain control-flow errors.

## Files Touched

- `crates/tsz-binder/src/binding/declaration.rs`
- `crates/tsz-checker/src/context/core.rs`
- `crates/tsz-checker/src/flow/control_flow/condition_narrowing.rs`
- `crates/tsz-checker/src/flow/control_flow/core.rs`
- `crates/tsz-checker/tests/conformance_issues/types/optional_chain.rs`

## Verification

- `cargo fmt --all --check` (pass)
- `cargo check --package tsz-checker --package tsz-solver` (pass)
- `cargo nextest run -p tsz-checker --test conformance_issues optional_chain_undefined_comparisons optional_chain_switch optional_chain_truthiness_false_paths_keep_prefix_optional --no-fail-fast` (pass)
- `./scripts/conformance/conformance.sh run --filter "controlFlowOptionalChain"` (3/3 pass)
- `./scripts/conformance/conformance.sh run --max 200` (200/200 pass)
