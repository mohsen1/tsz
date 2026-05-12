# perf(common,cli,solver): wire interner lock_wait_histogram_ns behind perf-counters-timing cfg

- **Date**: 2026-05-10
- **Branch**: `perf/t0-interner-lock-wait-histogram-2026-05-10`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 5 (Performance) — Tier 0.3 final follow-up

## Intent

Wire `interner.lock_wait_histogram_ns` per `docs/plan/PERFORMANCE_PLAN.md`
§4.T0.3 lock-wait shape. This is the last counter on the snapshot
surface that was still serializing as `null`. With this PR all six
counter buckets named in the plan's T0.3 follow-up list are wired:

1. `interner.intern_calls/hits/misses`        — #4955
2. `resolver.is_file/is_dir/read_dir_calls`   — #4966
3. `interner.lock_wait_histogram_ns`          — **this PR**

Plus the four full-PR follow-ups (#4957, #4960, #4970, #4984).

T2.4 (interner redesign) is no longer blocked on instrumentation: a
fresh attribution rerun now has the lock-wait signal.

## Approach

The plan §4.T0.3 spelled out the shape: a `time_shard_write(shard_idx,
f)` wrapper gated behind a new `perf-counters-timing` cargo feature.
The cfg gate is deliberate (per the plan): "A runtime branch is
acceptable for cheap integer counters, but benchmark timing builds
must not pay timestamp costs." So `Instant::now()` calls must be
*compile-time* eliminated, not just runtime-gated, for timing-mode
bench builds.

### `tsz-common`

- Add `[features] perf-counters-timing = []` to `Cargo.toml`.
- Add `LOCK_WAIT_BUCKET_COUNT = 8` and `LOCK_WAIT_BUCKET_UPPER_BOUNDS_NS`
  (log-spaced over 100 ns…100 ms with an overflow bucket).
- Add `interner_lock_wait_histogram_ns: [AtomicU64; 8]` field to
  `PerfCounters`.
- Add two-arm `time_shard_write<R>(_shard_idx: u32, f: impl FnOnce() -> R) -> R`:
  - `#[cfg(feature = "perf-counters-timing")]`: gates on
    `enabled_fast()`, brackets `f()` with `Instant::now()`, and
    bucketizes the elapsed nanos.
  - `#[cfg(not(feature = "perf-counters-timing"))]`: `#[inline(always)]`
    direct call of `f()`. No timestamp, no histogram access.
- Add `pub const fn lock_wait_histogram_wired() -> bool` so callers
  (snapshot, dump_string) can branch on whether the histogram is
  *physically wired* rather than just whether the env var is set.
- Snapshot: emit `Some(Vec<u64>)` of bucket counts when wired,
  `None` when the cfg is off. `wired.interner_lock_wait` mirrors
  the cfg.
- Add `interner_lock_wait` field to `WiredCounters` and update the
  `wired_keys_match_snapshot_struct_fields` test.
- Existing `unwired_buckets_serialize_as_null` test renamed to
  `lock_wait_histogram_serialization_matches_feature_gate` and made
  to assert both branches: feature-on serializes an array + flag
  true; feature-off serializes null + flag false.

### `tsz-solver`

- In `intern_slow`'s vacant-insert branch, wrap each
  `RwLock::write()` call with `time_shard_write(shard_idx as u32, ...)`.
  The closure returns the `RwLockWriteGuard`; the surrounding scope
  uses it normally. Both shards (`index_to_key` and `alloc_order`)
  are timed.

### `tsz-cli`

- `perf-tools = ["tsz-common/perf-counters-timing"]` so the existing
  `--features perf-tools` build (used by the bench harness) gets the
  histogram automatically. Default release builds keep `default = []`
  → no timing cost.

## Verification

- `cargo nextest run -p tsz-common -E 'test(json_tests)'` — 7/7 pass
  with feature off; 7/7 pass with feature on.
- `cargo clippy -p tsz-cli -p tsz-common -p tsz-solver --features
  tsz-cli/perf-tools --all-targets -- -D warnings` clean.
- `cargo clippy -p tsz-cli -p tsz-common -p tsz-solver --all-targets
  -- -D warnings` (default features) clean.
- End-to-end attribution-mode run on a 3-file fixture with `--features
  perf-tools`:

  ```
  lock_wait_histogram_ns = [1476, 1, 1, 0, 0, 0, 0, 0]
  wired.interner_lock_wait = true
  ```

  1476 acquires under 100 ns (uncontended fast path), 2 in the
  100ns–10µs range — exactly the shape expected on a single-threaded
  small fixture.

- End-to-end timing-mode (default, no `--features perf-tools`):
  the wrapper compiles to a direct call. No timestamp instrumentation
  lands in the binary.

## No conformance / behavior impact

Pure instrumentation. The wrapper is semantically transparent in both
feature configurations.
