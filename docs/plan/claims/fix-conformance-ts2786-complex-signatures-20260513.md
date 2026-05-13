# fix(checker): suppress false TS2786 for complex JSX signatures

- **Date**: 2026-05-13
- **Branch**: `fix/conformance-ts2786-complex-signatures-20260513`
- **PR**: TBD
- **Status**: ready
- **Workstream**: conformance

## Intent

Remove the final conformance blocker on current `main`: `callsOnComplexSignatures.tsx` emitted one extra TS2786 for a React `ComponentType<P1> | ComponentType<P2>` JSX tag. The fix keeps invalid return-type diagnostics intact while skipping the legacy class-return check for React-style readonly-props component unions after props extraction succeeds.

## Files Touched

- `crates/tsz-checker/src/checkers/jsx/extraction_render_fallback.rs` (~55 LOC)
- `crates/tsz-checker/src/checkers/jsx/orchestration/resolution.rs` (~10 LOC)
- `crates/tsz-checker/src/checkers/jsx/tests.rs` (~25 LOC)

## Verification

- `./scripts/conformance/conformance.sh run --filter callsOnComplexSignatures.tsx --workers 4` (1/1 passed)
- `cargo test -p tsz-checker --lib jsx_react_component_type_union_does_not_emit_ts2786 -- --nocapture` (1 passed)
- `cargo fmt --all -- --check` (passed)
- `git diff --check` (passed)
- `./scripts/conformance/conformance.sh run --workers 16` (12582/12582 passed; skipped 3; known failures 0; crashed 0; timeout 0; fingerprint-only 0; net 12581 -> 12582, +1)
