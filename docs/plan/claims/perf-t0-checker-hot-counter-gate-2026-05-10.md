# perf(checker): gate-once the hottest checker perf-counter inc() call sites

**2026-05-10 14:20:00**

## Scope

Five inline `inc(&counters().X)` increments in checker code that pay an
unconditional `OnceLock<PerfCounters>::get()` per invocation. All five
are on multi-million-call hot paths (one per symbol-type lookup or
cross-file type-params query), where the gate-once pattern saves a real
amount of disabled-mode overhead.

Sites:

1. `computed/mod.rs:697` — `compute_type_of_symbol_calls`
2. `core.rs:2056` — `compute_type_of_symbol_cache_hits` (provisional cache hit)
3. `core.rs:2077` — `compute_type_of_symbol_cache_hits` (cached symbol type)
4. `type_environment/core.rs:1825` — `cross_file_type_params_cache_hits` (arena-targeted)
5. `type_environment/core.rs:1838` — `cross_file_type_params_cache_misses` (arena-targeted)
6. `type_environment/core.rs:1959` — `cross_file_type_params_cache_hits` (file-targeted)
7. `type_environment/core.rs:1970` — `cross_file_type_params_cache_misses` (file-targeted)

## Approach

Same gate-then-deref pattern as the resolver wrappers (#4966, #5000):

```rust
if tsz_common::perf_counters::enabled_fast() {
    tsz_common::perf_counters::inc(
        &tsz_common::perf_counters::counters().X,
    );
}
```

`enabled_fast()` reads a cached `OnceLock<bool>` (one atomic load + one
predictable branch). When false, the `counters()` deref is skipped
entirely — a `OnceLock<PerfCounters>::get()` saved per invocation.

Not introducing a helper module/function because:

1. These are deep inside specific control-flow branches; a wrapping
   helper would either need a closure (defeating the gate-cheap goal)
   or a field selector (cluttering the call site more than the inline
   `if`).
2. The inline `if` gate is the same shape used in the `count_*`
   wrappers in `resolution.rs`, just expressed at the call site.

## Behavior

- **Enabled mode** (`TSZ_PERF_COUNTERS=1`): counter values unchanged.
- **Disabled mode** (default): each call site drops one
  `OnceLock<PerfCounters>::get()` per invocation. Saving compounds
  because these fire millions of times per `large-ts-repo` check.

No semantic change.

## Verification

- `cargo check -p tsz-checker` clean
- Pre-commit (fmt, clippy `-D warnings`, arch guard, full nextest
  suite) — to be confirmed by hook before push

## Conformance

No semantic change; counter values under `TSZ_PERF_COUNTERS=1` and
diagnostics-mode invariants are unaffected. Snapshots unchanged.
