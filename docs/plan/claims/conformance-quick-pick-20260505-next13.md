# [WIP] fix(checker): align template literal conformance fingerprints

- **Date**: 2026-05-05
- **Branch**: `conformance/quick-pick-20260505-next13`
- **PR**: #3035
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the picked `templateLiteralTypes1.ts` fingerprint mismatch by addressing the
root cause behind misplaced template-literal complexity diagnostics and extra
template-literal assignability errors. The initial target has matching error
codes but divergent fingerprints, so the work will separate semantic relation
bugs from diagnostic rendering/anchoring bugs before changing behavior.

## Files Touched

- `crates/tsz-solver/src/intern/{normalize.rs,template.rs}`
- `crates/tsz-solver/src/evaluation/{evaluate.rs,evaluate_rules/template_literal.rs}`
- `crates/tsz-solver/src/relations/{compat.rs,subtype/core.rs,subtype/rules/generics.rs}`
- `crates/tsz-solver/src/operations/property.rs`
- `crates/tsz-solver/src/caches/{db.rs,query_cache.rs}`
- `crates/tsz-checker/src/types/computation/access.rs`
- `crates/tsz-checker/src/types/property_access_type/helpers.rs`
- `crates/tsz-checker/src/types/{type_node_resolution.rs,type_checking/type_alias_checking.rs}`
- `crates/tsz-checker/src/state/state_checking/source_file.rs`
- `crates/tsz-solver/tests/{template_literal_subtype_tests.rs,generics_rules_tests.rs}`

## Verification

- `cargo fmt --all`
- `cargo build --profile dist-fast --bin tsz`
- `cargo nextest run -p tsz-solver solver_file_size_ceiling_tests::test_solver_file_size_ceiling`
- `cargo nextest run --package tsz-solver --lib`
- `cargo nextest run --package tsz-checker --lib`
- `cargo nextest run -p tsz-checker --test conformance_issues features::elaboration::test_simple_intersection_of_many_unions_emits_ts2590`
- `./scripts/conformance/conformance.sh run --filter "templateLiteralTypes1" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>&1 | grep FINAL`
