Status: ready
Branch: codex/cleanup-tsserver-import-presence-20260512
Owner: Codex
Date: 2026-05-12 01:53:44 UTC

## Intent

Clean up inverted `NodeIndex` sentinel checks in the tsserver incoming-call
import-binding scan by replacing `!x.is_none()` with direct `x.is_some()`
presence checks.

## Scope

- `crates/tsz-cli/src/bin/tsz_server/handlers_structure.rs`

## Verification Plan

- `cargo fmt --check`
- `cargo nextest run -p tsz-cli incoming_calls`
- `cargo nextest run -p tsz-cli`
- CI: unit, conformance, fourslash, emit

## Verification Results

- `cargo fmt --check` passed.
- `cargo nextest run -p tsz-cli incoming_calls` compiled successfully but matched
  0 tests.
- `cargo nextest run -p tsz-cli test_call_hierarchy_incoming` passed: 11
  passed, 1594 skipped.
- `cargo nextest run -p tsz-cli` completed with the existing local CLI baseline:
  1591 run, 1507 passed, 84 failed, 14 skipped. The failures are broad
  pre-existing local driver / tsc-compat expectation families (for example
  TS5011 rootDir expectation tests, showConfig/help parity, and
  `array_values_iterator_helpers_do_not_report_missing_members`), not the
  touched tsserver call-hierarchy import-binding tests.
- `cargo clippy -p tsz-cli --all-targets -- -D warnings` passed.
