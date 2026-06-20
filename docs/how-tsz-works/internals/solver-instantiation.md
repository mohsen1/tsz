# Instantiation, Type Mappers, and Instantiated-Type Caching

Instantiation is the act of taking a *generic* type body — the interned shape a
declaration like `type Box<T> = { value: T }` produces with `T` left as a free
`TypeData::TypeParameter` — and rewriting it under a substitution
`{ T -> number }` to produce the concrete `{ value: number }`. In `tsz` this is
the job of one subsystem, `crates/tsz-solver/src/instantiation`, whose engine is
`TypeInstantiator` (see `instantiation/instantiate.rs`). Every generic alias
application, every call where a function's type parameters get pinned to inferred
arguments, every mapped-type expansion, and every `this`-typed method return
ultimately routes through a `TypeInstantiator::instantiate` walk.

`tsz`'s instantiator is *not* a thin substitution loop. It is the point where
several of the compiler's most subtle parity behaviors live: declaration-scoped
type-parameter identity is preserved (`#13044`), homomorphic mapped types over
arrays/tuples/unions are special-cased to mirror tsc's
`instantiateMappedArrayType` / `instantiateMappedTupleType`, meta-types (`keyof`,
indexed access, template literals) are eagerly reduced for `O(1)` structural
equality, and the depth/frame guards bail to a *relation-preserving* partial type
rather than an `ERROR` sentinel (`#13652`). Because the same body and the same
substitution recur constantly during recursive utility-type expansion, a
cross-call `InstantiationCache` keyed on a canonicalized substitution sits in
front of the engine, with an alpha-renaming layer that lets structurally-equal
substitutions share a slot.

This chapter traces the substitution mapper, the engine walk arm by arm, the
public entry surface, the caches and their invariants, and the recursion/fuel
guards, all grounded in the real module structure.

## Owns / Must not own

| Owns | Must not own |
| --- | --- |
| `TypeSubstitution` (the `name -> TypeId` mapper) and its default-resolution / `from_args` construction | Deciding *which* arguments to bind — that is inference (`solver-inference`) or explicit type-argument resolution in the checker |
| The structural rewrite walk: `TypeInstantiator::instantiate_key` over every `TypeData` variant | Symbol/`DefId` resolution: the instantiator runs with a `NoopResolver` and defers `Lazy` bodies it cannot expand |
| Per-instance recursion-depth guard (`depth`, `MAX_TYPE_SUBSTITUTION_DEPTH`), the cross-operation `with_solver_frame` budget, and the relation-preserving `bail_value` | The checker-side TS2589 escalation (`tsz_common::limits::MAX_INSTANTIATION_DEPTH`, the `EvaluationSession` counters) |
| The cross-call `InstantiationCache` (key shape, alpha-renaming, `QueryCache` lifetime) | Any cache that survives a `QueryCache::clear()` — substitutions are never interned on the long-lived `TypeInterner` |
| Eager meta-type reduction (`keyof`, `IndexAccess`, mapped, template) when operands are concrete, and the deferral gates that keep them un-reduced when a resolver is required | Running the *resolver-backed* evaluation: deferred meta-types are handed back to `solver-evaluation`'s `TypeEvaluator` |
| Homomorphic mapped-type rewriting (array/tuple/union distribution, modifier inheritance) | The non-instantiation mapped/keyof evaluation rules in `evaluation/evaluate_rules` |
| `ApplicationEvaluator` (resolve base `DefId` body, build substitution, instantiate, recurse) | The relation kernel, narrowing, or operations |

For how generic *calls* arrive here from the checker, see
[checker-calls-signatures-generics](checker-calls-signatures-generics.md); for
how inference produces the arguments, see [solver-inference](solver-inference.md);
for the resolver-backed evaluation the instantiator defers to, see
[solver-evaluation](solver-evaluation.md); for the interner and `DefId`/`Lazy`
mechanics this builds on, see
[solver-types-intern-def](solver-types-intern-def.md); for the surrounding cache
family on `QueryCache`, see
[solver-caches-objects-contextual-compat](solver-caches-objects-contextual-compat.md).

## Where the code lives

