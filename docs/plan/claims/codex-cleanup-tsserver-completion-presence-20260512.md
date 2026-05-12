# chore(tsserver): simplify completion auto-import presence guards

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-tsserver-completion-presence-20260512`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 8.4 (DRY emitter helpers)

## Intent

This PR cleans up tsserver completion auto-import guard clauses that combine
action metadata checks with optional completion metadata checks. The change
keeps behavior identical while making the required presence checks read
directly in the completion and snippet flows.

## Files Touched

- `crates/tsz-cli/src/bin/tsz_server/handlers_completions.rs`
- `crates/tsz-cli/src/bin/tsz_server/handlers_completions_snippets.rs`

## Verification

- Pending implementation.
