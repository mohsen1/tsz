# Claim — Refresh post-#5863 attribution run with #6111 in flight

**Owner:** claude (this session)
**Branch:** `perf/attribution-refresh-post-6111-2026-05-13`
**Draft PR:** to be opened with this claim

## Goal

Produce a fresh attribution-mode run against `monorepo-001..006` on
current `main` (post #5863 and adjacent merges) and check in a decision
record under `docs/plan/perf-runs/`. The post-#5863 run is called out in
[`PERFORMANCE_PLAN.md`](../PERFORMANCE_PLAN.md) §2 status rows as the
next concrete decision input for:

- T2.1.D — picking the hottest child-checker path to replace with a
  session lease or typed query.
- T2.2.b — picking the next `CheckerCreationReason` after #6111's
  `DelegateCrossArenaSymbol` work.

## Why now

- #5863 wired the four-bucket `cross_file_cache_miss_causes`
  classification (`gate_off`, `bucket_empty`, `sentinel_error_unknown`,
  `type_id_not_interned`) into the readers in
  `crates/tsz-checker/src/context/cross_file_query.rs`. The bench harness
  can now split the load-bearing `cache_hits_cross_file = 0` figure into
  structural causes.
- #6111 is in flight and is the first cross-file cache-key migration
  built on top of #5863. Its body shows a per-fixture sample
  (`cache_hits_cross_file = 632`, `bucket_empty = 251` after the PR);
  the formal decision record covering ALL six cliff fixtures hasn't
  been checked in yet.
- The 2026-05-11 decision record predates #5863's reader-side wiring
  and #6111's first migration. Picking the next T2.2 migration from
  stale data would risk duplicating #6111's territory or missing a
  larger bucket.

## What this slice does

1. Build `tsz` with `--features perf-tools` on current `main`.
2. Run attribution mode (`TSZ_PERF_COUNTERS=1`) against
   `monorepo-001..006`, emitting per-fixture diagnostics + perf-counter
   JSON.
3. Save raw JSON under `docs/plan/perf-runs/raw/`.
4. Write up the breakdown in
   `docs/plan/perf-runs/2026-05-13-attribution-post-6111.md`:
   - lock-wait histogram (confirm the May-11 conclusion still holds).
   - `with_parent_cache_constructed` ratio (movement vs May-11's 1.22).
   - `delegate.cache_hits_cross_file` total + per-reason breakdown.
   - `cross_file_cache_miss_causes` four-bucket split (the load-bearing
     new signal).
   - `delegate_miss_classification` (by_kind + declaration-file vs
     source-file totals).
   - Recommended next T2.2 migration target, derived from the dominant
     `cross_file_cache_miss_causes` bucket.

## What this slice does NOT do

- No code changes to checker / solver / cli logic.
- No new counter wiring (#5843/#5863 are sufficient for the buckets we
  consume).
- No claim on the next T2.2 migration PR itself — only the decision
  record that names it.

## Coordination

- Will not touch any file outside `docs/plan/perf-runs/`.
- Pure read-only / report-out work; no merge conflict surface with
  in-flight code PRs (#6111, #6116, #6125, etc.).
- Bench is single-host (no cloud runner pin); the May-11 record set the
  precedent for checking in single-host attribution numbers when
  `large-ts-repo` is OOM-blocked.

## Exit criteria

- Six per-fixture perf-counter JSONs checked in under `raw/`.
- One markdown summary checked in under `perf-runs/`.
- One open-issue-or-PR pointer in the summary identifying the next
  T2.2 migration target.
- PR description quotes the new `cross_file_cache_miss_causes` table
  and the recommended migration.
