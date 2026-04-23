# `instantiate_type` cross-call cache — design

Status: **design only**. No production code changes associated with this document.
Companion to: `docs/plan/perf-large-repo-followup.md` §3.3.

## Executive summary

`TypeInstantiator` is constructed **per call** at every `instantiate_type(interner, type_id, substitution)` entry point
(`crates/tsz-solver/src/instantiation/instantiate.rs:1440`). Its `visiting` cache
(`instantiate.rs:203`) is scoped to one invocation, so two sibling callers with the same
`(type_id, substitution)` redo the full recursive walk. On `ts-essentials/deep-readonly.ts`
this shows up as a 16.56× gap vs `tsgo` which caches the result per unique shape.

The proposed fix is a cross-call memo cache keyed by `(TypeId, InternedSubstId, InstantiatorMode)`
that lives on `QueryCache` alongside the existing `eval_cache` and `application_eval_cache`.
Calls with identical keys return the cached `TypeId`; calls that tripped the depth/ERROR
guard are **not** cached (same discipline tsc uses for `recursionIdentity` failures).

## Current state

### `instantiate_type` entry points (`instantiate.rs:1440–1560`)
- `instantiate_type` — ordinary path (default flags).
- `instantiate_type_preserving` — `preserve_unsubstituted_type_params = true`.
- `instantiate_type_with_depth_status` — underlying primitive; returns the overflow flag.
- `instantiate_type_preserving_meta` — `preserve_meta_types = true`.
- `instantiate_type_with_infer` — `substitute_infer = true`.
- `substitute_this_type` (`instantiate.rs:1626`) — empty subst + `this_type = Some(..)`.

All five construct a fresh `TypeInstantiator::new(interner, &substitution)`
(`instantiate.rs:220`), then mutate up to four bool flags and an optional `this_type: TypeId`.
The `TypeInstantiator` struct fields are at `instantiate.rs:199–216`.

### Intra-call cache that exists today
`TypeInstantiator::visiting: FxHashMap<TypeId, TypeId>` (`instantiate.rs:203`) is the
per-invocation memo. It is primed on entry to `instantiate_inner` (`instantiate.rs:460, 484, 489`),
cloned/restored at shadowing-scope boundaries (`instantiate.rs:389–418`), and discarded when
the instantiator drops. Nothing persists across sibling `instantiate_type` calls.

### `QueryCache` — where a cross-call cache would live
`crates/tsz-solver/src/caches/query_cache.rs:275–308` already hosts:
- `eval_cache: RefCell<FxHashMap<(TypeId, bool), TypeId>>`
- `application_eval_cache: RefCell<FxHashMap<(DefId, SmallVec<[TypeId;4]>, bool), TypeId>>`
- `element_access_cache`, `object_spread_properties_cache`, `property_cache`,
  `variance_cache`, `canonical_cache`, `intersection_merge_cache`.

`SharedQueryCache` (`query_cache.rs:79–98`) wraps three of these in `DashMap` for
cross-file benefit.

### Partial caching already in place
- `application_eval_cache` memoizes **evaluated** generic applications
  keyed by `(DefId, args, no_unchecked_indexed_access)` at `query_cache.rs:1201–1229`.
  This is a *different* layer: it caches the post-evaluation result of
  `Application(Lazy(DefId), args)` being fully expanded. It does **not** cache
  raw `instantiate_type` (which is pre-evaluation substitution).
- Architecture contract test already reserves the name `InstantiationCache`
  (`crates/tsz-checker/src/tests/architecture_contract_tests.rs:2259`), meaning the
  checker is already forbidden from reaching into this cache — a clean slate for the solver.

### `TypeSubstitution` (`instantiate.rs:28–196`)
```rust
pub struct TypeSubstitution {
    map: FxHashMap<Atom, TypeId>,
}
```
Both `Atom` (`crates/tsz-common/src/interner/mod.rs:20`) and `TypeId` are `Copy` `u32`s,
`Eq + Hash`. The map is small: most substitutions have 1–4 entries (matching the
`SmallVec<[TypeId; 4]>` shape chosen in the existing application cache).

## Design

### 1. Cache key

```text
InstantiationCacheKey = (TypeId, InternedSubstId, u8 mode_bits)
```

`InternedSubstId` is a new `u32` handle backed by a content-keyed interner on
`TypeInterner`. Two `TypeSubstitution`s with the same `(name, type_id)` multiset
must map to the same `InternedSubstId`. Implementation shape:

