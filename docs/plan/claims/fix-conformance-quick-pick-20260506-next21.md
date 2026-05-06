# fix(solver): accept tuple numeric props before array fallback

- **Date**: 2026-05-06
- **Branch**: `fix/conformance-quick-pick-20260506-next21`
- **PR**: #3680
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Claimed `TypeScript/tests/cases/conformance/expressions/arrayLiterals/arrayLiterals3.ts`.

Current `origin/main` emits the expected TS2322 for the file, but also emits an
extra TS2739 at `var c0: tup = [...temp2]`. The slice will align array-literal
spread assignment diagnostics with tsc by checking tuple numeric properties
before falling back to the synthetic Array<T> interface comparison.

## Files Touched

- `crates/tsz-solver/src/relations/subtype/core.rs`
- `crates/tsz-checker/src/assignability/assignment_checker_tests.rs`
- `docs/plan/claims/fix-conformance-quick-pick-20260506-next21.md`

## Verification

- `cargo fmt --all`
- `CARGO_TARGET_DIR=.target/nextest-local cargo nextest run -p tsz-checker tuple_spread_assignment_satisfies_numeric_object_properties`
- `./scripts/conformance/conformance.sh run --filter "arrayLiterals3" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
- `git diff --check`
- `scripts/architecture-check.sh --quick`
- `CARGO_TARGET_DIR=.target/nextest-local cargo clippy -p tsz-solver -p tsz-checker --lib -- -D warnings`