| Path | Role |
| --- | --- |
| `instantiation/mod.rs` | Module root: re-exports `application`, `instantiate`, `request`, `result` |
| `instantiation/instantiate.rs` | The `TypeInstantiator` struct, the top-level `instantiate` / `instantiate_inner` / `instantiate_key` dispatch, `bail_value`, the cycle/leaf/shadowing machinery |
| `instantiation/instantiate/substitution.rs` | `TypeSubstitution` (the mapper), `from_args` default resolution, `is_identity_for`, `canonical_pairs` |
| `instantiation/instantiate/api.rs` | The public entry surface (`instantiate_type`, `instantiate_generic`, `substitute_this_type`, ...), the cache probe/fill, alpha-renaming, `can_skip_concrete_instantiation` |
| `instantiation/instantiate/mapped.rs` | The `TypeData::Mapped` arm: homomorphic array/tuple/union special cases, deferral gates |
| `instantiation/instantiate/conditional.rs` | The `TypeData::Conditional` arm: distributive expansion over substituted unions and `boolean` |
| `instantiation/instantiate/indexed.rs` | The `IndexAccess`, `KeyOf`, `TemplateLiteral`, `StringIntrinsic` arms: instantiate-then-maybe-evaluate |
| `instantiation/instantiate/homomorphic.rs` | `try_expand_substituted_homomorphic_object_mapped` |
| `instantiation/instantiate/signatures.rs` | The `Function` / `Callable` arms and the shared call-signature instantiation, including shadow-scope handling of own type parameters |
| `instantiation/instantiate/display_properties.rs` | Property-slot instantiation with a local memo, display-property / application-origin propagation |
| `instantiation/request.rs` | `InstantiationRequest`, `InstantiationOptions`, the mode-bit packing |
| `instantiation/result.rs` | `InstantiationResult` (`type_id` + `overflowed`) |
| `instantiation/application.rs` | `ApplicationEvaluator`: resolve a generic `Application` base to its body and instantiate |
| `caches/instantiation_cache.rs` | `InstantiationCacheKey`, `CanonicalSubst`, `InstantiationCache` storage |

## The substitution mapper: `TypeSubstitution`

A type mapper in `tsz` is `TypeSubstitution` (`substitution.rs`): a thin wrapper
around `FxHashMap<Atom, TypeId>` mapping a type-parameter *name atom* to its
replacement `TypeId`. Keys are bare `Atom`s, not `TypeParamInfo`, because the
mapper is consulted by name during the walk. The common single-binding case has
a dedicated `TypeSubstitution::single(name, type_id)` that allocates a capacity-1
map.

The non-trivial constructor is `TypeSubstitution::from_args`, which turns a
`(type_params, type_args)` pair into a substitution and is the universal entry
for "apply these type arguments to these declared parameters". It runs in three
phases:

```text
from_args(type_params = [T, U=T, V=U], type_args = [boolean])
  Phase 1  bind each supplied arg: T -> boolean
           (a TypeId::ERROR arg is SKIPPED — see edge cases)
  Phase 2  pre-seed every unsupplied param with `any`: U -> any, V -> any
  Phase 3  resolve defaults in declaration order against the map so far:
           U's default `T` instantiates to `boolean`  => U -> boolean
           V's default `U` instantiates to `boolean`  => V -> boolean
```

Phase 3 instantiates each default through `instantiate_type` *against the
substitution built so far*, which is how a default that references an earlier
parameter (`U = T`) sees the supplied argument, and how a chain (`U = T`,
`V = U`) propagates. The working map is wrapped in a `TypeSubstitution` once and
mutated in place (`insert`/`remove` per parameter) rather than cloned per default
— important for deeply-defaulted recursive utility shapes. A circular default
(`X = X`) is caught by `type_references_param` and falls back to `any`, matching
tsc; a forward reference (`U = V`, `V` later/unsupplied) sees `V`'s phase-2 `any`
seed. A parameter with neither argument nor default has its phase-2 `any` seed
removed in phase 3 so the body keeps the *raw* parameter.

`is_identity_for(interner, type_params)` is the soundness-critical identity test:
it compares each declared parameter's interned `TypeId`
(`interner.type_param(*param)`) against its substituted value, rather than
comparing names. This matters because declaration-scoped fresh type parameters
share a *name* but not a `TypeId` (`intern_fresh` bypasses the dedupe table), so
a name-only identity check would wrongly skip instantiation across scopes.
`canonical_pairs` produces the sorted `SmallVec<[(Atom, TypeId); 4]>` that becomes
the cache key's substitution component.

## The engine: `TypeInstantiator`

`TypeInstantiator` (`instantiate.rs`) borrows the interner and the substitution
and carries the walk state. The fields that shape behavior:

| Field | Meaning |
| --- | --- |
| `substitution` | the borrowed mapper |
| `visiting: FxHashMap<TypeId, TypeId>` | cycle table: maps an in-flight input `TypeId` to its placeholder, then to its result |
| `shadowed: Vec<Atom>` | type-parameter names locally bound (e.g. a function's own `<T>`, a mapped type's iteration variable) that must NOT be substituted by the outer map |
| `local_type_params: Vec<(Atom, TypeId)>` | freshly-instantiated own params of a nested generic scope, looked up before the outer map |
| `this_type: Option<TypeId>` | concrete type to substitute for `TypeData::ThisType` |
| `shallow_this_only: bool` | restrict `this` substitution to combinator positions; leave named object/method internals raw |
| `preserve_unsubstituted_type_params` | disable the constraint-fallback that rewrites an unmapped `T` to its instantiated constraint |
| `preserve_meta_types` | keep `keyof` / indexed-access / mapped un-reduced instead of eagerly evaluating |
| `substitute_infer` | also substitute `TypeData::Infer` placeholders |
| `depth`, `max_depth`, `depth_exceeded` | per-instance recursion guard |
| `substitution_is_inference_only` | cached: every key is an `__infer_*` placeholder; gates the constraint fallback (`#8725`) |

