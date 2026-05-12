# perf(checker,common): T2.2 prep — classify why each cross-file cache lookup misses

- **Date**: 2026-05-12
- **Branch**: `perf/t2.2-cross-file-cache-miss-cause-2026-05-12`
- **Base**: `perf/master`
- **PR**: TBD (draft against `perf/master` with `WIP` label, per `.claude/CLAUDE.md` §0)
- **Status**: claim
- **Workstream**: `PERFORMANCE_PLAN.md` §7 Tier 2.2 — pre-migration instrumentation

## Intent

The 2026-05-11 attribution decision record locked in the figure:

> `delegate.cache_hits_cross_file = 0` on the cliff. 1107 cross-arena
> delegate calls on `monorepo-006` produce 0 cross-file cache hits.

That figure is load-bearing for the entire Tier 2.2 prioritisation,
but it is **single-channel**: the only signal the JSON snapshot
carries is "miss = anything that wasn't a hit." Three structurally
different miss causes collapse into the same number:

1. `share_owner_symbol_type_results == false` — the gate short-
   circuits every lookup, regardless of the bucket state.
2. The bucket has no entry for `(kind, file_idx, primary, ...)`.
3. The bucket has an entry, but `type_id_is_known_to_db` rejects it
   (a child-checker-allocated `TypeId` that isn't visible to the
   reader), or the cached value is `TypeId::ERROR` / `UNKNOWN`.

Each of these maps to a different fix. (1) is a gate problem, (2)
is a cache-key problem, (3) is a `TypeId` namespacing problem.
Without distinguishing them, the next migration PR is guessing.

This PR adds a `cross_file_cache_miss_cause` classification array
mirroring the pattern of `alias_shortcut_outcomes` and
`direct_interface_lowering_outcomes` from #5843. The four reader
helpers in `crates/tsz-checker/src/context/cross_file_query.rs`
(`cached_cross_file_symbol_type`, `cached_cross_file_interface_type`,
`cached_cross_file_interface_member_simple_type`,
`cached_cross_file_class_instance_type`) record the precise reason
on every miss. The next attribution run will surface those buckets,
and the **first** T2.2 architecture PR — whether it's a DefId-keyed
cache redesign, a gate tightening, or a `TypeId` re-validation —
can target the dominant cause directly.

## Approach

1. New enum + `*_NAMES` constant + `*_COUNT` in
   `crates/tsz-common/src/perf_counters.rs`:

   ```rust
   #[derive(Copy, Clone, Debug, Eq, PartialEq)]
   #[repr(usize)]
   pub enum CrossFileCacheMissCause {
       GateOff = 0,
       BucketEmpty = 1,
       SentinelErrorUnknown = 2,
       TypeIdNotInterned = 3,
   }
   ```

2. New atomics array on `PerfCounters` plus a `record_cross_file_cache_miss_cause(...)`
   helper following the existing `record_cross_arena_*` shape (with
   the `enabled_fast()` early return so the cost on disabled runs
   is one atomic load).

3. New `cross_file_cache_miss_causes: Vec<NamedCount>` field on
   `PerfCounterSnapshot`. The pattern matches the three classification
   arrays added in #5843 exactly.

4. Wire the recorder into all four reader helpers in
   `crates/tsz-checker/src/context/cross_file_query.rs`. The
   helpers already have the gate check, the bucket lookup, and the
   sentinel filter as distinct branches; recording at each branch
   point is a one-line addition.

5. Tests:
   - Top-level key present in `serde_json::to_value(snapshot)`.
   - `cross_file_cache_miss_causes` has exactly
     `CROSS_FILE_CACHE_MISS_CAUSE_COUNT` rows in
     `*_NAMES` declaration order.
   - Each `NamedCount` row has the `{name, count}` shape.
   - Atomic-bump test: writing directly to the underlying atomic
     surfaces as a count delta in the snapshot. (Same pattern as
     `classification_arrays_propagate_atomic_state_into_snapshot`
     in #5843.)

`PERF_COUNTER_SNAPSHOT_SCHEMA_VERSION` stays at `1` — pure additive
extension.

## Out of scope

- Any change to the cache key (`SymbolId` vs `DefId` redesign).
- Any change to the gate's default state.
- Any new producer wiring outside the four helpers in
  `cross_file_query.rs`.
- Rerunning attribution mode against the cliff fixtures. That's a
  separate run-and-record PR after the producer is in place.

## Files Touched (estimated)

- `crates/tsz-common/src/perf_counters.rs` — new enum, names array,
  atomic field, snapshot field, recorder helper, plus extension of
  the existing `json_tests` module (~120 LOC additive).
- `crates/tsz-checker/src/context/cross_file_query.rs` — wire
  recorder into 4 reader helpers (~40 LOC additive at branch points).
- `docs/plan/claims/perf-t2.2-cross-file-cache-miss-cause-2026-05-12.md`
  — this file.

No checker / solver / emitter semantic changes. No conformance
surface touched.

## Verification

- `cargo check -p tsz-common -p tsz-checker -p tsz-cli`
- `cargo nextest run -p tsz-common --lib -E 'test(/json_tests::/)'`
- `cargo nextest run -p tsz-checker --lib`
- `cargo clippy -p tsz-common -p tsz-checker --all-targets -- -D warnings`

## Risk

Zero conformance surface. The recorder is gated through
`enabled_fast()` so disabled runs pay one relaxed atomic load per
recorder call site. JSON consumers either see the new key (and
ignore it if unknown) or read it directly.

## Followups (not in this PR)

After this PR lands and an attribution run captures the breakdown:

- If `GateOff` dominates: investigate why the gate is off on the
  cliff fixtures despite the `shared_definition_store` install path
  setting it to `true` in `ProgramContext::apply_to`.
- If `BucketEmpty` dominates: the cache-key SymbolId-namespace
  collision hypothesis from the 2026-05-12 Explore agent's diagnosis
  is confirmed; redesign the four bucket helpers to canonicalise on
  `DefId` (or the owner's local `SymbolId` via the owner binder).
- If `TypeIdNotInterned` dominates: the cached `TypeId` was
  allocated by a now-defunct child checker; the cache write path
  needs to use a TypeId that's interned in the parent's TypeInterner
  (or the cache value needs to round-trip through `DefinitionStore`
  rather than carry a raw `TypeId`).
