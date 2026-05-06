# fix(checker): align variadic tuples1 fingerprints

- **Date**: 2026-05-06
- **Branch**: `fix/checker-variadic-tuples1-fingerprint`
- **PR**: #3630
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Reduce the fingerprint-only conformance failure in
`TypeScript/tests/cases/conformance/types/tuple/variadicTuples1.ts`.
Both tsc and tsz emit `TS2322`, `TS2344`, `TS2345`, `TS2555`, and `TS4104`,
but diagnostic fingerprints do not match. The implemented scope fixes the
readonly type-parameter spread identity drift and preserves type-parameter
source display for single-rest variadic tuple assignment diagnostics. The file
remains fingerprint-only because unrelated tuple inference, arity, and alias
display fingerprints still differ.

## Files Touched

- `docs/plan/claims/fix-checker-variadic-tuples1-fingerprint.md`
- `crates/tsz-checker/src/assignability/assignability_checker.rs`
- `crates/tsz-checker/src/assignability/assignability_diagnostics.rs`
- `crates/tsz-checker/src/lib.rs`
- `crates/tsz-checker/src/query_boundaries/common.rs`
- `crates/tsz-checker/tests/variadic_tuple_readonly_relation_tests.rs`
- `crates/tsz-solver/src/diagnostics/format/compound.rs`
- `crates/tsz-solver/src/relations/subtype/rules/unions.rs`
- `crates/tsz-solver/tests/tuple_comprehensive_tests.rs`

## Verification

- `cargo fmt --check`
- `CARGO_TARGET_DIR=.target CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 cargo nextest run -j 1 -p tsz-checker readonly_constrained_type_param_rejects_mutable_spread_tuple_assignment variadic_tuple_assignment_keeps_type_param_source_display --no-tests=fail`
- `CARGO_TARGET_DIR=.target CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 cargo nextest run -j 1 -p tsz-solver test_readonly_type_param_not_assignable_to_mutable_spread_tuple test_nested_readonly_type_param_not_assignable_to_mutable_spread_tuple --no-tests=fail`
- `./scripts/conformance/conformance.sh run --filter "variadicTuples1" --verbose` (still fingerprint-only; reduced missing fingerprints from 10 to 4 and removed the readonly/source-display tuple mismatches)
- `./scripts/conformance/conformance.sh run --max 200`
