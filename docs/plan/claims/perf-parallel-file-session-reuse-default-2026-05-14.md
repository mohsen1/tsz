# Claim: Enable parallel file-session reuse by default

Date: 2026-05-14

## Claim

Enable parallel chunked file-session reuse by default in no-emit checking,
while keeping the shared opt-out (`TSZ_DISABLE_FILE_SESSION_REUSE=1`) and
preserving diagnostics.

## Evidence

- `crates/tsz-cli/src/driver/check.rs`
  - `parallel_file_session_reuse_requested()` now defaults to enabled unless
    `TSZ_DISABLE_FILE_SESSION_REUSE` is set.
  - Existing compatibility knob `TSZ_FILE_SESSION_REUSE` remains accepted.
  - Parallel-branch gating comments updated.
- `docs/plan/perf-runs/2026-05-14-parallel-file-session-reuse-default.md`
  - records 40-file and 400-file default-vs-disabled attribution evidence.
- `docs/plan/PERFORMANCE_PLAN.md`
  - PR7B status updated with the parallel default-on follow-up and links.

## Validation

- `cargo test -p tsz-cli file_session_reuse_preserves_multifile_diagnostics -- --nocapture`
- `cargo test -p tsz-cli file_session_reuse_preserves_parallel_multifile_diagnostics -- --nocapture`
- `cargo build -p tsz-cli --bin tsz --release --features perf-tools`
- Synthetic 40-file no-wildcard fixture:
  - default vs `TSZ_DISABLE_FILE_SESSION_REUSE=1` attribution runs
- Synthetic 400-file no-wildcard fixture:
  - default vs `TSZ_DISABLE_FILE_SESSION_REUSE=1` attribution runs

All runs keep diagnostics identical for each fixture (`40` and `400`
respectively) while shifting construction counters to reuse-oriented values.
