# perf(checker): close delegate-counter coverage gap for cross-arena delegations

- **Date**: 2026-05-10
- **Branch**: `perf/delegate-counter-coverage-2026-05-10`
- **PR**: TBD
- **Status**: claim
- **Workstream**: PERFORMANCE_PLAN.md §3 "Delegate recursion" counter bucket

## Intent

The `delegate_cross_arena_calls` counter and the `enter_delegate()` RAII
depth guard were wired in `delegate_cross_arena_symbol_resolution` only.
Three sibling delegate functions construct child checkers but never
incremented the call counter or updated `delegate_max_recursion_depth`:

- `delegate_cross_arena_class_instance_type` (`cross_file.rs:1140`)
- `delegate_cross_arena_interface_type` (`cross_file.rs:1287`)
- `delegate_cross_arena_interface_member_simple_type[s]` (`cross_file.rs:1510`)

That left the `delegate.calls` and `delegate.max_recursion_depth`
counters in `PerfCounterSnapshot::delegate` undercount-ing actual
cross-arena delegation traffic — per-reason child-checker construction
counters were already tracked via `CheckerCreationReason`, but the
aggregate delegate counter only saw the symbol-resolution slice.

## What changed

Each of the three child-checker construction sites now begins with the
same gate-once-cached counter increment + RAII depth guard block:

```rust
if tsz_common::perf_counters::enabled_fast() {
    tsz_common::perf_counters::inc(
        &tsz_common::perf_counters::counters().delegate_cross_arena_calls,
    );
}
let _delegate_depth_guard = tsz_common::perf_counters::enter_delegate();
```

The gate-once-cached pattern means default-release (counter-disabled)
builds pay only the `enabled_fast()` cached-bool load — no `OnceLock`
deref and no atomic increment. `enter_delegate()` has its own
`enabled_fast()` short-circuit at the top of its body, so the depth
guard call also collapses to a near-no-op when disabled.

## Cost model

| Build mode | Cost added per delegate entry |
| --- | --- |
| `TSZ_PERF_COUNTERS` unset (default) | One cached-bool load (`enabled_fast()`) and one `enter_delegate()` call that short-circuits on the same gate. No atomic increment, no `OnceLock` deref. |
| `TSZ_PERF_COUNTERS=1` | One atomic `fetch_add` + one thread-local `Cell` set in `enter_delegate()` + one `record_max` on the depth peak. |

## Verification

- `cargo check -p tsz-checker` — clean
- `cargo nextest run -p tsz-checker --lib` — to be run
- Pre-commit hook (fmt, clippy, arch guard, full nextest suite) — to be
  confirmed before push.
- LOC: `cross_file.rs` is at 1932 (well under the 2000-LOC arch guard).

## Conformance

No semantic change. Counter increments are observable only when counters
are enabled; the actual delegation behavior is byte-identical.
