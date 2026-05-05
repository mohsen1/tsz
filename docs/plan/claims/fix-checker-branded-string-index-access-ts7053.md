# fix(checker): report TS7053 for branded string index mismatches

- **Date**: 2026-05-05
- **Branch**: `fix/checker-branded-string-index-access-ts7053`
- **PR**: #2764
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Close the branded-string indexed-access slice of the `indexSignatures1`
conformance mismatch. `tsz` currently rejects intersection-branded string
keys such as `string & Tag1` with `TS2538` before normal indexed-access
compatibility can run, while `tsc` treats them as string-like keys and reports
`TS7053` only when the branded key is not accepted by the target index
signature. This PR will keep the fix in the checker/solver boundary for index
key classification and add a focused Rust regression test.

## Files Touched

- `crates/tsz-checker/src/types/computation/access.rs`
- `crates/tsz-checker/src/types/computation/access_helpers.rs`
- `crates/tsz-checker/tests/conformance_issues/types/membership_semantics.rs`
- `crates/tsz-solver/src/evaluation/evaluate_rules/index_access.rs`
- `crates/tsz-solver/src/evaluation/evaluate_rules/mod.rs`
- `crates/tsz-solver/src/evaluation/evaluate_rules/string_index_helpers.rs`
- `crates/tsz-solver/src/type_queries/extended.rs`

## Verification

- `cargo fmt --all --check`
- `cargo check --package tsz-checker`
- `cargo check --package tsz-solver`
- `cargo test -p tsz-solver branded_primitive_intersections_are_valid_index_types -- --nocapture`
- `cargo test -p tsz-solver object_only_intersections_remain_invalid_index_types -- --nocapture`
- `cargo test -p tsz-checker --test conformance_issues test_branded_string_index_key_mismatch_reports_ts7053_not_ts2538 -- --nocapture`
- `cargo test -p tsz-checker --test conformance_issues test_intersected_template_index_signatures_accept_template_string_reads -- --nocapture`
- `./scripts/conformance/conformance.sh run --filter "indexSignatures1" --verbose` (still fails on existing TS2374/TS2413/display mismatches; `TS2538` extra is removed and `TS7053` is present)
- `./scripts/conformance/conformance.sh run --max 200` (200/200)

`cargo nextest` is not installed in this environment, so targeted `cargo test`
commands were used for local verification.
