# fix(checker): report TS2852 for await using outside async contexts

- **Date**: 2026-05-12
- **Branch**: `fix-checker-await-using-ts2852-20260512`
- **Issue**: #5967
- **Status**: ready
- **Workstream**: 1 (diagnostic parity for an existing checker bug)

## Intent

Make `await using` declarations match `tsc` by reporting TS2852 when they
appear inside a non-async function, while preserving valid top-level module and
async function usage.

## Planned Scope

- Checker or grammar validation path for `await using` declarations.
- Focused diagnostic tests covering invalid non-async function usage and valid
  async/top-level module usage.

## Changes

- Report TS2852 when `await using` appears in a non-async function, including
  nested non-async functions under async parents.
- Report TS2853 for top-level `await using` in script files while preserving
  valid top-level module usage.
- Added the focused `ts2852_await_using_context_tests` integration test target.

## Verification Plan

- `env CARGO_INCREMENTAL=0 cargo test -p tsz-checker --test ts2852_await_using_context_tests -- --nocapture`
- `env CARGO_INCREMENTAL=0 cargo check -p tsz-checker`
- `git diff --check`
- `env CARGO_INCREMENTAL=0 cargo build --release -p tsz-cli --bin tsz`
- Manual comparison against `tsc` and `.target/release/tsz` for the issue repro,
  nested non-async function usage, top-level script TS2853, and top-level module
  no-diagnostic behavior.
