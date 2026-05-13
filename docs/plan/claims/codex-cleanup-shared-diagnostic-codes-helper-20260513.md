# Shared Checker-Test Diagnostic Code Helper

Issue: https://github.com/mohsen1/tsz/issues/6327
Branch: `codex/cleanup-shared-diagnostic-codes-helper-20260513`
Status: Ready

## Scope

Add a shared diagnostic-code projection helper to
`crates/tsz-checker/src/test_utils.rs` and replace local/repeated projection
helpers across checker tests.

Touched areas:

- JSDoc diagnostic assertion tests
- Tuple index access tests
- Contextual TS2345 tests
- Spread/rest tests
- Remaining direct TS2322 diagnostic-code projection

This is a behavior-preserving checker-test cleanup. It does not change roadmap
metrics, compiler behavior, or implementation direction.

Related narrower cleanup PRs already covered some per-file dedupe before this
branch became implementation-ready. This branch now keeps the useful shared
helper layer and migrates the remaining focused local helpers/direct projections
without expanding into unrelated checker tests.

## Verification

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --lib -E 'test(jsdoc_accessibility_tests::) + test(jsdoc_callback_rest_tests::) + test(jsdoc_readonly_tests::) + test(jsdoc_satisfies_tests::) + test(jsdoc_template_class_tests::)' --no-fail-fast`
- `cargo nextest run -p tsz-checker --test jsdoc_type_tag_tests --test tuple_index_access_tests --test contextual_ts2345_conformance_tests --test spread_rest_tests --no-fail-fast`
- `cargo nextest run -p tsz-checker --test ts2322_tests -E 'test(test_ts2322_array_not_assignable_to_interface_extending_array_with_extra_props)' --no-fail-fast`
- `cargo clippy --profile ci-lint -p tsz-checker --all-targets -- -D warnings`
- `scripts/safe-run.sh cargo nextest run --profile ci --cargo-profile ci-unit -p tsz-common -p tsz-scanner -p tsz-parser -p tsz-binder -p tsz-solver -p tsz-checker -p tsz-emitter -p tsz-lsp -p tsz-core` reached the full local unit lane and reported 24 pre-existing checker failures; sampled `generic_alias_assignability_pollution_tests` reproduces with the same missing-lib failures on fresh `origin/main` at `eb60cfbc61`.
- Full PR CI after marking the PR ready.
