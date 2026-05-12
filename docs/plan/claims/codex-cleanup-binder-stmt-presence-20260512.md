# codex/cleanup-binder-stmt-presence-20260512

Status: ready
Owner: codex
Created: 2026-05-12 00:35:57 UTC

## Intent

Simplify the binder CommonJS indicator scanner's sentinel `NodeIndex`
statement filter from `!idx.is_none()` to `idx.is_some()`.

## Scope

- `crates/tsz-binder/src/state/core.rs`
- This claim file

## Verification Plan

- `cargo fmt --check`
- `cargo nextest run -p tsz-binder`
- Pre-commit checks as appropriate for the focused binder cleanup