### The walk: `instantiate` -> `instantiate_inner` -> `instantiate_key`

`instantiate(type_id)` is the entry every recursion uses. Its prologue:

```text
instantiate(type_id):
  if type_id.is_intrinsic()          -> return type_id        (fast path)
  if self.depth_exceeded             -> return bail_value(type_id)
  if self.depth >= self.max_depth    -> set depth_exceeded; bail_value(type_id)
  with_solver_frame(|| {             -> cross-operation stack budget
      self.depth += 1
      result = self.instantiate_inner(type_id)
      self.depth -= 1
      result
  }).unwrap_or_else(|| { set depth_exceeded; bail_value(type_id) })
```

The intrinsic fast path skips everything for `number`, `string`, `boolean`,
`any`, etc. (`TypeId::is_intrinsic` is a reserved-id check, not a lookup). The
`with_solver_frame` wrapper (`crate::recursion`) is the *shared* stack-frame
breaker: it pairs a thread-local frame budget with `stacker::maybe_grow` so the
combined `evaluate -> subtype -> instantiate -> evaluate` recursion that no single
per-instance `depth` ever sees is still bounded (issue `#7574`).

`instantiate_inner` then handles cycles and leaves:

1. **Cycle hit**: if `visiting` already has `type_id`, return the cached value
   (the placeholder during an active cycle, or the final result afterward).
2. **Leaf**: `is_instantiation_leaf(&key)` returns `true` for
   `Intrinsic`, `Literal`, `UnresolvedTypeName`, `Error`, `Lazy`, `Recursive`,
   `BoundParameter`, `TypeQuery`, `UniqueSymbol`, `ModuleNamespace` — these never
   change under substitution, so they are returned as-is. Note `Lazy(DefId)` is a
   leaf here: the instantiator does *not* resolve semantic references; it leaves
   them for the resolver-backed evaluator.
3. Otherwise mark `visiting[type_id] = type_id` (the cycle placeholder), call
   `instantiate_key`, then overwrite `visiting[type_id] = result`.

`instantiate_key` is the per-`TypeData` dispatch. The shape-preserving arms all
follow the same "instantiate children, re-intern only if something changed,
otherwise return the original `type_id`" discipline (via helpers like
`instantiate_type_list_if_changed`, `instantiate_params_if_changed`,
`instantiate_properties_if_changed`). Returning the *original* id on no-change is
not just an optimization: for `TypeParameter` it is correctness — a structural
re-intern would rewrite a declaration-scoped fresh parameter to the structural
canonical and split identity (`#13044`).

### The `TypeParameter` arm

This is the heart of substitution. For `TypeData::TypeParameter(info)`:

```text
1. lookup_local_type_param(info.name)  -> if a fresh local scope bound it, use that
2. is_shadowed(info.name)              -> return the ORIGINAL id (do not re-intern)
3. substitution.get(info.name)         -> the substituted TypeId  (the hit case)
4. else, if not preserve_unsubstituted and should_apply_constraint_fallback:
     instantiate the parameter's constraint; if it CHANGED, use it
     (Actions extends ActionsObject<State>, {State: number} => ActionsObject<number>)
5. else return the ORIGINAL id  (free parameter, declaration identity preserved)
```

`should_apply_constraint_fallback` (`instantiate.rs`) is the `#8725` guard: when
the substitution binds *only* inference variables (`substitution_is_inference_only`)
and the parameter is user-defined, it belongs to a foreign generic scope and
walking its constraint with this substitution would collapse `keyof (A | B)` to
`never`; in that case the parameter stays put.

### Composite arms and re-interning

`Union` re-instantiates members, re-interns with `interner.union(...)`, and
re-stores any union *origin* (the pre-canonicalization member order). `Intersection`
re-interns and calls `propagate_display_properties_for_intersection`. `Array`,
`ReadonlyType`, `NoInfer` re-wrap only on change. `Object` / `ObjectWithIndex`
instantiate property read/write types and index signatures, re-intern through
`object_with_flags_and_symbol` / `object_with_index`, and propagate display
properties and `application_eval_origin`. `Enum(def_id, member)` keeps the
nominal `def_id` and instantiates only the structural member type. `Application`
instantiates base and args and re-interns — it does *not* resolve the application
here.

`Tuple` is the most involved structural arm: it flattens substituted spread arms
whose substituted element is itself a tuple, but only when substitution actually
changed something (pre-existing concrete annotation tuples are left in their
original form to match tsc). It tracks a *represented* cardinality (the semantic
sum across spread arms, not physical slots) against two gates: a hard gate at
`MAX_REPRESENTABLE_TUPLE_LENGTH` (10,000) that calls `mark_tuple_too_large` and
returns `TypeId::ERROR` before allocating, and a soft gate at 8,192
(`MAX_TUPLE_SPREAD_FLATTEN_ELEMENTS`) that keeps an oversized spread as a single
rest element instead of materializing exponential physical slots.

