# fix(checker): align long object instantiation chain fingerprint

- **Date**: 2026-05-05
- **Branch**: `fix/checker-long-object-instantiation-chain-fingerprint`
- **PR**: https://github.com/mohsen1/tsz/pull/3366
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the random conformance pick
`TypeScript/tests/cases/compiler/longObjectInstantiationChain3.ts`, where
`tsc` and `tsz` both emit `TS2339` but the diagnostic fingerprint differs for
property access through a long generic object merge chain.

## Files Touched

- `docs/plan/claims/fix-checker-long-object-instantiation-chain-fingerprint.md`
- `crates/tsz-solver/src/caches/db.rs`
- `crates/tsz-solver/src/caches/query_cache.rs`
- `crates/tsz-solver/src/evaluation/evaluate.rs`
- `crates/tsz-solver/src/intern/core/interner.rs`
- `crates/tsz-solver/src/diagnostics/format/mod.rs`
- `crates/tsz-solver/src/diagnostics/format/tests.rs`
- `crates/tsz-checker/src/error_reporter/core/diagnostic_source.rs`
- `crates/tsz-checker/src/error_reporter/core_formatting.rs`
- `crates/tsz-checker/src/error_reporter/mod.rs`
- `crates/tsz-checker/src/error_reporter/property_receiver_formatting.rs`
- `crates/tsz-checker/tests/conformance_issues/errors/error_cases.rs`

## Verification

- `./scripts/conformance/conformance.sh run --filter "longObjectInstantiationChain3" --verbose`
- `cargo nextest run --target-dir .target -p tsz-checker test_ts2339_keeps_conditional_merge_receiver_branch_display test_ts2339_elides_long_merge_receiver_instantiation_chain`
- `cargo nextest run --target-dir .target -p tsz-solver display_alias_does_not_repaint_preexisting_structural_type concrete_display_alias_can_name_preexisting_structural_type preferred_application_display_alias_can_name_preexisting_structural_type display_alias_can_be_stored_for_empty_object_type`
- `./scripts/conformance/conformance.sh run --max 200`
