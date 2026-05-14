# 2026-05-14 - Sequential File-Session Reuse Default-On (monorepo-006)

Follow-up on current `main` (`7f124a76bb`) to flip sequential file-session
reuse from env-gated to default-on behavior for no-emit checking.

## Reproducer

| Item | Value |
| --- | --- |
| commit | `71994a8eaf` |
| branch | `codex/perf-next-slice-20260514` |
| `tsz` build | `cargo build -p tsz-cli --bin tsz --release --features perf-tools` |
| fixture path | `/private/tmp/tsz-bench-fixtures/monorepo-006` |
| counter mode | `TSZ_PERF_COUNTERS=1` |
| machine | macOS Darwin 25.1.0 arm64 |

Raw JSON artifacts:

- default-on run 1:
  - `docs/plan/perf-runs/raw/2026-05-14-file-session-reuse-default-monorepo-006-diag.json`
  - `docs/plan/perf-runs/raw/2026-05-14-file-session-reuse-default-monorepo-006-pc.json`
- opt-out run 1 (`TSZ_DISABLE_FILE_SESSION_REUSE=1`):
  - `docs/plan/perf-runs/raw/2026-05-14-file-session-reuse-disabled-monorepo-006-diag.json`
  - `docs/plan/perf-runs/raw/2026-05-14-file-session-reuse-disabled-monorepo-006-pc.json`
- default-on run 2:
  - `docs/plan/perf-runs/raw/2026-05-14-file-session-reuse-default2-monorepo-006-diag.json`
  - `docs/plan/perf-runs/raw/2026-05-14-file-session-reuse-default2-monorepo-006-pc.json`
- opt-out run 2 (`TSZ_DISABLE_FILE_SESSION_REUSE=1`):
  - `docs/plan/perf-runs/raw/2026-05-14-file-session-reuse-disabled2-monorepo-006-diag.json`
  - `docs/plan/perf-runs/raw/2026-05-14-file-session-reuse-disabled2-monorepo-006-pc.json`

The fixture intentionally emits diagnostics, so `tsz` exits with code `2`.
Artifacts are still written and are the source of truth.

## Change

- Sequential no-emit checking now reuses one `CheckerState` by default.
- `TSZ_DISABLE_FILE_SESSION_REUSE=1` provides an explicit opt-out back to
  fresh-per-file checker construction.
- Parallel chunk reuse remains explicit opt-in through `TSZ_FILE_SESSION_REUSE=1`.

## Counter Outcomes

Stable behavior/correctness signals on monorepo-006:

- diagnostics unchanged: `10,198` in all runs.
- child-checker constructions unchanged at zero:
  `checker.with_parent_cache_constructed=0`.

Deterministic checker-lifetime shift:

| Metric | Opt-out | Default-on |
| --- | ---: | ---: |
| `checker.state_constructed` | 5,251 | 2 |
| `checker.file_session_resets` | 0 | 5,249 |
| `delegate.calls` | 975 | 879 |
| `delegate.cache_hits_cross_file` | 434 | 340 |
| `delegate.misses` | 0 | 0 |

## Timing Snapshots

Timing is noisy on shared runners, so these are observations, not a
single-point timing claim.

A/B pair 1 (`opt-out` -> `default-on`):

- check: `77.37s -> 78.50s` (`+1.46%`)
- total: `78.82s -> 79.94s` (`+1.42%`)

A/B pair 2 (`opt-out` -> `default-on`):

- check: `79.14s -> 65.25s` (`-17.55%`)
- total: `80.89s -> 67.02s` (`-17.15%`)

## Decision

Keep sequential file-session reuse default-on for no-emit checking, retain the
`TSZ_DISABLE_FILE_SESSION_REUSE=1` escape hatch, and keep parallel reuse as
explicit opt-in. Treat this as a checker-construction/counter improvement with
mixed wall-time evidence under noisy runners rather than a stable timing win.
