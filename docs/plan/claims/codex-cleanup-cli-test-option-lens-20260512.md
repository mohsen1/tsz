# chore(cli-tests): tidy optional vector length assertions

Status: ready
Branch: codex/cleanup-tsserver-result-presence-20260512
PR: #5857
Owner: Codex
Date: 2026-05-12

## Intent

Replace repeated optional-vector length assertion patterns in CLI tests with a small local helper so the tests read by behavior instead of Option plumbing.

## Planned Scope

- crates/tsz-cli/tests/args_tests.rs
- crates/tsz-cli/tests/config_tests.rs

## Verification Plan

- cargo nextest run -p tsz-cli args_tests config_tests --no-fail-fast
- cargo clippy -p tsz-cli --tests -- -D warnings
