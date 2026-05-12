# perf(checker): close `delegate_cross_arena_misses` counter coverage gap

- **Date**: 2026-05-11
- **Branch**: `perf/delegate-misses-counter-coverage-2026-05-11`
- **PR**: TBD
- **Status**: claim
- **Workstream**: PERFORMANCE_PLAN.md §3 "Delegate recursion" counter bucket,
  fourth follow-on after #5061, #5064, #5069.

## Intent

`delegate_cross_arena_misses` is the counter that lets attribution-mode
runs compute the cache-effectiveness ratio for cross-arena delegation:

```
miss_rate ≈ delegate_cross_arena_misses / delegate_cross_arena_calls
```

After #5061 wired the `calls` counter into the 3 sibling cross_file.rs
delegate paths and #5069 wired it into the 4 non-cross_file.rs reasons
(ExpandoProperty / CallableTruthiness / CallHelpers / ImportType), the
`calls` counter is now correct across all 11 construction sites. The
`misses` counter, however, was still wired only in the original
`delegate_cross_arena_symbol_resolution` slice (cross_file.rs:774). That
makes the miss_rate ratio meaningless: the numerator covers one
construction path, the denominator covers eleven. Any future T0.4
attribution run reporting "miss rate" would mis-route prioritisation.

## What changed

Each of the 10 sites where #5061/#5069 already added the `calls`
counter now also increments `delegate_cross_arena_misses` in the same
`enabled_fast()` block, inside the cache-miss branch (for the 4
cross_file.rs paths that have a cache fast-path) or unconditionally
(for the 4 non-cross_file.rs paths that have no cache fast-path —
every entry is a miss).

Sites covered (8 outside cross_file.rs + 3 inside, all *in addition to*
the pre-existing symbol_resolution site):

- `cross_file.rs` (3 siblings of symbol_resolution):
  - `delegate_cross_arena_class_instance_type` (`cross_file.rs:1158`)
  - `delegate_cross_arena_interface_type` (`cross_file.rs:1323`)
  - `delegate_cross_arena_interface_member_simple_types` (`cross_file.rs:1564`)
- non-cross_file.rs paths:
  - `expando.rs:399` (ExpandoProperty)
  - `callable_truthiness.rs:338` (CallableTruthiness)
  - `call_helpers.rs:771`, `:928`, `:1049` (CallHelpers, 3 sites)
  - `import_type.rs:471`, `:541` (ImportType, 2 sites)

The per-call placement matches `delegate_cross_arena_calls`. The
semantics — "we reached actual child-checker construction, no cache
short-circuited us" — is consistent with the original symbol_resolution
site (cross_file.rs:774): the increment happens right before
`Box::new(CheckerState::with_parent_cache_attributed(...))`.

## Cost model

| Build mode | Cost added per delegate entry |
| --- | --- |
| `TSZ_PERF_COUNTERS` unset (default) | Zero (existing `enabled_fast()` gate already covers the new increment). |
| `TSZ_PERF_COUNTERS=1` | One additional atomic `fetch_add` per construction site. |

The default-release path is byte-identical to post-#5069. The
counter-enabled path adds one extra atomic increment per delegate
construction.

## Why this completes the trio

`delegate.calls` + `delegate.misses` + `delegate.cache_hits_*` form a
self-consistent set:

```
calls = misses + cache_hits_cross_file + cache_hits_lib
```

After this PR, all 11 construction sites contribute correctly to
`calls` (post-#5061/#5069) and to `misses` (this PR). The cross_file
cache hits are correctly counted at all 4 typed-cache read sites
(post-#5064). The lib cache hits are only checked in the
symbol_resolution path (which is fine; the other paths don't have a
lib_delegation_cache fast-path). The identity above therefore holds at
every site, making attribution-mode bench output fully self-consistent.

## Verification

- `cargo check -p tsz-checker` — clean
- Pre-commit hook (fmt, clippy, arch guard, full nextest suite) — to be
  confirmed before push.

## Conformance

No semantic change. Counter increments are observable only when counters
are enabled; the actual delegation behavior is byte-identical.
