# perf(common): T0.3 follow-up — expose delegate classification counters in JSON

- **Date**: 2026-05-12
- **Branch**: `perf/t0.3-classification-json-2026-05-12`
- **Base**: `perf/master`
- **PR**: [#5843](https://github.com/mohsen1/tsz/pull/5843)
- **Status**: ready
- **Workstream**: `PERFORMANCE_PLAN.md` §4.T0.3 follow-up

## Intent

Make the four cross-arena classification breakdowns the perf-counter
**text dump** already prints accessible from the perf-counter **JSON
snapshot** so the bench harness and offline analysis tools
(`scripts/conformance/query-conformance.py`-style readers) can pick
the next T2.2 migration target from data instead of `dump_string`
parsing.

The text dump currently emits, but the JSON snapshot does not:

1. `dump_cross_arena_symbol_miss_classification`
   — `delegate_cross_arena_symbol_miss_by_source` (4 buckets),
     `delegate_cross_arena_symbol_miss_by_kind` (N buckets),
     plus the two scalar declaration-file vs. source-file totals.
2. `dump_cross_arena_alias_shortcut_outcomes`
   — `delegate_cross_arena_alias_shortcut_outcome` (N buckets).
3. `dump_direct_cross_file_interface_lowering_outcomes`
   — `direct_cross_file_interface_lowering_outcome` (N buckets).

The headline `by_reason` array (the per-`CheckerCreationReason`
breakdown) was added in #4948 / the T0.3 PR. This PR finishes the
job by adding the **why-the-fast-path-didn't-fire** classification
arrays alongside it.

## Why this matters

The 2026-05-10 attribution decision record
(`docs/plan/perf-runs/2026-05-10-scale-cliff-summary.md`) chose
T2.2 typed cross-file queries as the highest-priority work based
on `delegate.cache_hits_cross_file = 0` and
`with_parent_cache_constructed = 1.28 × files`. Picking the
next migration target requires the *why* signal: each
`CheckerCreationReason` miss has a classification row that tells
a reader whether the miss is fundamental (no fast path exists for
this shape) or accidental (the fast path bailed for a recoverable
reason such as `AliasOutcome::MissingModule` /
`InterfaceValueMerge` / `DefaultImport`).

Without that data in JSON, every attribution run requires manual
text-dump parsing. With it, the bench harness can render a "next
target" table directly from the snapshot.

## Approach

1. Extend `PerfCounterSnapshot` with three new substructures:
   - `delegate_miss_classification: DelegateMissClassification`
   - `alias_shortcut_outcomes: Vec<NamedCount>` (one per
     `CROSS_ARENA_ALIAS_SHORTCUT_OUTCOME_NAMES`)
   - `direct_interface_lowering_outcomes: Vec<NamedCount>` (one per
     `DIRECT_CROSS_FILE_INTERFACE_LOWERING_OUTCOME_NAMES`)
2. The substructure shapes mirror the text dump rows. Always emit
   the full enumeration (not just non-zero rows) so the JSON shape
   is stable across runs. Consumers filter as needed.
3. `PERF_COUNTER_SNAPSHOT_SCHEMA_VERSION` stays at `1` — this is a
   pure schema **extension** (new fields only, no field removal,
   no semantic change).
4. The existing `dump_string` and `dump_*` helpers continue to use
   the atomics directly. Sharing a single snapshot path between
   text and JSON is a separate, larger refactor (`PERFORMANCE_PLAN.md`
   §4.T0.3 calls it out under "Counter Wiring Details"); this PR
   keeps the text path untouched so the change stays additive.
5. Tests:
   - Schema-version unchanged.
   - New top-level keys present in `serde_json::to_value(snapshot)`.
   - Bucket-name arrays match `*_NAMES` constants (catches drift
     when a new variant lands).
   - Sum-of-bucket invariant: `by_source.sum()` matches text-dump
     classification total (locks the populator against off-by-one
     when a new source variant is added).

## Out of scope

- Any change to the producer atomics (`record_cross_arena_*` etc).
- Any change to the text dump format.
- Merging `dump_string` and JSON paths into a single snapshot.
- New migrations of any `CheckerCreationReason`.

## Files Touched (estimated)

- `crates/tsz-common/src/perf_counters.rs` — add struct fields and
  snapshot population (+~60 LOC).
- `crates/tsz-common/src/perf_counters.rs` (tests) — extend the
  `json_tests` module (+~40 LOC).
- `docs/plan/claims/perf-t0.3-classification-json-2026-05-12.md`
  — this file (+~120 LOC).

No checker / solver / emitter changes.

## Verification

Pre-flip-to-ready (local, 2026-05-12):

- `cargo check -p tsz-common -p tsz-cli` — clean (default + `--features perf-tools`).
- `cargo nextest run -p tsz-common` — **437 / 437 pass** (including 19
  `perf_counters::json_tests::*` tests, of which 6 are new in this PR).
- `cargo nextest run -p tsz-checker --lib` — **3862 / 3862 pass**.
- `cargo clippy -p tsz-common --all-targets -- -D warnings` — clean.
- Pre-commit hook — all 5 stages pass.

CI (post-ready-for-review): tracked on PR #5843.

## Risk / regression surface

- **Conformance**: zero. No checker code changes.
- **Bench schema**: additive. Existing consumers see the same keys
  they did before plus three new ones.
- **Text dump**: unchanged. Both paths still read atomics
  independently; the convergence of text-and-JSON onto one
  snapshot path is a separate PR per the plan.
