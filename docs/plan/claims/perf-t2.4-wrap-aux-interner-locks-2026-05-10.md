# perf(solver): T2.4 — wrap auxiliary-interner write-locks with `time_shard_write`

- **Date**: 2026-05-10
- **Branch**: `perf/t2.4-wrap-aux-interner-locks-2026-05-10`
- **PR**: TBD
- **Status**: claim
- **Workstream**: T2.4 lock-wait instrumentation (PERFORMANCE_PLAN.md §9, §3 lock-wait timing shape)

## Intent

The lock-wait histogram framework (`tsz_common::perf_counters::time_shard_write`)
is wired and used by the primary `TypeShard`'s write-lock acquisitions in
`intern_slow_path` (`interner.rs:1159, 1166`). Two siblings — the
`ConcurrentSliceInterner::intern` slow path (`:376`) and the
`ConcurrentValueInterner::intern` slow path (`:466`) — were not wrapped, so
contention on those `RwLock<Vec<...>>` writes never lands in the histogram.
Both interners back substantial perf-relevant storage:

- `ConcurrentSliceInterner` backs `interner_type_list_intern_calls` (every
  union/intersection member list, every tuple element list, every template
  literal list).
- `ConcurrentValueInterner` backs `interner_object_shape_intern_calls`,
  `interner_function_shape_intern_calls`, `interner_application_intern_calls`,
  `interner_conditional_intern_calls`, `interner_mapped_intern_calls` — every
  shape-bearing type construction.

## What changed

Two lock-write sites are now wrapped with
`tsz_common::perf_counters::time_shard_write(0, || …)`:

- `crates/tsz-solver/src/intern/core/interner.rs:376` —
  `ConcurrentSliceInterner::intern`'s `inner.items.write()` call.
- `crates/tsz-solver/src/intern/core/interner.rs:466` —
  `ConcurrentValueInterner::intern`'s `inner.items.write()` call.

`shard_idx` is `0` because these interners aren't sharded; the
`time_shard_write` doc comment explicitly notes the parameter is reserved
("today every shard's observations land in the same global histogram"), so
the value is forward-compatible.

## Cost model

`time_shard_write` is `cfg`-gated:

- **`perf-counters-timing` ON**: `Instant::now()` brackets the closure;
  elapsed nanos land in `interner_lock_wait_histogram_ns` (gated on
  `enabled_fast()`, so timing-mode runs that don't enable counters still
  pay only the gate load + closure call).
- **`perf-counters-timing` OFF (default)**: the wrapper compiles to a
  direct call of the closure. Zero `Instant::now()`, zero atomic touches.

Both default release builds and timing-mode bench builds remain unaffected.
Only attribution-mode runs with `perf-counters-timing` on now see contention
data from the auxiliary interners.

## Verification

- `cargo check -p tsz-solver` — clean
- `cargo check -p tsz-solver --features tsz-common/perf-counters-timing` — clean
- `cargo nextest run -p tsz-solver --lib -E 'test(intern)'` — 235/235 pass
- Pre-commit hook (fmt, clippy, arch guard, full nextest suite) — to be
  confirmed before push.

## Conformance

No semantic change. Lock-write code paths are byte-identical except for the
closure wrapper; no observable behavior change in default builds.
