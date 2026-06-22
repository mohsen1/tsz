# Caches, Object Types, Contextual Typing, and the Compatibility Model

This document covers the solver's "supporting infrastructure" tier: the
memoization layer that makes a from-scratch type checker fast enough to compete
with `tsc` and `tsgo`, the in-memory representation of object/interface/class
types and their members, the reverse-inference (contextual typing) machinery,
the `type_queries` API surface the checker uses to interrogate types without
matching `TypeData` directly, the depth/fuel limits that bound the recursive
kernels, and the visitor utilities every walker is built on. It closes with the
**compatibility model** — the Judge/Lawyer split that distinguishes pure
structural subtyping from TypeScript's unsound legacy quirks
(`any`/variance/excess/freshness/void-return/weak-type).

These are the modules under `crates/tsz-solver/src/caches`, `objects`,
`contextual`, `type_queries`, `limits`, `visitors`, and `utils`, plus the
`relations/compat.rs`, `relations/lawyer.rs`, and `relations/freshness.rs`
files that own the Lawyer layer. The relation *kernel* itself (the structural
walk) is covered in [solver-relations](solver-relations.md); this doc focuses on
the caches that store relation verdicts, the object shapes those verdicts
compare, and the legacy-compat rules layered on top.

## Owns / Must not own

**Owns:**

- The concrete per-file query database (`QueryCache` in
  `caches/query_cache.rs`) and the cross-file shared store (`SharedQueryCache`
  in `caches/shared_query_cache.rs`): every memo of evaluation, relation,
  property, element-access, instantiation, and subtype-reduction results.
- The cache *keys* and their invalidation discipline: which compiler-option
  flags participate, why some caches are per-file and some shared, and the
  `DefId`-keyed dependency index that invalidates application-eval entries on
  body re-registration.
- The in-memory object representation (`ObjectShape`, `PropertyInfo`,
  `IndexSignature`, `ObjectFlags`) and the algorithms that read it:
  intersection property collection (`objects/collect.rs`), apparent/primitive
  members (`objects/apparent.rs`), index-signature resolution
  (`objects/index_signatures.rs`), and element access
  (`objects/element_access.rs`).
- Contextual typing / reverse inference (`contextual/core.rs`,
  `contextual/extractors.rs`): extracting an expected type for a sub-expression
  from an outer expected type.
- The `type_queries` query boundary: a stable, `TypeData`-abstracting API
  (`is_tuple_type`, `is_callable_type`, `classify_*`, …) the checker calls
  instead of pattern-matching solver internals.
- The depth/fuel limit policy (`limits/mod.rs`) and the visitor/traversal
  utilities (`visitors/`, `utils/`).
- The Lawyer (`CompatChecker`, `AnyPropagationRules`) and freshness tracking
  (`relations/freshness.rs`).

**Must not own:**

- The structural subtype walk itself — that is the Judge (`SubtypeChecker`),
  documented in [solver-relations](solver-relations.md). The caches *store* its
  verdicts; the Lawyer *wraps* it.
- Meta-type evaluation (`keyof`, conditional, mapped, indexed-access reduction)
  — that is the `TypeEvaluator`, see [solver-evaluation](solver-evaluation.md).
  These modules *call* `evaluate_type` and cache its results.
- Generic inference and instantiation — see
  [solver-inference](solver-inference.md) and
  [solver-instantiation](solver-instantiation.md). The instantiation cache
  here is storage; the algorithm lives there.
- Source locations, AST identity, and diagnostic rendering — those belong to
  the checker ([checker-context-and-state](checker-context-and-state.md),
  [checker-error-reporter-diagnostics](checker-error-reporter-diagnostics.md)).
- Raw `TypeKey` construction for policy decisions. The checker reaches these
  modules through `type_queries` and the `TypeDatabase`/`QueryDatabase` traits,
  never by interning raw keys.

## Module map

| Path | Role |
| --- | --- |
| `caches/query_cache.rs` | `QueryCache<'a>` — the concrete per-file `QueryDatabase`: wraps `&TypeInterner` with `RefCell`/`Cell` memos for evaluation, relation, property, element-access, instantiation, and BCT subtype-reduction queries. |
| `caches/db.rs` | The trait stack: `TypeDatabase`, `QueryDatabase`, `TypeApplicationEvalCache`, `TypeWidenCache`, `TypePredicateCache`, `TypeCompilerOptions`. Defines the cache *capability* surface (with no-op defaults so raw `TypeInterner` opts out). |
| `caches/shared_query_cache.rs` | `SharedQueryCache` — thread-safe `DashMap`-backed cross-file store for `eval`, `subtype`, `assignability`, and (experimentally) application-eval/instantiation caches. |
| `caches/instantiation_cache.rs` | `InstantiationCache` + `InstantiationCacheKey`/`CanonicalSubst` — cross-call memo for `instantiate_type`. |
| `caches/subtype_reduction_cache.rs` | `SubtypeReductionCache` + `SubtypeReductionKey`/`SortedTypeIds` — memo for `remove_subtypes_for_bct` (the BCT O(N²) loop). |
| `caches/application_eval_index.rs` | `ApplicationEvalDependencyIndex` — `DefId -> {ApplicationEvalCacheKey}` reverse index that drives per-`DefId` invalidation. |
| `caches/options.rs` | `solver_options!` macro + `IndexAccessOptions` — the shared `{no_unchecked_indexed_access, exact_optional_property_types}` flag pair embedded in cache keys. |
| `caches/query_cache_statistics.rs`, `query_trace.rs`, `query_cache_*.rs` | Cache stats snapshots, `--extendedDiagnostics`-style counters, and per-query relation tracing. |
| `objects/literal.rs` | `ObjectLiteralBuilder` — constructs object-literal `TypeId`s (the only object *construction* surface in this group). |
| `objects/collect.rs` | `collect_properties` / `collect_properties_cached` — intersection property merging with cross-collector recursion guards. |
| `objects/apparent.rs` | Apparent/boxed members for primitives (`apparent_primitive_members`, `index_receiver_apparent_type`). |
| `objects/index_signatures.rs` | `IndexSignatureResolver`, `IndexKind` — string/number index-signature resolution. |
| `objects/element_access.rs` | `ElementAccessEvaluator`, `ElementAccessResult` — structured `obj[k]` evaluation with error classification. |
| `contextual/core.rs` | `ContextualTypeContext`, `apply_contextual_type`, `rest_argument_element_type`. |
| `contextual/extractors.rs` | The visitor-based extractors (`ParameterExtractor`, `PropertyExtractor`, `ArrayElementExtractor`, …). |
| `type_queries/` | The `TypeData`-abstracting query API (`classifiers`, `extended`, `shape_queries`, `mapped`, `flow`, `traversal`, …). |
| `limits/mod.rs` | All named depth/fuel constants and the `LimitBudgets` thread-local. |
| `visitors/visitor.rs` | `TypeVisitor` trait + `for_each_child` / `walk_referenced_types`. |
| `visitors/child_policy.rs` | `ChildPolicy` + `try_for_each_child_with_policy` — the single policy-parameterized child enumerator. |
| `visitors/visitor_predicates.rs` | Canonical `is_*_type` predicates re-exported by `type_queries`. |
| `utils/mod.rs` | Cross-cutting helpers (`union_or_single`, `classify_tuple_arity`, param/element counting, numeric-name checks). |