### Function / Callable arms and own-parameter shadowing

`instantiate_function` and `instantiate_callable` (`signatures.rs`) treat a
function's own `<T>` as a *new scope*. They call `enter_shadowing_scope`, which
extends `shadowed` with the own parameter names and removes those parameters from
the `visiting` cycle table (cloning it only when non-empty — a perf shortcut for
the common empty case). They instantiate the own parameters' constraints/defaults
(`instantiate_type_params_if_changed`, with `preserve_unsubstituted_type_params`
forced on so self-references stay anchored), then — *only when those infos
changed* — push the own parameters into `local_type_params` so their occurrences
in params/return redirect to the freshly-instantiated declaration. They restore
via `exit_shadowing_scope`. When *nothing* changed, the original `type_id` is
returned (function/callable shapes are canonically interned, so no re-intern is
needed).

## Meta-type arms: instantiate then maybe evaluate

The instantiator does more than substitute: for *meta-types* (`keyof`, indexed
access, conditional, mapped, template literal, string intrinsic) it eagerly
reduces the result to a concrete shape so equality is `O(1)` (referred to in the
code as "Task #46"). But it runs with a `NoopResolver`, so it must *defer*
reduction whenever a resolver-backed body (`Lazy`, generic `Application`,
`TypeQuery`) is involved.

### `IndexAccess` and `KeyOf` (`indexed.rs`)

`instantiate_index_access` instantiates `obj` and `idx`, then:

- if either still `contains_type_parameters`, return the raw
  `interner.index_access(...)` (don't evaluate `T[K]` while `T` is a placeholder);
- if `preserve_meta_types` or `index_access_operand_needs_resolver` (operand
  reaches `Application`/`Lazy`/`TypeQuery`/`Conditional`/nested meta), stay
  deferred;
- otherwise `evaluate_index_access` immediately.

`instantiate_keyof` mirrors this. It additionally keeps `keyof` deferred over a
union/intersection operand that `contains_lazy_or_recursive_db`, because the
resolver-less `evaluate_keyof` would collapse such an operand to a
structurally-detached key set that drops the source's optional/readonly modifiers.
On successful eager reduction it stores a *display alias*
(`store_display_alias(result, keyof_type)`) so the printer can show `keyof Shape`
rather than the expanded union.

### `TemplateLiteral` and `StringIntrinsic`

`instantiate_template_literal` instantiates each `TemplateSpan::Type`; if any
becomes a string/number/boolean literal, a union, or a primitive, it flags
`needs_evaluation` and runs `evaluate_type` to expand
`` `prefix${"a"|"b"}` `` into a string-literal union. `instantiate_string_intrinsic`
(`Uppercase`, `Lowercase`, etc.) instantiates the argument and evaluates when the
argument became a concrete string-shaped type.

### `Conditional` (`conditional.rs`)

`instantiate_conditional` implements distributive expansion. When the conditional
is distributive and its `check_type` is exactly the substituted type parameter:

- substituting `never` yields `never` directly;
- substituting `boolean` distributes over `true | false` (tsc treats `boolean`
  as `true | false` for distribution), evaluating each branch and unioning;
- substituting a union (or a type that evaluates to a union) distributes over
  each member — but *without* evaluating the per-member conditionals here,
  because the instantiator's `NoopResolver` can't resolve `Lazy` types in the
  check/extends positions; the unevaluated conditionals are unioned and handed
  back to the resolver-backed caller. Distribution is capped at
  `MAX_CONDITIONAL_DISTRIBUTION_SIZE` (shared with the evaluation path) to prevent
  OOM on thousand-member literal unions; exceeding it sets `depth_exceeded` and
  returns `ERROR`. The per-member substitution map is reused (only the
  distributed key is overwritten) instead of cloned per member.

Otherwise it instantiates all four parts and re-interns only on change.

### `Mapped` (`mapped.rs`)

The mapped arm is the largest. It enters a shadowing scope for the mapped type's
iteration variable, then handles homomorphic special cases *before* falling back
to standard constraint/template substitution, because standard substitution would
collapse `keyof T` to a flat union and lose the homomorphic structure. In order:

1. `rewrite_single_key_self_indexed_template` — for `{ [Q in P]: T[P] }` whose
   constraint parameter `P` is substituted by a single key, rewrite `T[P]` to
   `T[Q]` so `T`'s per-key `readonly`/optional modifiers are inherited (the
   ts-essentials `ReadonlyKeys` / `WritableKeys` substrate).
2. **Homomorphic union distribution** — `{ [K in keyof T]: ... }` with `T`
   substituted by a non-array-like union distributes into a union of per-member
   mapped types, shadowing the iteration variable per member.
3. **`any` source over array/tuple constraint** — mirrors tsc's
   `instantiateMappedArrayType`: produce an `Array<template[K:=number]>` result;
   identity templates (`T[K]`) over `any` return `any`, non-identity templates
   (`Box<T[K]>`) fall through to `{ [x: string]: Box<any> }`.
