# 2026-05-14 - Parallel File-Session Reuse Default-On (no-emit lane)

Follow-up on top of `main` to remove the extra opt-in gate for parallel
chunked file-session reuse in no-emit checking.

## Reproducer

| Item | Value |
| --- | --- |
| commit | `ae3cd00802` (pre-change baseline), branch-local after-change run |
| branch | `codex/perf-next-slice2-20260514` |
| `tsz` build | `cargo build -p tsz-cli --bin tsz --release --features perf-tools` |
| fixture A | `/private/tmp/tsz-perf-fixtures/reuse-parallel-40` |
| fixture B | `/private/tmp/tsz-perf-fixtures/reuse-parallel-400` |
| counter mode | `TSZ_PERF_COUNTERS=1` |
| machine | macOS Darwin 25.1.0 arm64 |

Raw artifacts (after change):

- 40-file fixture
  - default: `docs/plan/perf-runs/raw/2026-05-14-parallel-reuse-default-40-diag.json`
  - default: `docs/plan/perf-runs/raw/2026-05-14-parallel-reuse-default-40-pc.json`
  - disabled (`TSZ_DISABLE_FILE_SESSION_REUSE=1`):
    `docs/plan/perf-runs/raw/2026-05-14-parallel-reuse-disabled-40-diag.json`
  - disabled (`TSZ_DISABLE_FILE_SESSION_REUSE=1`):
    `docs/plan/perf-runs/raw/2026-05-14-parallel-reuse-disabled-40-pc.json`
- 400-file fixture
  - default: `docs/plan/perf-runs/raw/2026-05-14-parallel-reuse-default-400-diag.json`
  - default: `docs/plan/perf-runs/raw/2026-05-14-parallel-reuse-default-400-pc.json`
  - disabled (`TSZ_DISABLE_FILE_SESSION_REUSE=1`):
    `docs/plan/perf-runs/raw/2026-05-14-parallel-reuse-disabled-400-diag.json`
  - disabled (`TSZ_DISABLE_FILE_SESSION_REUSE=1`):
    `docs/plan/perf-runs/raw/2026-05-14-parallel-reuse-disabled-400-pc.json`

Both fixtures intentionally emit one type-error diagnostic per source file, so
`tsz` exits with code `2`; artifacts are still written.

## Change

- Parallel no-emit lane now defaults to chunked `CheckerState` reuse.
- Shared opt-out remains `TSZ_DISABLE_FILE_SESSION_REUSE=1`.
- Legacy `TSZ_FILE_SESSION_REUSE` is kept as a compatibility knob.

## Observed Outcomes

Diagnostics remain identical in both fixtures:

- 40-file fixture: `40` diagnostics in default and disabled modes.
- 400-file fixture: `400` diagnostics in default and disabled modes.

Constructor/counter shift (default vs disabled):

| Fixture | `state_constructed` | `file_session_resets` |
| --- | ---: | ---: |
| 40 files | `6` vs `41` | `35` vs `0` |
| 400 files | `51` vs `401` | `350` vs `0` |

Timing snapshots (single-run, noisy):

- 40-file fixture: total `66.27ms` (default) vs `59.72ms` (disabled)
- 400-file fixture: total `427.83ms` (default) vs `440.81ms` (disabled)

## Decision

Keep parallel file-session reuse default-on in the no-emit lane with the same
global disable knob. Treat this as a constructor/counter reduction slice with
mixed small-fixture timing and favorable larger-fixture timing, not a universal
single-run timing claim.
