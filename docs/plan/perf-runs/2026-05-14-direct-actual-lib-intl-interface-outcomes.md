# 2026-05-14 - Direct Actual-lib Intl Interface Outcome Attribution

Attribution-mode follow-up on top of `a5834834c1`
(`fix(checker): skip TS2786 return-type check for React alias types in overload path`).

## Reproducer

| Item | Value |
| --- | --- |
| commit | `a5834834c1` |
| branch | `codex/perf-intl-info-interface-outcomes-20260514` |
| `tsz` build | `cargo build -p tsz-cli --bin tsz --release --features perf-tools` |
| fixture path | `/private/tmp/tsz-bench-fixtures/monorepo-006` |
| counter mode | `TSZ_PERF_COUNTERS=1` |
| machine | macOS Darwin 25.1.0 arm64 |

Raw JSON:

- `docs/plan/perf-runs/raw/2026-05-14-direct-actual-lib-intl-interface-outcomes-monorepo-006-diag.json`
- `docs/plan/perf-runs/raw/2026-05-14-direct-actual-lib-intl-interface-outcomes-monorepo-006-pc.json`

The fixture intentionally emits diagnostics, so `tsz` exits with code `2`.
Artifacts are still written and are the source of truth.

## Change

Add a new perf-counter outcome surface for the direct actual-lib Intl interface
lane:

- counter array: `direct_actual_lib_intl_interface_outcomes`
- recorder: `record_direct_actual_lib_intl_interface_outcome`
- text dump block: `Direct actual-lib Intl interface outcomes`
- JSON snapshot field with stable ordered names

This is instrumentation-only: no type-checking behavior is changed.

## Observed Outcomes (monorepo-006)

| Outcome | Count |
| --- | ---: |
| `success_namespace_export` | 8 |
| all other rows | 0 |

Current headline counters on this main tip:

- diagnostics: `10,198`
- `DelegateCrossArenaSymbol` children: `0`
- `delegate.misses`: `0`
- `checker.with_parent_cache_constructed`: `0`
- declaration-file miss residues: none

## Decision

Keep this instrumentation slice. It adds a stable JSON/text attribution surface
for direct actual-lib Intl interface routing without changing semantics.