## The cache database trait stack

The solver never talks to storage directly; it talks to a trait. The base trait
is `TypeDatabase` (`caches/db.rs`, line 518): `intern`, `lookup`,
`object_shape`, `type_list`, `function_shape`, `conditional_type`,
`mapped_type`, atom interning, and the like. `TypeDatabase` is a supertrait
composition that bundles several narrow cache capabilities so the broad query
trait stays under its method cap (#8205):

```
TypeDatabase
  : TypePredicateCache        // type-predicate narrowing memo
  + TypeTupleLimitSignal       // tuple-too-large signalling
  + TypeDisplayProvenance      // display-properties / alias side-tables
  + TypeCompilerOptions        // no_unchecked_indexed_access, eopt, ...
  + TypeApplicationEvalCache   // application-eval + eval-memo + closed-eval
  + TypeWidenCache             // widen_type memo
```

`QueryDatabase` (`caches/db.rs`, line 1394) extends `TypeDatabase` with
`TypeResolver` and `CollectPropertiesResultCache`. The crucial design property
is that **every cache hook has a no-op default**. A bare `&TypeInterner`
implements `TypeDatabase` but returns `None`/no-op for every cache method, so it
always misses and never updates counters. Only `QueryCache` overrides them with
real storage. This is what lets the solver run the same algorithm with caching
(production, `Some(&dyn QueryDatabase)`) or without (tests, raw interner) without
two code paths — the cache-aware instantiation entry points pass
`Some(&dyn QueryDatabase)`; plain `instantiate_type` passes `None`
(`caches/instantiation_cache.rs`, module doc).

```
checker ── &dyn QueryDatabase ──▶ QueryCache<'a> ──▶ &'a TypeInterner
                                      │  (RefCell memos, single-threaded)
                                      └── Option<&'a SharedQueryCache>
                                              (DashMap, cross-file)
```

## QueryCache: the per-file memo bundle

`QueryCache<'a>` (`caches/query_cache.rs`, line 220) holds one
`&'a TypeInterner` plus roughly a dozen `RefCell`/`Cell` caches. The choice of
`RefCell`/`Cell` over `RwLock`/atomics is deliberate and documented at the
struct: a `QueryCache` borrows `&TypeInterner` and is **single-threaded** — one
per file-checker, never crossed by Rayon workers — so `RefCell::borrow()` (a
plain integer check) beats `RwLock::read()` (an atomic CAS) on every subtype
check, property lookup, and eval hit.

The owned caches (constructor at line 301):

| Field | Key | Value | Purpose |
| --- | --- | --- | --- |
| `eval_cache` | `EvaluationCacheKey = (TypeId, nuia, eopt)` | `TypeId` | Top-level + nested type-evaluation memo (conditional/mapped/indexed reduction). |
| `closed_eval_cache` | same | `TypeId` | Substitution-independent evaluation of *closed* types (no free params/`this`/`infer`/type-query); reusable project-wide across fresh evaluator instances. |
| `application_eval_cache` | `(DefId, SmallVec<[TypeId;4]>, nuia, eopt)` | `TypeId` | Evaluated generic applications (`Promise<T>`, `Awaited<T>`, …). |
| `application_eval_dependency_index` | `DefId -> {ApplicationEvalCacheKey}` | — | Reverse index for per-`DefId` invalidation. |
| `element_access_cache` | `(TypeId, TypeId, Option<u32>, bool)` | `TypeId` | `obj[k]` result types. |
| `object_spread_properties_cache` | `TypeId` | `Vec<PropertyInfo>` | Spread-property expansion. |
| `collect_properties_result_cache` | `(TypeId, resolver_generation)` | `PropertyCollectionResult` | Context-free intersection property collection. |
| `subtype_cache` | `RelationCacheKey` | `RelationCacheValue` | Subtype verdicts. |
| `assignability_cache` | `RelationCacheKey` | `RelationCacheValue` | Assignability verdicts (kept *separate* so loose assignability cannot poison strict subtype). |
| `property_cache` | `(TypeId, Atom, bool, bool)` | `PropertyAccessResult` | Property lookups. |
| `variance_cache` | `DefId` | `Arc<[Variance]>` | Computed variance masks for generic defs. |
| `canonical_cache` | `TypeId` | `TypeId` | Canonical id for structurally identical types. |
| `intersection_merge_cache` | `TypeId` | `Option<TypeId>` | Intersection-to-merged-object results. |
| `instantiation_cache` | `InstantiationCacheKey` | `TypeId` | `instantiate_type` cross-call memo. |
| `subtype_reduction_cache` | `SubtypeReductionKey` | `Arc<[TypeId]>` | `remove_subtypes_for_bct` (BCT) memo. |

Each cache (or cache group) carries a `CacheCounter` or `SharedCacheCounter`
(hits/misses, plus shared-tier hits/misses/inserts), surfaced through
`QueryCache::statistics()` as a `QueryCacheStatistics` and through
`relation_cache_stats()` as a `RelationCacheStats`.

### Why options are in the key (the #10970 footgun)

`EvaluationCacheKey`, `ApplicationEvalCacheKey`, and the element-access key all
embed `no_unchecked_indexed_access` and `exact_optional_property_types`. This is
not optional: evaluating a generic application can expand a *homomorphic* mapped
type whose optional-modifier stripping depends on `exactOptionalPropertyTypes`,
and indexed access of optional/array members depends on
`noUncheckedIndexedAccess`. Omitting a policy-affecting flag from a cache key is
a correctness footgun — the same `TypeId` would return a result computed under
the wrong option. `caches/options.rs` exists precisely to make this hard to get
wrong: the `IndexAccessOptions` newtype bundles the two flags and derives
`Hash`/`Eq` so both `EvaluationCacheKey` and `NarrowTypeCacheKey` hash them
byte-identically, and a new index-access option lands in *one* place.

Note the deliberate counter-example documented at the element-access key
(`caches/query_cache.rs`, line 45): element (indexed) access of an optional
property includes `undefined` under *both* `exactOptionalPropertyTypes`
settings (matching `tsc`), so that result does **not** depend on the flag and it
is intentionally left out of `ElementAccessTypeCacheKey`.

### The relation cache: definitive vs. fuel-conditional

Relation verdicts are stored as `RelationCacheValue` (`types.rs`, line 554),
which is *not* a plain `bool`:

- `True` / `False` — definitive, budget-independent verdicts.
- `LimitTrue { fuel_band }` — a `tsc` `Ternary.Maybe`-style *assumed-related*
  verdict, produced when a relation chain exhausted the global subtype fuel
  budget. This is **only honest** for a later query whose remaining fuel at
  lookup time is `<= fuel_band` (the budget the recorded run started with). A
  query holding a *larger* budget could complete the comparison honestly, so it
  must recompute. `QueryCache::limit_true_usable` (line 519) enforces this:
  `limit_result_cache_enabled() && remaining_global_subtype_fuel() <= fuel_band`.

`lookup_policy_relation_cache_value` (line 463) returns any found entry as a hit;
`lookup_policy_relation_cache` (line 493) collapses it through
`RelationCacheValue::as_definitive`, which yields `None` for `LimitTrue`. The
dispatch in `is_cached_policy_relation` (line 524) treats a usable `LimitTrue`
as a real hit, an unusable `LimitTrue` (or a miss) as "recompute and let the
definitive insert overwrite the truncated entry."

The `RelationCacheKey` (`types.rs`, line 427) keys on `source`, `target`, a
`RelationCacheKind` (`Subtype` / `Assignable` / `CheckerAssignable` /
`Identical`), a `RelationCacheConfig` (the relevant option flags), and a
`this_context` `TypeId`. That last field (#13828) discriminates a verdict by the
resolved polymorphic-`this` receiver: a pair carrying `ThisType` is valid only
under one binding, so encoding the resolved binding lets such verdicts live in
the *shared* cross-checker cache without poisoning a sibling that compares the
same `(source, target)` under a different receiver. For every non-`this` pair it
is `TypeId::NONE`, so ordinary keys are byte-identical to the old protocol.

A fast path (`relation_fast_path`, line 416) short-circuits identity / top
(`target == UNKNOWN`, `source == ANY` or `target == ANY`) / bottom
(`source == NEVER`, `ERROR` either side -> `true`; `target == NEVER` -> `false`)
*before* any key construction or `RefCell` borrow — these are the dominant cheap
cases and skipping the cache machinery for them is a measurable win.

## Cross-file sharing: SharedQueryCache

In a multi-file project (e.g. ts-toolbelt's 242 files), every file-checker gets
its own `QueryCache`. Without sharing, sibling checkers re-derive the same deep
mapped/conditional relation subtrees in every file. `SharedQueryCache`
(`caches/shared_query_cache.rs`, line 53) is a `DashMap`-backed store consulted
*on local miss* and written-through on local insert.

Only the highest-impact caches are shared:

- `eval_cache` — type evaluation results.
- `subtype_cache` and `assignability_cache` — relation verdicts, covering both
  the top-level `is_cached_policy_relation` entry *and* the inner
  `QueryDatabase` entries from the `SubtypeChecker`'s recursive descent. Inner
  writes are gated by `cache_definitive!` in the `SubtypeChecker`, so only
  lazy-resolution-stable results reach the shared store (#10921).

`application_eval_cache` and `instantiation_cache` are **intentionally not
shared** by default. Parallel file-checking can observe incomplete lib-merge
state during the first evaluation of a generic alias (`Promise<T>`,
`Awaited<T>`), producing a stale result that would then be served to sibling
files. Keeping them per-file eliminates the ordering-sensitive correctness risk
(#9507). The experimental `TSZ_SHARE_INSTANTIATION_CACHES=1` (#13240) opts a
build into sharing them; `share_instantiation_family` records that choice.

```
QueryCache.subtype_cache (RefCell, local) ── miss ──▶ Shared.subtype_cache (DashMap)
        ▲                                                     │
        └──────────── write-through on hit ───────────────────┘
                       (also write-through on definitive insert)
```

## Caches and invariants

`QueryCache::clear()` (line 331) is the **authoritative invalidation
boundary**: it empties every owned cache and resets relation stats. Two
properties follow from this:

1. Caches that grow unbounded on large repos live on `QueryCache`, **not** on
   `TypeInterner`. The interner survives clears and is not counted in
   `estimated_size_bytes`; a substitution-keyed cache there would leak. This is
   why `InstantiationCache` and `SubtypeReductionCache` are owned by
   `QueryCache` even though their *keys* (e.g. `CanonicalSubst`) are not
   interned (`caches/instantiation_cache.rs`, "Why on QueryCache" section).

2. Order-independent keys. `InstantiationCacheKey` carries a `CanonicalSubst`
   (`SmallVec<[(Atom, TypeId); 4]>` sorted by `Atom`), so two `TypeSubstitution`
   maps with the same `{name -> type}` multiset hash equal regardless of
   `FxHashMap` insertion order. `SubtypeReductionKey` carries `SortedTypeIds`
   (mirroring `tsc`'s `getTypeListId`), so `{1,2,3}` and `{3,1,2}` collapse to
   one slot. Both keys pack a `mode_bits` byte for the non-list inputs that
   change the result: for instantiation, bits 0–3 are `substitute_infer`,
   `preserve_meta_types`, `preserve_unsubstituted_type_params`, and
   shallow-return `this`; for reduction, `MODE_HAS_RESOLVER` (bit 0) records
   whether nominal class-hierarchy resolution was available, which changes which
   subtypes get removed.

### Per-`DefId` application-eval invalidation

The one cache with structured (sub-`clear()`) invalidation is the
application-eval cache. When a definition body is re-registered with different
content, every cached application result computed under the old body (or before
any body existed) is stale. `application_eval_index.rs` maintains a reverse
index `DefId -> {ApplicationEvalCacheKey}`:

- `record_dependencies` (line 12) runs
  `collect_application_eval_entry_def_dependencies` over a `(key, result)` pair
  to find every `DefId` the entry depends on — the keyed def, any def applied in
  the key's argument list, and any def whose `Lazy(def)` still appears in the
  cached *result* — and inserts the key under each.
- `invalidate_for_def` (line 25) removes the def's key set from the index and
  evicts exactly those keys from the cache.

`QueryCache::invalidate_application_eval_cache_for_def`
(`caches/query_cache.rs`, line 882) calls this and, when the shared cache opts
into sharing the instantiation family, also evicts the shared entries via
`SharedQueryCache::invalidate_application_eval_cache_for_def`.

### Checker-side per-`TypeId` env-eval invalidation

The checker keeps its own top-level result memo, `env_eval_cache`
(`crates/tsz-checker/src/context/env_eval_cache.rs`). Three invalidation
granularities exist, mirroring the solver's:

- `clear_env_eval_cache` — global flush.
- `invalidate_env_eval_for(type_id)` (line 177) — drop exactly one entry, no
  `O(cache)` scan. Used when one `type_id` must be re-evaluated under a
  different resolution mode after a bounded/speculative verdict.
- `invalidate_env_eval_reachable_from(type_id)` (line 206) — drop the entry for
  `type_id` *and* every entry whose key or cached result is a structural
  sub-term, computed by `collect_referenced_types` once and tested by O(1)
  membership. This is needed because a shallow/bounded first pass also cached
  results for the sub-terms it walked, and a later full pass would short-circuit
  to those stale sub-results. The recent targeted `env_eval_cache` invalidation
  (#13991) is exactly this minimal, non-`O(cache)` path.

### collect_properties: cross-collector recursion guard

`collect_properties_result_cache` scopes each result by `resolver_generation`.
The `resolver_generation` (a monotonic counter on `TypeResolver`,
`def/resolver.rs` line 25, bumped whenever a later `resolve` could return a
different type) prevents reusing a result after lazy `DefId` resolution can
change the answer — it is a generation-stamp invalidation, not an explicit
sweep.

Because the generation only advances, a result stamped with a superseded
generation is never served again (a lookup always supplies the caller's current
generation), yet the legacy flat `(TypeId, generation)` map never evicted it —
it grew by one mostly-dead entry per cacheable collection until the per-file
`clear()`. The memo (`query_cache_collect_properties_memo.rs`,
`CollectPropertiesMemo`) therefore retains only the `MAX_GENERATIONS_PER_TYPE`
most-recent generations per `TypeId` and evicts the oldest, bounding residency
on generation-churning programs without changing any served value (issue
\#14347). Replacing the generation epoch with per-dependency invalidation —
so an *unrelated* def publish stops invalidating the entry at all — rides on
canonical type identity (\#14344) and is tracked there.

Caching intersection property collection is subtle because the collector
recurses across *public* `collect_properties` calls (resolving recursive
mapped/indexed aliases resets the `TypeEvaluator`-local `seen` set).
`objects/collect.rs` tracks a thread-local `COLLECT_PROPERTIES_STACK` and a
`COLLECT_PROPERTIES_MIN_TRUNCATION` cell. A `CollectPropertiesDepthGuard::enter`
(line 45) that finds `type_id` already in flight up the chain records the
position so the owning `collect_properties_cached` can tell whether the
truncation was against one of *its own* in-flight entries (result still
context-free, cacheable) or against an *outer ancestor* (context-dependent
partial closure — must **not** be cached; the #12142 pitfall). The depth cap is
`MAX_COLLECT_PROPERTIES_DEPTH = 16_384`. The result enum is
`PropertyCollectionResult` (`objects/collect.rs`, line 102) — `Any`,
`NonObject`, or `Properties { properties, string_index, number_index }`.

## Object, interface, and class type representation

`tsz` represents structural object types with one struct,
`ObjectShape` (`types.rs`, line 1268), reached through the interned variants
`TypeData::Object(ObjectShapeId)` and `TypeData::ObjectWithIndex(ObjectShapeId)`
(the latter when the shape carries index signatures). The fields:

- `properties: Vec<PropertyInfo>` — sorted by name for stable hashing.
- `string_index` / `number_index: Option<IndexSignature>`.
- `flags: ObjectFlags`.
- `symbol: Option<SymbolId>` — the **nominal identity** for class instance
  types. This field participates in `Hash`/`PartialEq`, so two distinct classes
  with identical structure still intern to *different* `TypeId`s. Structural
  subtyping is computed explicitly in the solver, not via `PartialEq` — the
  `symbol` only prevents *interning* collisions between nominally distinct
  classes.

`PropertyInfo` (`types.rs`, line 1013) is rich enough to carry both type
semantics and emit/display metadata:

- `type_id` (read type) and `write_type` (setter type). When `write_type`
  differs from `type_id` and is non-`NONE`, the property is a TS 4.3+ split
  accessor (`has_split_accessor`); `write_type == NONE` encodes readonly from
  the lowering pass; `write_type == type_id` is a uniform property.
- `optional`, `readonly`, `is_method`, `is_class_prototype`, `visibility`
  (`Public`/`Protected`/`Private`, used for nominal subtyping).
- `parent_id` — declaring symbol, for nominal identity of private/protected
  members.
- `is_string_named` — `"404"` vs `404`: included in `PartialEq`/`Hash` because
  `"100"` and `100` are semantically distinct keys.
- `is_symbol_named` — distinguishes a real `__unique_N` symbol key from a
  user-authored string of that exact text.
- `declaration_order`, `single_quoted_name` — emit/display metadata, *excluded*
  from `PartialEq`/`Hash` so cosmetic differences intern to the same `TypeId`.

`ObjectFlags` (`types.rs`, line 1203) is a `bitflags` `u32`. The most
consequential flag for the compatibility model is `FRESH_LITERAL` (bit 0): it
marks an object literal as subject to excess-property checking. Others encode
structural-but-display-relevant facts: `HAS_LATE_BOUND_MEMBERS` (computed keys
that did not resolve to one literal — implicitly string-indexable for TS7053),
`MAPPED_CONSTRAINT_KEYS` (a non-homomorphic mapped type whose `keyof` is the
constraint key space, never an implicit `number`), `ENUM_NAMESPACE`,
`CONST_ENUM`, `INTERSECTION_MERGED` (identity-only: a merged `A & B` object stays
a distinct `TypeId` so diagnostics can recover the `&` form, but structural
subtyping ignores it), and `ALL_PROPERTIES_CONTEXT_SENSITIVE` (defer this fresh
literal's inference to generic-call Round 2). `mark_fresh_literal` /
`is_fresh_literal` are the named accessors so callers do not import the flag
directly.

Class instance and constructor types reuse `ObjectShape` (instance) and
`CallableShape`/`FunctionShape` (the constructor/call side, `types.rs` lines
1384 and 1466). `CallableShape` carries its own `properties`, `string_index`,
`number_index` — a callable can have object members (static-side properties on a
constructor).

### Apparent and primitive members

`objects/apparent.rs` answers "what members does a primitive *appear* to have?"
— `string` apparent-types to the boxed `String` interface, `number` to
`Number`, etc. `index_receiver_apparent_type` (line 26) mirrors `tsc`'s
`getApparentType(objectType)` for the TS7053 implicit-any-index diagnostic: the
`object` intrinsic maps to `{}`, a bare primitive to its boxed wrapper, every
other type unchanged. `apparent_primitive_members` supplies the bootstrap
fallback member list (e.g. `STRING_METHODS_RETURN_STRING`), with deliberate
omissions — `at` (es2022) is absent so the checker can still emit the correct
TS2550 "change your target library" suggestion when an old lib is loaded.

### Index signatures and element access

`IndexSignatureResolver<'a, R>` (`objects/index_signatures.rs`, line 440)
resolves string/number index signatures across object, callable, mapped, and
intersection shapes via dedicated `TypeVisitor` implementations
(`StringIndexResolver`, `NumberIndexResolver`). Public API:
`resolve_string_index`, `resolve_number_index`, `is_readonly(obj, kind)` with
`IndexKind::{String, Number}`.

`ElementAccessEvaluator` (`objects/element_access.rs`, line 29) computes `obj[k]`
into an `ElementAccessResult` (line 6): `Success(TypeId)`,
`NotIndexable`, `IndexOutOfBounds { index, length }`, `NoIndexSignature`, or
`PropertyNotFound`. The structural index-access checks (indexability, tuple
bounds, missing index signature) operate on the **apparent type** of the
receiver — a `TypeParameter`/`Infer` is indexed via its base constraint's
apparent type, matching `getApparentType` — while the result-type computation
routes through `evaluate_index_access_with_options`. `evaluate_type` is called
on the object first; `ERROR` and `ANY` short-circuit to `Success(ERROR)` /
`Success(ANY)`.

## Contextual typing (reverse inference)

Contextual typing flows type information *backwards*: from an expected type to
an expression that would otherwise be inferred bottom-up. It drives arrow-param
inference (`const f: (x: string) => void = x => …`), array/object literal
element typing, and callback parameter typing inside calls.

The entry point is `ContextualTypeContext<'a>` (`contextual/core.rs`, line 20),
holding the `interner`, an `expected: Option<TypeId>`, and `no_implicit_any`.
The heavy lifting is delegated to a family of visitor-based **extractors** in
`contextual/extractors.rs`, each a `TypeVisitor` that pulls one positional slice
of contextual type out of an expected type:

| Extractor | Pulls |
| --- | --- |
| `ParameterExtractor` / `ParameterForCallExtractor` | The contextual type of the *n*th parameter of an expected function type. |
| `RestParameterExtractor` / `RestPositionCheckExtractor` / `RestOrOptionalTailPositionExtractor` | Rest-parameter element types and trailing-position checks. |
| `ReturnTypeExtractor` | The expected return type. |
| `ThisTypeExtractor` / `ThisTypeMarkerExtractor` | The expected `this` parameter type. |
| `ArrayElementExtractor` / `TupleElementExtractor` | Per-element contextual types for array/tuple literals. |
| `PropertyExtractor` | The expected type of a named object-literal property. |
| `ApplicationArgExtractor` | The contextual type argument of a generic application. |

These compose through helpers like `collect_single_or_union`,
`collect_single_or_union_no_reduce`, `collect_single_or_union_preserve`, and
`collect_from_intersection`, which spread the extraction over union/intersection
members (a property's contextual type from `A | B` is the union of its type in
each member).

`rest_argument_element_type` (`contextual/core.rs`, line 35) extracts the
per-argument element type from a rest parameter: `...args: Foo[]` -> `Foo`, a
tuple rest's trailing element otherwise. It is depth-bounded (8 levels) and
normalizes evaluatable wrappers (`ConstructorParameters<T>`, `ReadonlyType`,
`NoInfer`, type-parameter constraints) by calling `evaluate_type` before
descending, so generic-call Round-2 contextual typing does not pass a whole
tuple application through as one argument type.

### apply_contextual_type: parity-critical preferences

`apply_contextual_type` (`contextual/core.rs`, line 1836) decides, given an
already-computed `expr_type` and an optional `contextual_type`, which to keep.
Its preference rules are exact `tsc`-parity edge cases:

1. `expr_type == ANY` stays `ANY` (line 1854). `tsc` computes an object-literal
   property's type from the expression itself; a contextual property type
   influences widening and freshness but never overwrites an `any` value.
   Substituting here once turned `{ value }` shorthand into the union of
   contextual property types across a discriminated-union target, producing a
   false TS2322. Since `any` is assignable everywhere, preserving it cannot mask
   a real error.
2. `unknown`/error `expr_type` -> use the contextual type.
3. A literal `expr_type` against a `Union` contextual type -> keep the literal
   (more specific than the union).
4. If `expr_type` is a subtype of the contextual type (or of a union member),
   keep `expr_type` (it is more specific). A single reused `SubtypeChecker` runs
   all these checks, `reset()` between each.
5. **Default: prefer the expression type.** When the contextual type is
   *narrower* than the expression type (ctx = `"foo"`, expr = `string`), the
   contextual type must **not** be substituted — the expression genuinely has
   the wider type at runtime, and substituting the narrower contextual type
   would mask a real TS2322.

Contextual typing is invoked by the checker's call/argument and
declaration machinery; the checker side is documented in
[checker-calls-signatures-generics](checker-calls-signatures-generics.md). The
solver here owns only the *type-shape* extraction, not the AST walk.

## The type_queries API surface

`type_queries` is the checker's stable window into type shape. Its design
principle (`type_queries/mod.rs`) is **abstraction**: checker code calls
`is_callable_type`, `is_tuple_type`, `classify_for_excess_properties`, etc.,
instead of matching on `TypeData`. This is the architectural boundary that keeps
the checker from pattern-matching raw solver internals.

The module re-exports three layers:

- **Predicates** (from `visitors/visitor_predicates`, the canonical
  implementations): `is_array_type`, `is_union_type`, `is_conditional_type`,
  `is_mapped_type`, `is_empty_object_type`, `is_function_type`,
  `contains_any_type`, … `type_queries` re-exports them so a single
  `type_queries::*` import suffices.
- **Classifiers** (`type_queries/classifiers.rs` and `extended.rs`):
  `classify_*` functions returning small enums — `ExcessPropertiesKind`,
  `InterfaceMergeKind`, `ConstructorAccessKind`, `CallSignaturesKind`,
  `PromiseTypeKind`, `ArrayLikeKind`, `IndexKeyKind`, `LiteralTypeKind`, … —
  plus accessors like `get_lazy_def_id` (aliased as `get_def_id`),
  `get_conditional_type_id`, `get_mapped_type_id`, `get_application_info`.
- **Deeper queries**: `shape_queries.rs` (recursive structural predicates over
  projection paths, e.g. `is_generic_conditional_check_type`,
  `shape_contains_conditional_type_db`), `mapped.rs` /
  `mapped_declaration_surface.rs` / `mapped_display_order.rs` (mapped-type
  surface), `flow.rs` (flow-relevant type facts), `declaration_walks.rs`,
  `iterable.rs`, `global_interfaces.rs`, `extended_constructors.rs`,
  `traversal.rs`.

A representative parity-critical query is `is_generic_conditional_check_type`
(`type_queries/core.rs`, line 34): `tsc`'s `getConditionalType` never resolves a
conditional whose effective check type is still generic. The function
implements `isGenericType` — instantiable markers (`TypeParameter`/`infer`/
`this`/indexed-access/`keyof`/string-mapping/deferred conditional) plus object
types whose *type arguments* are generic — and crucially does **not** recurse
through object members or signatures, so `(x: T) => void extends Function` still
resolves eagerly. It uses a worklist traversal with a `visited` set (replacing
an older depth-recursive `.any()` that re-walked shared subtrees super-linearly
and caused a ~300s typebox timeout), making the cost O(distinct reachable nodes)
and the decision schedule-independent under parallel checking.

## Visitor traversal utilities

Every solver walk is built on the `TypeVisitor` trait (`visitors/visitor.rs`,
line 65). It has one method per `TypeData` variant (`visit_intrinsic`,
`visit_object`, `visit_union`, `visit_conditional`, `visit_mapped`,
`visit_lazy`, …), each defaulting to `default_output()`, and one dispatch entry
point `visit_type` -> `visit_type_key` (line 256) that matches the `TypeData`
and routes to the right method. A visitor implements only the variants it cares
about and lets the rest fall through.

For pure structural traversal (no per-variant logic), `for_each_child`
(line 339) invokes a closure on each immediate child `TypeId`, and
`walk_referenced_types` walks the transitive closure. `for_each_child_by_id`
fast-paths intrinsics (a free `TypeId`-range check) to skip the `lookup` and
match dispatch on every leaf. `walk_referenced_types` pools its visited-set and
stack in a thread-local `WALK_POOL` to avoid a fresh `FxHashSet` + `Vec` per
call.

The key consolidation lives in `visitors/child_policy.rs`: historically several
hand-rolled enumerations of all `TypeData` variants each visited a slightly
different child set, kept in sync only by comments. They are now one enumerator,
`try_for_each_child_with_policy`, parameterized by an explicit `ChildPolicy`
struct (line 32) whose boolean fields make each walker's deliberate child-set
differences visible at the type level. Examples:

- `application_base` — predicate walkers skip an `Application`'s base because the
  base's own type parameters are bound by the application's arguments, so
  `A<number>` must not count as "containing type parameters."
- `skip_generic_signature_bodies` — free-occurrence walkers skip a *generic*
  signature's body: it binds its own type parameters, so references inside are
  not free occurrences from the enclosing scope.
- `property_write_types`, `index_key_types`, `signature_this_type`,
  `signature_type_predicate` — gate the positions where read vs. write,
  value vs. key, and signature-metadata walkers diverge.

`for_each_child` is just the `ChildPolicy::FULL` driver. `visitor_predicates.rs`
holds the canonical `is_*_type` predicates (pooling their scratch buffers in
thread-locals via `with_predicate_buffers`), and `visitor_extract.rs` holds the
`TypeData`-extraction helpers (`array_element_type`, `tuple_list_id`,
`readonly_inner_type`, …) re-exported through `visitor.rs`.

## Evaluation limits and fuel

The recursive kernels (relation, evaluation, instantiation, substitution) are
bounded by a *family* of named, per-operation-class limits, consolidated in
`limits/mod.rs` (#13091). Critically they are **not** unified into one
depth+fuel pair: each class has different firing semantics, and this policy area
is regression-prone in both directions (a depth-bail change once caused a 7.1x
ts-toolbelt slowdown, #6973). What *is* consolidated is the mechanism: every
constant is defined or re-exported here, and the scattered per-counter
`thread_local!` cells are merged into one `LimitBudgets` struct behind a single
`thread_local!` so multi-counter hot paths resolve TLS once (macOS
`__tls_get_addr` is ~10–15ns/access).

The guard inventory (selected; full table in the module doc):

| Limit constant | Value | `tsc` analogue | Firing semantics |
| --- | --- | --- | --- |
| `MAX_SUBTYPE_DEPTH` | 100 (+100k iters) | `recursiveTypeRelatedTo` depth 100 -> `Ternary.Maybe` | assumed-related `DepthExceeded` |
| `MAX_GLOBAL_SUBTYPE_FUEL` | 10_000 / chain | (closest: `relationCount`) | assumed-related; cacheable only as fuel-band `LimitTrue` |
| `MAX_DEF_DEPTH` | 100 | `instantiationDepth` -> TS2589 | hard bail, memoized `ERROR` when real |
| `REAL_INSTANTIATION_BAILOUT_THRESHOLD` | 40 | — | escalation floor: TS2589 only if a `DefId` expanded ≥40× |
| `MAX_TAIL_RECURSION_DEPTH` | 1000 | `getConditionalType` `tailCount` 1000 (exact parity) | TS2589 + `ERROR` |
| `MAX_TYPE_SUBSTITUTION_DEPTH` | 50 | (half of `instantiationDepth`) | sticky `depth_exceeded`, returns input opaque |
| `MAX_INFER_SUBSTITUTION_NODES` | 1_000_000 | — | breadth bound; remaining nodes left opaque |
| `MAX_EVALUATION_FUEL` | 2_000_000 / file | `instantiationCount` (5M; lower here) | TS2589-style `ERROR` |
| `MAX_GLOBAL_EVAL_DEPTH` | 200 frames | — | silent opaque bail |
| `MAX_GLOBAL_INSTANTIATION_DEPTH` / `_FUEL` | 50 / 2000 per file | `instantiationDepth`/`Count` at checker boundary | leaves application un-expanded |
| `MAX_SOLVER_STACK_FRAMES` | 2000 live frames | — (OS-stack protection, #7574) | relation-preserving default |

The `MAX_GLOBAL_SUBTYPE_FUEL` budget is what makes the `LimitTrue` cache value
honest. `remaining_global_subtype_fuel()` (line 357) is
`MAX_GLOBAL_SUBTYPE_FUEL - consumed`; a `LimitTrue { fuel_band }` entry is reused
only when the *current* query's remaining budget is `<= fuel_band` and
`limit_result_cache_enabled()` is true (the #13241 kill switch). The eval-fuel
counter is sampled every `EVAL_FUEL_CHECK_INTERVAL = 128` guard iterations to
amortize the TLS access on the hot path.

The module doc deliberately records *known double-fire / divergence findings*
without changing them — e.g. the same recursive descent is depth-counted by up
to three stack guards (per-instance depth 100, cross-evaluator depth 200, shared
frames 2000) at different scopes, each catching a recursion shape the others
structurally cannot. These are documented overlaps, not bugs.

## The compatibility model: Judge vs. Lawyer

The relation engine separates *pure structural subtyping* (the **Judge**) from
*TypeScript's unsound legacy quirks* (the **Lawyer**). The Judge is
`SubtypeChecker` (covered in [solver-relations](solver-relations.md)); it knows
nothing about `any` permissiveness, excess properties, or weak types. The Lawyer
is `CompatChecker` (`relations/compat.rs`, line 245) plus `AnyPropagationRules`
(`relations/lawyer.rs`, line 149). The guiding invariant from the relation docs:
**the Lawyer never makes types *more* compatible than the Judge — it only adds
restrictions** — with the single true exception that `any` short-circuits to
*more* permissive.

```
checker assignability boundary (TS2322/2345/2416)
        │  (query_boundaries/assignability gateway)
        ▼
CompatChecker  ── any short-circuit (Lawyer) ─┐
        │                                      │
        │  excess-property / weak-type /       │
        │  void-return / variance overrides    │
        ▼                                      ▼
SubtypeChecker (Judge: pure structural subtyping)
```

### AnyPropagationRules — the any selector

`AnyPropagationRules` (`relations/lawyer.rs`) captures whether `any` may silence
nested structural mismatches by choosing one of the subtype engine's
`AnyPropagationMode`s (`any_propagation_mode`, line 199):

- `All` (default, `allow_any_suppression = true`) — `any` is both top and bottom
  at every nesting level (classic TypeScript).
- `TopLevelOnly` (`allow_any_suppression = false`, the `strict()` constructor) —
  even when `any` is involved, structural checking still runs and reports nested
  mismatches.
- `AnySourceNotRelated` (set via `set_any_source_not_related`) — the
  overload-resolution subtype pass (`tsc`'s `chooseOverload` with
  `subtypeRelation`): an `any` *source* is not related to concrete targets at
  any nesting, while an `any`/`unknown` *target* still accepts everything. Takes
  precedence over `allow_any_suppression`.

This is the project's "default preference: `any` must not silence structural
mismatches unless compatibility mode requires it" knob.

### CompatChecker — the configured Lawyer

`CompatChecker<'a, R>` (line 245) holds a `SubtypeChecker` (the Judge it wraps),
the `AnyPropagationRules` lawyer, the policy flags (`strict_function_types`,
`strict_null_checks`, `no_unchecked_indexed_access`,
`exact_optional_property_types`, `allow_bivariant_rest`,
`disable_method_bivariance`, `skip_weak_type_checks`, `strict_subtype_checking`),
an optional `query_db` for memoization, and an operation-local
`cache: FxHashMap<(TypeId, TypeId), bool>`.

`is_assignable` (line 811) is the main entry. Its top-level fast paths: identity
(`source == target` -> `true`); and *without* `strictNullChecks`, a nullish
target accepts everything — applied **only at the top level**, not inside union
member iteration, so it cannot incorrectly accept a type inside a union
comparison. Then it checks the local `cache`, runs `is_assignable_impl` (with
the configured `strict_function_types`), and memoizes.
`is_assignable_strict` (line 1407) is the variant used for lib.d.ts and
identity-leaning checks; `explain_failure` (line 1468) re-runs the check to
produce a structured `SubtypeFailureReason` consumed by the assignability
gateway (see [checker-assignability-gateway](checker-assignability-gateway.md)).

### Freshness and excess-property checking (TS2353)

Excess-property checking is the Lawyer's signature restriction. A property the
target does not declare is an error **only** when the source is a *fresh* object
literal — a literal whose `TypeId` was interned with
`ObjectFlags::FRESH_LITERAL`. Once a literal is assigned through a variable or
widened, freshness is gone and excess properties are allowed.

`relations/freshness.rs` owns the flag's lifecycle:

- `is_fresh_object_type` (line 8) — does the shape carry `FRESH_LITERAL`?
- `widen_freshness` / `widen_freshness_deep` (lines 21/34) — mirror `tsc`'s
  `getRegularTypeOfObjectLiteral`: strip `FRESH_LITERAL` from the object **and
  recursively from all property (read and write) types**, depth-bounded at 10.
  This deep walk is needed because generic inference can produce a non-fresh
  outer object whose property types are still fresh (e.g. inferring through
  `Readonly<T>`). When widening produces a new `TypeId`, the function carries
  forward display properties and the display alias so diagnostics keep the
  original surface.

`CompatChecker::check_excess_properties` (line 845) / `find_excess_property_in`
(line 858) implement the check. The algorithm, matching `tsc`:

1. Union sources -> report the first excess property found in any fresh member.
2. If the source is not a fresh object literal, there is nothing to check.
3. If the target has a string index signature accepting all strings
   (`STRING` is a subtype of the key type), skip the check entirely.
4. If the resolved target has no properties, no number index, and no string
   index types — i.e. `{}`, an empty interface/class — forgive everything (an
   empty target accepts any non-primitive).
5. Otherwise, for each source property not in the target's property set: allow
   numeric-named properties when the target has a number index signature; allow
   names matching a string index key type; otherwise it is excess (TS2353).

### Weak types (TS2559)

A *weak type* is an object type with only optional properties: assigning an
object that shares *none* of those properties is almost always a mistake.
`relations/compat_weak.rs` owns `is_weak_type`, `violates_weak_type`, and
`violates_weak_union`. The Lawyer's `WeakViolation` enum (`compat.rs`, line 237)
lets `explain_failure` either compute the probe inline (`Compute`) or reuse a
single shared probe (`Precomputed { violates_union, violates_type }`) so the
reason-collection boundary derives both the boolean and the failure reason from
one pass (#13243). The `skip_weak_type_checks` flag exists because `tsc`'s
`isTypeAssignableTo` does *not* include the weak-type check — it is only applied
at specific diagnostic sites — so the Lawyer can suppress it when emulating the
plain assignability relation.

### Other nominal/variance overrides

The Lawyer also layers enum/abstract-constructor/private-brand nominality
(`relations/compat_overrides.rs`), mapped-type assignability shortcuts
(`relations/compat_mapped.rs`), and the variance/bivariance knobs
(`disable_method_bivariance`, `allow_bivariant_rest`). The method-parameter
bivariance exception and `void`-return tolerance are `tsc` legacy unsoundnesses
the Judge does not model on its own; the Lawyer enables them through these flags.

## Edge cases and tsc parity

- **`any` in contextual position.** `apply_contextual_type` preserves an `any`
  expression type rather than substituting the contextual type
  (`contextual/core.rs`, line 1854) — substituting it produced false TS2322 on
  shorthand object-literal properties against discriminated-union targets.
- **`any` in assignability.** Default `AnyPropagationMode::All` makes `any`
  top-and-bottom everywhere; the overload subtype pass uses
  `AnySourceNotRelated` so an `any` source is *not* related to concrete targets
  during overload selection (matching `chooseOverload` with `subtypeRelation`).
- **Excess properties through index signatures.** A target with a string index
  signature accepting all strings, or `{}`/empty interfaces, forgives excess
  properties; a number index signature forgives only numeric-named ones
  (`find_excess_property_in`).
- **Freshness survives one widening.** Generic inference through `Readonly<T>`
  can leave property types fresh under a non-fresh outer object; `widen_freshness`
  walks *all* property read/write types, not just fresh outer shapes.
- **Nominal class identity.** `ObjectShape.symbol` participates in `Hash`/`Eq`,
  so two structurally identical classes get distinct `TypeId`s; structural
  subtyping is still computed by the solver, not implied by interning.
- **Element access includes `undefined` regardless of `exactOptionalPropertyTypes`.**
  The element-access cache key intentionally omits that flag because indexed
  access of an optional property includes `undefined` under both settings
  (`caches/query_cache.rs`, line 45) — matching `tsc`.
- **Fuel-truncated relation verdicts are budget-conditional.** A `LimitTrue`
  cache entry is reused only when the current query has *no more* budget than
  the recorded run; a query with a fatter budget recomputes honestly
  (`limit_true_usable`).
- **Application-eval staleness on body re-registration.** Re-registering a
  definition body with new content evicts every cached application result that
  depends on that `DefId` (directly, via an argument, or via a `Lazy(def)` in
  the result) through the dependency index — not a blanket flush.
- **Per-`this`-binding relation caching.** A verdict that resolves a polymorphic
  `this` is cached under its resolved receiver (`RelationCacheKey.this_context`)
  so the cross-checker shared cache cannot serve it to a sibling comparing the
  same pair under a different receiver (#13828).
- **`MAX_TAIL_RECURSION_DEPTH = 1000` exact parity.** The conditional
  tail-recursion loop matches `tsc`'s `getConditionalType` `tailCount` exactly,
  so tail-recursive conditional types accept the same programs.

## See also

- [solver-relations](solver-relations.md) — the Judge (`SubtypeChecker`) and the
  structural walk whose verdicts these caches store.
- [solver-evaluation](solver-evaluation.md) — the `TypeEvaluator`, owner of the
  meta-type reduction these eval caches memoize.
- [solver-instantiation](solver-instantiation.md) — `instantiate_type`, whose
  results the `InstantiationCache` stores.
- [solver-inference](solver-inference.md) — generic inference, which consumes
  contextual types and BCT subtype reduction.
- [solver-types-intern-def](solver-types-intern-def.md) — `TypeData`,
  `TypeInterner`, `DefId`/`Lazy` resolution, the layer beneath these caches.
- [checker-assignability-gateway](checker-assignability-gateway.md) — the
  checker boundary that calls `CompatChecker` and turns a reason into TS2322/
  TS2345/TS2416.
- [checker-calls-signatures-generics](checker-calls-signatures-generics.md) —
  the checker side of contextual typing for call arguments.
- [end-to-end-timeline](end-to-end-timeline.md) — where cache lifetimes sit in
  the per-file checking timeline.
