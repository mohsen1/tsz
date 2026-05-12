# chore(conformance): narrow memory limit allowance

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-conformance-mem-limit-allowance-20260512`
- **PR**: #5719
- **Status**: ready
- **Workstream**: 8.4 (lint cleanup)

## Intent

Narrow the conformance batch worker's `max_rss_bytes` unused-variable lint
allowance to non-Linux targets. The parameter is used by the Linux `RLIMIT_AS`
safety net, while macOS and other non-Linux builds compile out that block.

## Files Touched

- `crates/conformance/src/batch_pool.rs`
- `docs/plan/claims/codex-cleanup-conformance-mem-limit-allowance-20260512.md`

## Verification

- `cargo fmt -p tsz-conformance`
- `cargo test -p tsz-conformance batch_pool::`
- `cargo clippy -p tsz-conformance --all-targets -- -D warnings`
- `git diff --check`
- `cargo build --profile dist-fast -p tsz-cli` (pre-commit prerequisite)
- pre-commit hook: 275 `tsz-conformance` tests passed