4. **Tuple source** — `instantiateMappedTupleType`: per-element rebinding with
   four cases (`RestArray`, `OpaqueRest`, `SuffixFixed`, `PrefixFixed`) so a
   fixed element after a rest is not widened to the union of all element types
   and a `...E[]` rest stays array-shaped.
5. **Array source** — `instantiateMappedArrayType`: `Array<template[K:=number]>`,
   preserving source `readonly` via `resolve_readonly`.
6. **Primitive source** — passes through unchanged when the template is the
   identity index.
7. `try_expand_substituted_homomorphic_object_mapped` (`homomorphic.rs`) for an
   object/callable source.

If none apply, the standard path instantiates constraint, template, name type,
and the iteration parameter's own constraint/default (with
`preserve_unsubstituted_type_params` temporarily forced on so the iteration key
stays a parameter), then re-interns. Eager evaluation of the resulting mapped
type is *skipped* — leaving a `MappedType` for the outer evaluator — under any of
these resolver-dependent conditions: `preserve_meta_types`,
`conditional_condition_needs_resolver(template)`,
`template_has_lazy_application_in_composite(template)`, a `Lazy` application in
`name_type`, `mapped_constraint_needs_resolver(constraint)`, or the constraint
still containing type parameters (e.g. `keyof __infer_0` mid-inference, where
premature evaluation would destroy the homomorphic pattern reverse-mapped
inference needs).

These deferral gates (`api.rs`) are the contract between the instantiator and
the evaluator: the instantiator handles everything the `NoopResolver` can
correctly reduce, and explicitly hands back anything that needs alias/application
expansion to `solver-evaluation`.

## The `this`-type mapper

`ThisType` is substituted via the `this_type` field. Three public entries shape
*how deep* the substitution goes:

- `substitute_this_type` — deep substitution through object/method internals,
  with `preserve_unsubstituted_type_params` on; used for class-inheritance
  specialization (heritage merge).
- `substitute_this_type_at_return_position` — sets `shallow_this_only`, so only
  combinator positions (intersection/union/index-access/keyof/conditional/
  application) get `this` rewritten while named object and method internals stay
  raw. This keeps a method body's polymorphic `this` re-bindable at the call site,
  which is required for the chained `extend({a}).extend({b})` pattern in
  `intersectionThisTypes.ts`. In this mode the `Object`/`ObjectWithIndex` arms
  short-circuit when the shape has a backing symbol, and `instantiate_function` /
  `instantiate_callable` rewrite only top-level `ThisType` slots.

## The public entry surface (`api.rs`)

All public entries are thin wrappers that set `InstantiationOptions` and route
through one staged engine. The cached/uncached split: a `_cached` entry takes
`Option<&dyn QueryDatabase>` and consults the cross-call cache when `Some`; the
plain entry passes `None`.

| Entry | Options / behavior |
| --- | --- |
| `instantiate_type` / `instantiate_type_cached` | default options; leaf fast paths for `TypeParameter` and `IndexAccess(T,P)` before any cache-key build |
| `instantiate_generic` / `instantiate_generic_cached` | build substitution via `from_args`, skip if identity (`is_identity_for`), then run the full walk; enables the alpha cache |
| `instantiate_type_preserving` / `..._cached` | `preserve_unsubstituted_type_params` (mapped-type bodies keep `P` as a parameter) |
| `instantiate_type_preserving_meta` / `..._cached` | `preserve_meta_types` (keep `keyof`/indexed/mapped un-reduced) |
| `instantiate_type_with_infer` / `..._cached` | `substitute_infer` (also substitute `Infer` placeholders) |
| `instantiate_type_with_depth_status` | uncached; returns `(TypeId, bool)` overflow flag for recursion-sensitive callers |
| `instantiate_type_with_request` | run an explicit `InstantiationRequest` uncached, returning a typed `InstantiationResult` |
| `substitute_this_type` / `..._cached` | deep `this` substitution |
| `substitute_this_type_at_return_position` | shallow `this` substitution |
| `instantiate_function_with_type_args` | instantiate a generic function shape to a non-generic one (JSX `<Comp<number> />`) |
| `instantiate_type_params_to_constraints` | error-recovery: map every reachable parameter to its constraint after failed overload resolution |

`instantiate_type_cached` carries allocation-free leaf shortcuts that run *before*
any cache-key construction: a direct `TypeParameter` hit returns
`substitution.get(name)` immediately, and a top-level `IndexAccess(obj, idx)`
recursively instantiates the two operands without building a `TypeInstantiator`.
After those, `can_skip_concrete_instantiation` walks the type to decide whether it
is fully concrete *and* contains no meta-type that the instantiator would
normalize; if so the original id is returned. This check is intentionally
narrower than "no type parameters" because the instantiator normalizes concrete
`keyof`/indexed/mapped/template/string-intrinsic shapes even when the
substitution cannot touch their leaves.

