# chore(conformance): split process RSS cfg paths

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-conformance-process-rss-cfg-20260512`
- **PR**: #5725
- **Status**: ready
- **Workstream**: 8.4 (lint cleanup)

## Intent

Split `get_process_rss` into per-platform `cfg` implementations so Linux,
macOS, and unsupported targets each compile only the relevant body. This removes
the `#[allow(unreachable_code)]` fallback that existed only because the old
single function returned early inside platform-specific blocks.

## Files Touched

- `crates/conformance/src/process_rss.rs`
- `docs/plan/claims/codex-cleanup-conformance-process-rss-cfg-20260512.md`

## Review Follow-up

- Replaced the Linux 4 KiB page-size assumption with `sysconf(_SC_PAGESIZE)`.
- Gated the positive RSS test to platforms where RSS lookup is expected to work.

## Verification

- `cargo fmt -p tsz-conformance`
- `cargo test -p tsz-conformance process_rss::`
- `cargo clippy -p tsz-conformance --all-targets -- -D warnings`
- `git diff --check`
- `cargo build --profile dist-fast -p tsz-cli` (pre-commit prerequisite)
- pre-commit hook: 275 `tsz-conformance` tests passed