```text
struct SubstKey(SmallVec<[(Atom, TypeId); 4]>);  // sorted by Atom, dedup-free
```

Sorting by `Atom` produces a canonical form so `{"T": u32, "U": f64}` and
`{"U": f64, "T": u32}` hit the same slot. Sorting is O(k log k) for k ≤ ~4
in practice; this is trivially cheaper than the recursive walk being cached.

`mode_bits` packs the four instantiator booleans plus a `this_type` discriminator:
```
bit 0: substitute_infer
bit 1: preserve_meta_types
bit 2: preserve_unsubstituted_type_params
bit 3: has_this_type
```
When `has_this_type` is set, the `this_type: TypeId` must also be part of the key.
Cleanest encoding: make the key `(TypeId, InternedSubstId, u8, Option<TypeId>)`
so `ThisType`-substitution calls don't share cache slots with non-`this` calls.

### 2. Storage and ownership

New file: `crates/tsz-solver/src/caches/instantiation_cache.rs`, containing
`InstantiationCache` with the same `RefCell<FxHashMap<_, _>>` pattern as `eval_cache`.
Add it as a field on `QueryCache`:

```text
instantiation_cache: RefCell<FxHashMap<InstantiationCacheKey, TypeId>>,
```

Add its entry count to `QueryCacheStatistics` (`query_cache.rs:127`),
`clear()` (`query_cache.rs:350`), and `estimated_size_bytes`.

Expose two methods through `QueryDatabase` (`caches/db.rs`), mirroring the existing
`lookup_application_eval_cache`/`insert_application_eval_cache` pattern at
`db.rs:724–741`:

```text
fn lookup_instantiation_cache(&self, key: InstantiationCacheKey) -> Option<TypeId>;
fn insert_instantiation_cache(&self, key: InstantiationCacheKey, result: TypeId);
```

Defaults return `None` / no-op so non-`QueryCache` databases (raw `TypeInterner`,
tests) don't need the cache. `TypeInstantiator` is already parameterized by
`&dyn TypeDatabase`, so upgrading it to probe the cache is a matter of having
`TypeDatabase` expose the lookup methods (or a supertrait). Prefer promoting the
two methods onto `TypeDatabase` with default `None`/no-op — matches the existing
`lookup_application_eval_cache` precedent.

**Per-call vs project-wide.** Start with per-`QueryCache` (per-file). Optionally add
`SharedQueryCache` instantiation cache later if profiling shows cross-file wins
(similar to what `eval_cache` does). Start single-threaded; the utility-type blow-up
targets are single-file.

### 3. Invariants and things that must **not** be cached

1. **`depth_exceeded == true`.** When `instantiator.depth_exceeded` flips, the returned
   `TypeId::ERROR` is a cycle/fuel artifact, not a true result. **Skip the insert** in
   that case (`instantiate.rs:1494–1498, 1512–1516, 1536–1540, 1555–1559`).
2. **Incomplete cycle results.** `visiting.insert(type_id, type_id)` at
   `instantiate.rs:484` is a placeholder used during recursion. Only the *final*
   return at `instantiate.rs:489` goes through the instantiator's public return,
   so cache insertion at the entry-point layer (outside `instantiator.instantiate`)
   automatically skips placeholders. No special logic needed as long as caching
   happens in the five module-level convenience functions, not inside `instantiate_inner`.
3. **Lazy `DefId` resolution.** `instantiate_type` currently runs with
   `NoopResolver` semantics inside the instantiator — it does **not** resolve
   `Lazy(DefId)` during the walk. Two `TypeId`s with the same structure but
   different underlying `Lazy(DefId)` already intern to **different** `TypeId`s
   because `TypeData::Lazy(DefId)` is part of the intern key. So caching on
   `TypeId` alone is sound: DefId-shape divergence cannot alias.