`instantiate_generic_cached` deliberately routes through
`instantiate_with_request_cached` (the full walk) rather than
`instantiate_type_cached`, because the latter's top-level `IndexAccess` fast path
returns a raw `IndexAccess` without the eager `evaluate_index_access` step that a
`T[K]`-bodied alias needs for mapped/keyof conformance.

### `InstantiationRequest` / `InstantiationResult` / options

`InstantiationOptions` (`request.rs`) is a four-flag set
(`substitute_infer`, `preserve_meta_types`, `preserve_unsubstituted_type_params`,
`shallow_this_only`) generated by the `solver_options!` macro, with a bespoke
`mode_bits() -> u8` packing (`0b0001`..`0b1000`) pinned by a test so the cache
wire format can't silently drift. `InstantiationRequest` bundles
`(type_id, &substitution, options, this_type)` and exposes `cache_key()` which
canonicalizes the substitution and packs the options. `InstantiationResult`
(`result.rs`) carries `(type_id, overflowed)`; on overflow it keeps the
*relation-preserving partial type* from the walk (`overflow_with`) rather than
collapsing to `ERROR`, and the `overflowed` flag tells the cache to refuse to
memoize a budget-limited result.

## Caches and invariants

### The cross-call `InstantiationCache`

`InstantiationCache` (`caches/instantiation_cache.rs`) is an
`FxHashMap<InstantiationCacheKey, TypeId>` in a `RefCell`, owned by `QueryCache`.
The key:

```text
InstantiationCacheKey = (
    type_id:   TypeId,            // the body being substituted into
    subst:     CanonicalSubst,    // SmallVec<[(Atom, TypeId); 4]> sorted by Atom
    mode_bits: u8,                // the 4-flag walk shape
    this_type: Option<TypeId>,    // for substitute_this_type (empty subst)
)
```

| Invariant | Where enforced |
| --- | --- |
| The substitution is order-independent: two `TypeSubstitution`s with the same `{name -> type_id}` set hash/compare equal | `canonical_pairs` sorts by `Atom`; `CanonicalSubst` derives `Hash`/`Eq` on the sorted `SmallVec` |
| Different walk modes never alias for the same `(type_id, subst)` | `mode_bits` is part of the key (test `mode_bits_match_legacy_constants`) |
| `substitute_this_type` calls (empty subst, distinct receivers) never alias | `this_type` is a separate key slot |
| The cache lives only for one check session and is the authoritative invalidation boundary | `QueryCache::clear()` calls `instantiation_cache.clear()`; the cache is never on the long-lived `TypeInterner` |
| Raw `TypeDatabase` callers (no `QueryCache`) always miss and never mutate counters | `QueryDatabase::lookup_instantiation_cache` / `insert_instantiation_cache` default to `None` / no-op (`caches/db.rs`) |
| A depth/frame-overflowed walk is never memoized | `instantiate_with_request_cached` only inserts when `!result.depth_exceeded()` |

The probe/fill lives in `instantiate_with_request_cached` (`api.rs`): with a
`query_db`, it builds the key, probes `lookup_instantiation_cache`, optionally
probes the alpha key, runs `run_instantiator` on a miss, and inserts on success.

### Alpha-renaming: sharing slots across structurally-equal substitutions

`instantiate_generic_cached` passes `allow_alpha_cache = true`. The alpha layer
(`alpha_instantiation_cache_key`, `alpha_canonicalize_type`, `restore_alpha_type`)
rewrites the *free, unconstrained* type parameters in the substitution's values to
positional `TypeData::BoundParameter(index)` nodes, recording the original bindings.
This makes two substitutions that differ only in the *names* of free parameters
they bind hash to the same alpha key, so a previously-computed result can be
recovered (`restore_alpha_result` re-substitutes the bound indices) instead of
re-walking the body. On a miss, after computing the real result, the engine also
stores the alpha-canonicalized result under the alpha key
(`alpha_canonicalize_cached_result`) so future structurally-equal requests hit.
Alpha canonicalization bails (returns `None`, disabling the optimization) for
shapes it can't safely re-key — constrained parameters, `Function`/`Callable`,
`Mapped`, `Application`, `Enum`, `Infer`, `TemplateLiteral` — preserving
correctness over coverage.

### Operation-local caches

Inside one walk, `instantiate_properties_if_changed` (`display_properties.rs`)
uses a local `FxHashMap<TypeId, TypeId>` memo (only when a shape has >= 8
properties) so identical property slot types are instantiated once. The
constraint-collection helper `instantiate_type_params_to_constraints` reuses a
thread-local `FxHashSet<TypeId>` visited pool. The `ApplicationEvaluator`
(`application.rs`) carries its own per-evaluator `FxHashMap<TypeId, TypeId>`
result cache, dropped with the evaluator and never shared across resolver modes.

## How instantiation interacts with `DefId`, `Lazy`, and `Application`

The instantiator never resolves a semantic reference. `TypeData::Lazy(DefId)` is
a leaf and `Application` is rewritten structurally. The bridge from a generic
*reference* to its instantiated body is `ApplicationEvaluator` (`application.rs`),
which the resolver-backed evaluator drives:

