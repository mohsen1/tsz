# perf(common,cli): T0.3 perf-counter JSON snapshot + write

- **Date**: 2026-05-10
- **Branch**: `perf/t0.3-perf-counter-json-2026-05-10`
- **PR**: TBD (will draft as WIP)
- **Status**: claim
- **Workstream**: `PERFORMANCE_PLAN.md` Tier 0 — T0.3 / PR 2

## Intent

Implement `PerfCounters::snapshot()` and `write_json_to(path)` per
`PERFORMANCE_PLAN.md` §4.T0.3 / §11 PR 2. Distinguishes `null` (not wired)
from `0` (wired but didn't happen) so reviewers don't mistake one for the
other. Default end-user `tsz` release builds expose no flag and pay no
extra cost.

## Approach

1. Add a `PerfCounterSnapshot` value object in `tsz_common::perf_counters`
   with serde::Serialize. Sub-structs: `WiredCounters`, `DelegateCounters`,
   `CheckerCounters`, `OverlayCounters`, `ResolverCounters`,
   `InternerCounters`. Top-level fields: `schema_version`, `enabled`, `mode`,
   `wired`, plus the section structs.
2. Add `PerfCounters::snapshot()` that loads all atomics once. Buckets that
   the producer code does not yet write get `Option<u64>::None` and the
   matching `wired.<key> = false`.
3. Add `PerfCounters::write_json_to(path: &Path)` doing serde_json + atomic
   rename, mirroring T0.2's pattern.
4. Gate a `--perf-counters-json <path>` flag in `tsz-cli` behind the
   existing `perf-tools` feature (introduced in T0.2). Call site wired
   alongside the existing diagnostics-JSON path.
5. T0.2's report gains a `mode: \"attribution\"` opt-in when the perf
   counters are enabled (`TSZ_PERF_COUNTERS=1`), per the plan's
   timing/attribution split.

## Files Touched (planned)

- `crates/tsz-common/src/perf_counters.rs` (new types, snapshot, write_json_to)
- `crates/tsz-cli/src/commands/args.rs` (gated flag)
- `crates/tsz-cli/src/bin/tsz.rs` (gated call site)
- `crates/tsz-cli/src/perf_json.rs` (mode flips when counters enabled)

## Verification

- `cargo build -p tsz-common -p tsz-cli` (default): unchanged surface.
- `cargo build -p tsz-cli --features perf-tools`: flag visible to clap,
  hidden from `--help`.
- `cargo nextest run -p tsz-common -E 'test(perf_counters)'`: snapshot
  serializes; wired keys match the unwired-bucket set; round-trip via
  serde_json parses back to the same shape.
- End-to-end:
  `TSZ_PERF_COUNTERS=1 .target/release/tsz --noEmit --perf-counters-json /tmp/c.json file.ts`
  emits `enabled: true, mode: \"attribution\"`, populates wired buckets.
  Without the env var, `enabled: false` and unwired buckets serialize as
  `null`.

## Out of scope (follow-up)

- Wiring more counter sites (resolver fs probes, interner intern calls
  per kind). Plan §4 lists the priority buckets; this PR ships the
  schema and surfaces what's already wired.
- T0.4 phase split + decision record.
