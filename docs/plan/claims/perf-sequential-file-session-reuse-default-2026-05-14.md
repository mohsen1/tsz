# Claim: Enable sequential file-session reuse by default

Date: 2026-05-14

## Claim

Enable sequential file-session reuse by default for no-emit checking to reduce
checker-state construction overhead without changing diagnostics, while keeping
an explicit opt-out (`TSZ_DISABLE_FILE_SESSION_REUSE=1`) and leaving parallel
reuse explicit opt-in.

## Evidence

- `crates/tsz-cli/src/driver/check.rs`
  - `file_session_reuse_requested()` now defaults to on and respects
    `TSZ_DISABLE_FILE_SESSION_REUSE=1`.
  - `parallel_file_session_reuse_requested()` keeps parallel chunk reuse behind
    `TSZ_FILE_SESSION_REUSE=1`.
  - sequential/parallel gating comments and caller contract updated.
- `docs/plan/perf-runs/2026-05-14-sequential-file-session-reuse-default.md`
  - captures monorepo-006 A/B attribution evidence (default-on vs opt-out).
- `docs/plan/PERFORMANCE_PLAN.md`
  - PR7A/PR7B status updated with default-on + opt-in/opt-out policy and
    measured outcomes.

## Validation

- `cargo test -p tsz-cli file_session_reuse_preserves_multifile_diagnostics -- --nocapture`
- `cargo test -p tsz-cli file_session_reuse_preserves_parallel_multifile_diagnostics -- --nocapture`
- `cargo build -p tsz-cli --bin tsz --release --features perf-tools`
- `TSZ_PERF_COUNTERS=1 /private/tmp/tsz-perf-next-target/release/tsz --project /private/tmp/tsz-bench-fixtures/monorepo-006/tsconfig.json --diagnostics-json docs/plan/perf-runs/raw/2026-05-14-file-session-reuse-default-monorepo-006-diag.json --perf-counters-json docs/plan/perf-runs/raw/2026-05-14-file-session-reuse-default-monorepo-006-pc.json` (expected exit `2`)
- `TSZ_PERF_COUNTERS=1 TSZ_DISABLE_FILE_SESSION_REUSE=1 /private/tmp/tsz-perf-next-target/release/tsz --project /private/tmp/tsz-bench-fixtures/monorepo-006/tsconfig.json --diagnostics-json docs/plan/perf-runs/raw/2026-05-14-file-session-reuse-disabled-monorepo-006-diag.json --perf-counters-json docs/plan/perf-runs/raw/2026-05-14-file-session-reuse-disabled-monorepo-006-pc.json` (expected exit `2`)