```text
ApplicationEvaluator::evaluate( Box<string> ):
  is_generic_type?  ----------------------------------- no  -> NotApplication
  cache hit?  ----------------------------------------- yes -> Resolved(cached)
  RecursionGuard::enter (cycle / depth via RecursionProfile::TypeApplication)
  evaluate_inner:
    get_application_info -> (base = Lazy(DefId of Box), args = [string])
    resolver.resolve_lazy(DefId)        -> body  { value: T }
    resolver.get_lazy_type_params(DefId) -> [T]
    subst = TypeSubstitution::from_args([T], [string])
    instantiated = instantiate_type(body, subst)        // <- the engine
    if contains_this_type(instantiated): substitute_this_type(instantiated, Box<string>)
    recurse evaluate(instantiated) for nested applications
  guard.leave; cache Resolved
```

The resolver (`TypeResolver`) is the abstraction that keeps the solver
independent of the binder/checker: `resolve_lazy(def_id)` returns the alias body,
`get_lazy_type_params(def_id)` returns the parameters. Crucially,
`evaluate_inner` substitutes type arguments *raw* (no eager evaluation of the
args before substitution), because eagerly evaluating an `Application` argument
would erase the structural identity an `infer V` match needs — matching tsc,
which substitutes raw and evaluates nested applications lazily.

This is the DefId-resolution interaction the checker depends on: the checker
stabilizes a `DefId`, the `TypeEnvironment` resolves it to a `TypeId`, and an
`Application(Lazy(DefId), args)` becomes concrete only when something forces
evaluation — at which point `ApplicationEvaluator` instantiates the resolved
body. See [solver-types-intern-def](solver-types-intern-def.md) for the
`Lazy`/`DefId` mechanics and [checker-context-and-state](checker-context-and-state.md)
for the two `TypeEnvironment`s.

## Recursion, fuel, and the bail value

Instantiation participates in several stacked guards (the full table lives in
`crates/tsz-solver/src/limits/mod.rs`); the ones the instantiator owns or hits:

| Guard | Value | Scope | On exhaustion |
| --- | --- | --- | --- |
| `TypeInstantiator.depth` vs `MAX_TYPE_SUBSTITUTION_DEPTH` | 50 | per-instance substitution walk | sticky `depth_exceeded`; `bail_value` |
| `with_solver_frame` / `MAX_SOLVER_STACK_FRAMES` | 2000 live frames | thread-local, cross-operation | sets `depth_exceeded`; `bail_value` |
| `MAX_REPRESENTABLE_TUPLE_LENGTH` | 10,000 | tuple-spread materialization | `mark_tuple_too_large`; `TypeId::ERROR` |
| `MAX_TUPLE_SPREAD_FLATTEN_ELEMENTS` | 8,192 | tuple-spread soft gate | keep spread as one rest element |
| `MAX_CONDITIONAL_DISTRIBUTION_SIZE` | (shared) | distributive conditional members | `depth_exceeded`; `ERROR` |

Note the **deliberate name collision**: the *solver* constant
`crate::limits::MAX_TYPE_SUBSTITUTION_DEPTH` (50) is re-exported by the
instantiator as `MAX_INSTANTIATION_DEPTH`, but the *checker-side*
`tsz_common::limits::MAX_INSTANTIATION_DEPTH` is 100 (tsc's `instantiationDepth`,
the TS2589 trigger). They are different limits in different crates; aligning them
would be a behavior change needing its own witness. The solver value bounds the
structural substitution walk; TS2589 escalation lives at the checker boundary and
in the `EvaluationSession` counters, not here.

`bail_value` (`instantiate.rs`) is the parity-critical recovery. The historical
sentinel was `TypeId::ERROR`, but that *dropped the active substitution*: a
downstream consumer (e.g. iterator-element resolution on a fully-concrete
`Map<K, V>`) then fell back to the original un-instantiated element type and
surfaced a bare bound `T` into a concrete context, producing false TS2488/TS2345
(`#13652`). `bail_value` instead applies only the *head* substitution:

- a bailing `TypeParameter` bound by the substitution resolves to its binding
  (`T` with `{T: number}` becomes `number`, never a leaked `T`);
- a `TypeParameter` *not* bound stays unchanged (a genuinely free parameter — a
  truly generic iteration still reports its diagnostic);
- any other shape is returned opaque (un-walked, relation-preserving).

This mirrors tsc returning the type un-instantiated/deferred at its depth cap,
with the guarantee that no substitution-domain parameter ever escapes.

## A worked example: `instantiate_generic` over a generic alias

Take `type Pair<A, B> = { first: A; second: B }` applied as `Pair<number, string>`.

