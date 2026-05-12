# fix(cli): restore driver diagnostic gates

- **Date**: 2026-05-12
- **Branch**: `fix/cli-driver-diagnostic-gates-20260512`
- **PR**: #5747
- **Status**: ready
- **Workstream**: 8.4 (repo hygiene)

## Intent

Restore the `tsz-cli` pre-commit diagnostic invariants exposed while preparing
the CLI file-discovery cleanup. Direct CLI `--target ES3` removed-value
diagnostics should stop at TS5108, and TS5011 should stay limited to
declaration emit layouts that actually require an explicit `rootDir`.

## Evidence

Both failures reproduce on current `origin/main` with no local cleanup changes:

- `cargo nextest run -p tsz-cli driver_tests::cli_removed_target_es3_emits_ts5108 --no-capture`
- `cargo nextest run -p tsz-cli driver_tests::compile_basic_template_literal --no-capture`

## Duplicate-Work Check

- Reviewed open PRs #5088, #5643, #5732, #5734, #5735, #5737, and #5738.
- No open PR currently touches `crates/tsz-cli/src/driver/core.rs` or
  `crates/tsz-cli/tests/driver_tests.rs`.

## Files Touched

- `crates/tsz-cli/src/driver/core.rs`
- `crates/tsz-cli/tests/driver_tests.rs`
- `docs/plan/claims/fix-cli-driver-diagnostic-gates-20260512.md`

## Verification

- `cargo fmt -p tsz-cli`
- `cargo nextest run -p tsz-cli removed_target_es3 --no-capture`
- `cargo nextest run -p tsz-cli driver_tests::ts5011 --no-capture`
- `cargo nextest run -p tsz-cli compile_basic_template_literal --no-capture`
- `cargo clippy -p tsz-cli --all-targets -- -D warnings`
- `git diff --check`
- `cargo build --profile dist-fast -p tsz-cli` (pre-commit prerequisite)
