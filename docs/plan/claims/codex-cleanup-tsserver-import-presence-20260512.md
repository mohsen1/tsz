Status: claim
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
