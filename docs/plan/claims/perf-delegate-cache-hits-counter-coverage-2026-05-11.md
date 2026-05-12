# perf(checker): close cache-hits counter coverage gap for cross-arena delegations

- **Date**: 2026-05-11
- **Branch**: `perf/delegate-cache-hits-counter-coverage-2026-05-11`
- **PR**: TBD
- **Status**: claim
- **Workstream**: PERFORMANCE_PLAN.md §3 "Delegate recursion" counter bucket
  (`delegate_cross_arena_cache_hits_cross_file`), follow-on to #5061 which
  closed the `delegate_cross_arena_calls`/`max_recursion_depth` gap.

## Intent

`delegate_cross_arena_cache_hits_cross_file` was incremented in only one of
four cross-arena delegate sites — `delegate_cross_arena_symbol_resolution`
at `cross_file.rs:707`. The three sibling delegate functions also short-
circuit on a typed-cache hit but returned silently:

- `delegate_cross_arena_class_instance_type` (`cross_file.rs:1102-1108`)
- `delegate_cross_arena_interface_type` (`cross_file.rs:1245-1255`)
- `delegate_cross_arena_interface_member_simple_types` per-member loop
  (`cross_file.rs:1464-1472`)

That left `delegate.cache_hits_cross_file` in the attribution-mode
snapshot undercounting actual typed-cache hits. PERFORMANCE_PLAN.md §2
quotes "delegate.cache_hits_cross_file = 0 on cliff fixtures (~1100
calls, 0% hit)" as the highest-priority signal for T2.2 typed-query
migration — but that signal is taken from a counter that was only wired
on one of four cache-read sites. Closing the coverage gap is a prereq
for trusting any future T0.4 re-measurement.

## What changed

Each of the three sibling cache-read sites now increments the same
counter via the gate-once-cached pattern, mirroring the symbol-
resolution path:

```rust
if tsz_common::perf_counters::enabled_fast() {
    tsz_common::perf_counters::inc(
        &tsz_common::perf_counters::counters()
            .delegate_cross_arena_cache_hits_cross_file,
    );
}
```

The increment is placed inside the cache-hit branch, so it fires only
on the actual short-circuit path (not on misses that fall through to
child-checker construction).

## Cost model

| Build mode | Cost added per cache hit |
| --- | --- |
| `TSZ_PERF_COUNTERS` unset (default) | One cached-bool load (`enabled_fast()`). No `OnceLock` deref, no atomic increment. |
| `TSZ_PERF_COUNTERS=1` | One atomic `fetch_add` on `delegate_cross_arena_cache_hits_cross_file`. |

The default-release path is byte-identical aside from the `enabled_fast()`
gate load; the cache-hit semantics are unchanged.

## Verification

- `cargo check -p tsz-checker` — clean
- `cargo nextest run -p tsz-checker --lib` — to be run
- Pre-commit hook (fmt, clippy, arch guard, full nextest suite) — to be
  confirmed before push.
- LOC: `cross_file.rs` is at 1950 (well under the 2000-LOC arch guard).

## Conformance

No semantic change. Counter increments are observable only when counters
are enabled; the actual delegation behavior is byte-identical.