```text
instantiate_generic(interner, body, [A, B], [number, string])
  body intrinsic? no; params/args empty? no
  subst = TypeSubstitution::from_args([A, B], [number, string])
        = { A -> number, B -> string }            // phase 1, no defaults
  subst.is_identity_for([A,B])? no
  instantiate_with_request_cached(allow_alpha = true, request{ body, subst })
    cache key = (body, [(A,number),(B,string)] sorted, mode 0, this None)
    miss -> run_instantiator:
      instantiate(body)  -> instantiate_inner -> Object arm
        instantiate_properties_if_changed:
          first:  instantiate(A) -> TypeParameter arm -> subst.get(A) = number
          second: instantiate(B) -> TypeParameter arm -> subst.get(B) = string
        properties changed -> object_with_flags_and_symbol -> { first: number, second: string }
    insert (body-key -> {first:number, second:string})
    alpha key (A,B free) -> store alpha-canonical result for name-agnostic reuse
  return { first: number; second: string }
```

Because the arguments here are *concrete* (`number`, `string`),
`alpha_canonicalize_type` leaves them untouched and `changed` stays false, so no
alpha key is built — only the exact-key slot is used. The alpha layer pays off
for a body applied with *free* parameters: `Pair<X, Y>` canonicalizes its
substitution values `X`, `Y` to positional `BoundParameter(0)`,
`BoundParameter(1)`, so `Pair<X, Y>` and `Pair<P, Q>` (both binding two free,
unconstrained parameters) share one alpha slot, while `Pair<number, string>`
keeps its own exact key.

## Edge cases and tsc parity

- **Declaration identity (`#13044`)**: an unmapped or shadowed `TypeParameter` is
  returned as its *original* `TypeId`, never structurally re-interned. Fresh
  declaration-scoped parameters share a name but not an id; a re-intern would
  collapse them to the structural canonical and split identity between
  instantiated and never-instantiated mentions. The same discipline governs
  pushing own params into `local_type_params` only when their infos changed
  (`signatures.rs`).
- **`ERROR`-sentinel arguments (`#13044`/`#13484`)**: in `from_args`, a
  `TypeId::ERROR` argument (the internal cycle/fuel sentinel from a mid-resolution
  base-class chain) is treated as *unsupplied* and bound to `any`, not baked in as
  `error`. Binding `error` would poison a cross-arena base-class instance
  (`SelectFrom<error, ...>`); leaving it free would leak into a contextual
  signature. `any` (the no-candidate fallback) is what tsc effectively produces.
- **Concrete-conditional default collapse**: in `from_args`,
  `maybe_evaluate_concrete_conditional` resolves a default like
  `K extends string ? Map<K,V> : Map<string,V>` (with concrete `K`, `V`) to the
  picked branch *unevaluated*, preserving the `Application` `TypeId` so the
  default's `Map<string, number>` is the same interned id the source expression
  produces — subtype comparison then succeeds without structural expansion.
- **Distributive `boolean`/`never`/union**: `never` short-circuits to `never`,
  `boolean` distributes over `true | false`, a union distributes per member —
  matching tsc's `distributeConditionalType`, but capped to avoid OOM on huge
  literal unions.
- **Homomorphic array/tuple/union**: the mapped arm reproduces tsc's
  `instantiateMappedArrayType` / `instantiateMappedTupleType` element-by-element,
  including the four-case tuple rebinding so a fixed slot after a rest is not
  widened to the union of all element types.
- **Modifier inheritance for self-indexed mapped types**:
  `rewrite_single_key_self_indexed_template` keeps `T`'s per-key
  `readonly`/optional modifiers when `P := "k"` would otherwise erase the
  `keyof T` link — the ts-essentials `ReadonlyKeys`/`WritableKeys` substrate.
- **Resolver-deferral correctness**: a mapped/keyof/conditional whose body reaches
  a `Lazy(DefId)` application is left un-reduced so the resolver-backed evaluator
  expands it; reducing under the `NoopResolver` would silently drop union members
  or collapse a filter to `never` (e.g. `tsxLibraryManagedAttributes`,
  recursive `Spec<T[P]>`).
- **Depth bail leak-freedom (`#13652`)**: `bail_value` and `InstantiationResult::overflow_with`
  ensure a budget-limited walk hands back a relation-preserving partial type, not a
  leaked free parameter or a fallback to the un-instantiated original.

## Cross-references

- [solver-evaluation](solver-evaluation.md) — the resolver-backed `TypeEvaluator`
  the instantiator defers meta-types to, and the `EvaluationSession` counters.
- [solver-inference](solver-inference.md) — where `__infer_*` placeholders and the
  argument substitutions come from.
- [solver-types-intern-def](solver-types-intern-def.md) — `TypeId`, `TypeData`,
  `DefId`/`Lazy`, interning, and the meta-type re-intern surface.
- [solver-caches-objects-contextual-compat](solver-caches-objects-contextual-compat.md)
  — `QueryCache` and the surrounding cache family.
- [solver-relations](solver-relations.md) — the subtype check
  `maybe_evaluate_concrete_conditional` and homomorphic gates call into.
- [checker-calls-signatures-generics](checker-calls-signatures-generics.md) —
  how generic call instantiation reaches this subsystem.
- [checker-context-and-state](checker-context-and-state.md) — the
  `TypeEnvironment` `DefId -> TypeId` resolution boundary.
