# chore(conformance): remove stale dead code allowances

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-conformance-dead-code-20260512`
- **PR**: #5686
- **Status**: ready
- **Workstream**: 8.4 (dead code cleanup)

## Intent

Remove conformance harness dead code that was kept behind
`#[allow(dead_code)]`: the old source-pragma strictness detector, which is no
longer part of tsconfig generation, and the deserialized server response `id`
field that is never read.

## Files Touched

- `crates/conformance/src/tsz_wrapper.rs`
- `crates/conformance/src/server_pool.rs`
- `docs/plan/claims/codex-cleanup-conformance-dead-code-20260512.md`

## Verification

- `cargo fmt -p tsz-conformance`
- `cargo check -p tsz-conformance`
- `cargo clippy -p tsz-conformance --all-targets -- -D warnings`
- `cargo build -p tsz-cli --bin tsz`
- `cargo test -p tsz-conformance`