4. **Conditional types with `extends` state.** Conditionals are interned by
   `ConditionalType` (`types.rs:713`) which includes both branches. Instantiation
   substitutes into the *unevaluated* conditional structure; it never decides which
   branch to take (that is `evaluate_type`'s job, which has its own cache). Safe.
5. **Fresh type parameters.** `enter_shadowing_scope` creates fresh type params
   (`instantiate.rs:400` via `interner.type_param(*tp)`). These are *interned*
   by `TypeParamInfo`, so two calls produce the same `TypeId` — the substitution
   result does not depend on a per-call fresh identity. Safe.
6. **Error types.** `TypeId::ERROR` substituted into a position yields a final
   type containing `ERROR`. Caching that is fine; it's identical to what a
   re-walk would produce. Not the same as #1.
7. **Fuel exhaustion.** `consume_evaluation_fuel` is an evaluator concern, not
   an instantiator concern. No interaction.

### 4. Invalidation

None required for the common case. `TypeId` values are stable once interned;
`TypeSubstitution` values are hashed by content. New types being interned
do **not** invalidate existing entries.

Edge case: if the checker ever retracts or reassigns a `DefId → TypeId` mapping
in `TypeEnvironment`, the cache would be stale. Today this does not happen within
a single `QueryCache` lifetime. `QueryCache::clear()` (`query_cache.rs:350`)
already drops all caches together, which is the correct coarse-grained invalidation
boundary for rebinding.

### 5. Implementation plan — 4 PRs

**PR 1 — Content-hashable `TypeSubstitution` + interner.**
- In `instantiate.rs`, derive a `canonical_entries() -> SmallVec<[(Atom, TypeId); 4]>`
  method that returns a sorted copy of the map.
- Implement `Hash`/`PartialEq`/`Eq` on a new `CanonicalSubst` wrapper (not on
  `TypeSubstitution` itself — keep its `FxHashMap` implementation detail).
- Add `TypeInterner::intern_substitution(&CanonicalSubst) -> InternedSubstId`
  alongside the existing list/shape interners.
- No behavior change yet. Unit tests: two structurally-equal substitutions hash
  equal; map order does not affect identity.

**PR 2 — `InstantiationCache` storage and trait methods.**
- Add `crates/tsz-solver/src/caches/instantiation_cache.rs` with
  `InstantiationCacheKey` + `InstantiationCache` + `RefCell<FxHashMap>`.
- Add `lookup_instantiation_cache` / `insert_instantiation_cache` to
  `TypeDatabase` with `None`/no-op defaults (mirror `lookup_application_eval_cache`
  at `db.rs:724–741`).
- Implement them on `QueryCache`.
- Extend `QueryCacheStatistics` (+ display, + `estimated_size_bytes`, + `clear`).
- Architecture test already blocks checker access (`architecture_contract_tests.rs:2259`);
  keep it that way.
- No behavior change yet (cache exists, no one writes to it).

**PR 3 — Wire cache at the five entry points.**
- In each of `instantiate_type`, `instantiate_type_preserving`,
  `instantiate_type_preserving_meta`, `instantiate_type_with_infer`,
  `substitute_this_type`: before constructing the `TypeInstantiator`, build the
  cache key and probe. On hit, return early. After the walk, insert **only if**
  `!instantiator.depth_exceeded` **and** no intermediate `ERROR` was returned from
  the overflow path.
- Do **not** cache when `substitution.is_empty() || substitution.is_identity(...)` —
  those are already free.
- Gate behind `#[cfg]`-free runtime check; no flag needed. Add a stats counter
  (`instantiation_cache_hits` / `_misses` on `QueryCache`, like the subtype counters
  at `query_cache.rs:298–301`).
- Expected win: the recursive utility-type cases in
  `docs/plan/perf-large-repo-followup.md` §2.
- Verify: `bench-vs-tsgo` specifically `ts-essentials/deep-readonly.ts`,
  `ts-essentials/paths.ts`, `ts-essentials/deep-pick.ts`,
  `ts-toolbelt/Any/Compute.ts`.

**PR 4 — (Optional) Shared cross-file cache.**
- Add `instantiation_cache: DashMap<InstantiationCacheKey, TypeId>` to
  `SharedQueryCache`. Mirror the L1/L2 pattern of `eval_cache`
  (`query_cache.rs:1120–1125, 1181–1183`).
- Gate on measured cross-file hit rate from PR 3 stats. Skip if the single-file
  cache already captures the wins.

Each PR is independently shippable. PR 1 and 2 are pure refactors with no
behavior change; PR 3 is where the perf win lands.

## Test strategy

All tests in `crates/tsz-solver/src/tests/` (or alongside the new module).
Run via `cargo nextest run -p tsz-solver --lib` per CLAUDE.md §19.5.

1. **Equal-substitution hit test.** Construct two `TypeSubstitution`s with the
   same contents in different insertion order. Assert the second
   `instantiate_type` call hits the cache (via the new `instantiation_cache_hits`
   counter) and returns an identical `TypeId`.

2. **Distinct-substitution miss test.** `{"T": number}` vs `{"T": string}` over
   the same `type_id` must produce different cached entries and different results.

3. **Recursive utility-type parity.** For a `DeepReadonly<T>`-style fixture,
   run `instantiate_type` twice with the same args; result `TypeId` must equal
   the uncached baseline (call once with cache, once through a fresh
   `TypeInterner` without cache, compare).

4. **Depth-exceeded not cached.** Build a pathological input that trips
   `MAX_INSTANTIATION_DEPTH` (`instantiate.rs:24`). First call returns
   `TypeId::ERROR`; second call must also trigger the overflow (i.e., not
   return `ERROR` from a poisoned cache entry). Verify by checking
   `instantiation_cache.len() == 0` after two calls.

5. **Mode-bit isolation.** Same `(type_id, substitution)` called via
   `instantiate_type` vs `instantiate_type_preserving` must **not** collide —
   they return different results for the same input.

6. **`this_type` isolation.** `substitute_this_type(t, Builder)` and
   `substitute_this_type(t, Derived)` must have distinct entries.

7. **Empty/identity short-circuit preserved.** Existing fast paths
   (`instantiate.rs:1488, 1507, 1530, 1549`) must run before the cache probe
   so empty/identity subs remain zero-cost.

8. **Intern stability under cache.** Construct a substitution, mutate it
   (`.insert`), re-canonicalize, and confirm the `InternedSubstId` changes
   — prevents accidental `&TypeSubstitution` capture by ID after mutation.

## Risks and mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Cache key aliases two non-identical `(ty, subst, mode)` triples | **Corrupts type identity across the pipeline.** Produces spurious conformance failures that look random. | (a) Make mode bits total (no optional fields hidden outside the key). (b) Run the full conformance suite after PR 3 — if the cache is wrong the big3 count moves. (c) Add a debug-assertion mode that recomputes uncached and asserts equality on every hit. |
| `TypeSubstitution` sort is not canonical | Hash mismatch → cache never hits → no corruption, just no win | Unit test #1 above is the tripwire. |
| `depth_exceeded` result accidentally cached | A transient cycle error becomes permanent; later calls return `ERROR` instead of retrying | Insert guarded by `!depth_exceeded`. Unit test #4. |
| `Lazy(DefId)` resolves differently between two `QueryCache` instances | Cross-file cache returns wrong result | Start single-file (no `SharedQueryCache` write in PR 3). Measure before enabling PR 4. |
| Memory growth on large repos | Another OOM risk on `large-ts-repo` | Report cache size via `estimated_size_bytes`. Cap entries or evict on `clear()` if growth exceeds a threshold in profiling. |
| Fresh-identity concerns under shadowing (`enter_shadowing_scope`) | Cache returns a `TypeId` that references a shadowed type param interned elsewhere | Shadowing only affects the instantiator's `shadowed`/`local_type_params` lists; the returned `TypeId` is interned on the shared `TypeInterner`. Same-shape ⇒ same `TypeId` is already the intern contract. Unit test #3 covers this. |

## Non-goals

- Caching inside `TypeInstantiator::instantiate_inner` (the per-call `visiting`
  map stays; this design is only about cross-call sharing).
- `SharedQueryCache` instantiation caching — explicit PR 4 follow-up.
- Evaluation caching for post-substitution types — already covered by
  `eval_cache` and `application_eval_cache`.
- Changing the entry-point function shapes — the public signatures stay
  `(interner, type_id, substitution) -> TypeId`.

## Files referenced

- `crates/tsz-solver/src/instantiation/instantiate.rs:28–196` — `TypeSubstitution` struct.
- `crates/tsz-solver/src/instantiation/instantiate.rs:199–216` — `TypeInstantiator` struct.
- `crates/tsz-solver/src/instantiation/instantiate.rs:220–235` — `TypeInstantiator::new`.
- `crates/tsz-solver/src/instantiation/instantiate.rs:432–492` — `instantiate` / `instantiate_inner`.
- `crates/tsz-solver/src/instantiation/instantiate.rs:1440–1651` — 5 entry points + `substitute_this_type`.
- `crates/tsz-solver/src/caches/query_cache.rs:275–308` — `QueryCache` struct.
- `crates/tsz-solver/src/caches/query_cache.rs:1201–1229` — existing `application_eval_cache` precedent.
- `crates/tsz-solver/src/caches/db.rs:724–741` — `lookup_application_eval_cache` trait default precedent.
- `crates/tsz-checker/src/tests/architecture_contract_tests.rs:2256–2259` — `InstantiationCache` already architecturally blessed.
- `docs/plan/perf-large-repo-followup.md` §3.3 — motivation and expected impact.
