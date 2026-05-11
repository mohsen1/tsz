# perf(checker): close delegate-counter gap for non-cross_file.rs child-checker sites

- **Date**: 2026-05-11
- **Branch**: `perf/delegate-counter-non-crossfile-2026-05-11`
- **PR**: TBD
- **Status**: claim
- **Workstream**: PERFORMANCE_PLAN.md §3 "Delegate recursion" counter bucket,
  third follow-on after #5061 (closed the gap for 3 sibling cross_file.rs
  paths) and #5064 (closed the cache-hits-cross_file counter gap for the
  same set).

## Intent

The aggregate `delegate_cross_arena_calls` counter and the
`enter_delegate()` RAII depth guard were wired only in `cross_file.rs`
paths (4 sites: symbol_resolution + class_instance + interface +
interface_member). The 4 sibling `CheckerCreationReason`s — explicitly
called out by the plan §15 as the next T2.2 migration targets —
construct child checkers via `with_parent_cache_attributed` *outside*
`cross_file.rs` and therefore never incremented the aggregate counter
nor updated the depth peak:

- `CheckerCreationReason::ExpandoProperty` (`expando.rs:393`)
- `CheckerCreationReason::CallableTruthiness` (`callable_truthiness.rs:332`)
- `CheckerCreationReason::CallHelpers` (`call_helpers.rs:765`, `:918`, `:1035`)
- `CheckerCreationReason::ImportType` (`import_type.rs:465`, `:525`)

That left `delegate.calls` and `delegate.max_recursion_depth` in the
attribution-mode snapshot undercount-ing actual cross-arena delegation
traffic. Per-reason child-checker construction counters were already
tracked via `CheckerCreationReason`, but the aggregate delegate counters
only saw the cross_file.rs slice.

## What changed

Each of the seven child-checker construction sites now begins with the
same gate-once-cached counter increment + RAII depth guard block:

```rust
if tsz_common::perf_counters::enabled_fast() {
    tsz_common::perf_counters::inc(
        &tsz_common::perf_counters::counters().delegate_cross_arena_calls,
    );
}
let _delegate_depth_guard = tsz_common::perf_counters::enter_delegate();
```

Per-call placement (not per-loop) is intentional: the depth guard's RAII
contract requires that the peak measured reflects each call's nesting
depth in isolation, not a stale max from an earlier iteration.

The gate-once-cached pattern means default-release (counter-disabled)
builds pay only the `enabled_fast()` cached-bool load — no `OnceLock`
deref and no atomic increment. `enter_delegate()` has its own
`enabled_fast()` short-circuit at the top of its body, so the depth
guard call also collapses to a near-no-op when disabled.

## Cost model

| Build mode | Cost added per delegate entry |
| --- | --- |
| `TSZ_PERF_COUNTERS` unset (default) | One cached-bool load (`enabled_fast()`) and one `enter_delegate()` call that short-circuits on the same gate. No atomic increment, no `OnceLock` deref. |
| `TSZ_PERF_COUNTERS=1` | One atomic `fetch_add` on the call counter + one thread-local `Cell` set in `enter_delegate()` + one `record_max` on the depth peak. |

## Verification

- `cargo check -p tsz-checker` — clean
- `cargo nextest run -p tsz-checker --lib` — to be run
- Pre-commit hook (fmt, clippy, arch guard, full nextest suite) — to be
  confirmed before push.

## Conformance

No semantic change. Counter increments are observable only when counters
are enabled; the actual delegation behavior is byte-identical.
