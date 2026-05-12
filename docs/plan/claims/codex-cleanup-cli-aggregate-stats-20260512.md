# chore(cli): remove aggregate stats assignment suppressions

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-cli-aggregate-stats-20260512`
- **PR**: #5661
- **Status**: ready
- **Workstream**: 8.4 (DRY CLI cleanup)

## Intent

Remove the local `#[allow(unused_assignments)]` suppressions around
`collect_diagnostics` aggregate stats. Both checking paths already produce the
query-cache and definition-store stats values, so returning those values from
the branch expression keeps the control flow explicit without placeholder
initialization.

The scoped pre-commit tests also exposed a stale hard-coded CLI config default.
The CLI test now uses the shared default-module helper instead of duplicating
the old literal expectation.

## Files Touched

- `crates/tsz-cli/src/driver/check.rs`
- `crates/tsz-cli/tests/config_tests.rs`
- `docs/plan/claims/codex-cleanup-cli-aggregate-stats-20260512.md`

## Verification

- `cargo fmt -p tsz-cli`
- `cargo check -p tsz-cli`
- `cargo clippy -p tsz-cli --all-targets -- -D warnings`
- `cargo nextest run -p tsz-cli config_tests::resolve_compiler_options_defaults --no-fail-fast`
- `git diff --check`
