# Mapped-Type and Tuple Shards: Homomorphic Mapping, Key Remapping, Tuple Rebinding

## Orientation

[solver-evaluation](solver-evaluation.md) and [solver-instantiation](solver-instantiation.md)
both touch mapped types at the *boundary* level: evaluation says "a `TypeData::Mapped`
visits `evaluate_mapped`," instantiation says "a `TypeData::Mapped` arm runs
`instantiate_mapped`." Neither goes inside. This doc fills that gap. It is the
kernel walkthrough of the two shards that actually turn a `MappedType` node into a
concrete object, array, or tuple: the **evaluation** shard under
`crates/tsz-solver/src/evaluation/evaluate_rules/` (`mapped.rs`, `mapped_array.rs`,
the `mapped/` subdir, `mapped_template_index.rs`) and the **instantiation** shard under
`crates/tsz-solver/src/instantiation/instantiate/` (`mapped.rs`, `homomorphic.rs`).

These two shards are siblings that solve almost the same problem from two sides.
The instantiation shard (`TypeInstantiator`, `NoopResolver`-backed, no `TypeResolver`)
runs *during* generic substitution — `Partial<T>` with `T := [number, string]` — and
must decide homomorphic special cases *before* `keyof T` collapses to a flat union.
The evaluation shard (`TypeEvaluator<'a, R: TypeResolver>`) runs *after*, when a
mapped node reaches `visit_mapped` and needs to become a property bag. They share
the same four-case tuple-rebinding switch (`RestArray` / `OpaqueRest` / `SuffixFixed`
/ `PrefixFixed`), the same `compute_mapped_modifiers` arithmetic in
`tsz_solver::type_queries::mapped`, and the same `MappedType::resolve_readonly`
rule. This doc traces both, names where they agree and where they intentionally
differ, and ends with the cache/guard invariants and the `tsc`-parity edge cases
that justify the special cases.

It extends — and does not re-explain — [solver-types-intern-def](solver-types-intern-def.md)
(the `MappedType` / `TupleElement` / `TypeData` shapes), [solver-relations](solver-relations.md)
(how a *deferred* mapped target is checked structurally without expansion), and
[solver-inference](solver-inference.md) (reverse-mapped inference, which depends on
the opaque-rest preservation rule below). For how the checker drives mapped
evaluation across files, see [checker-type-of-symbol-and-symbol-types](checker-type-of-symbol-and-symbol-types.md)
and [checker-declarations-modules](checker-declarations-modules.md).

## Owns / Must not own

| Owns | Must not own |
| --- | --- |
| Turning a `MappedType` node into a concrete `Object` / `ObjectWithIndex` / `Array` / `Tuple` (`evaluate_mapped`). | Deciding *where* a mapped type appears or *which symbol* it types (checker / binder). |
| The homomorphic short-circuits during substitution (`instantiate_mapped`, `homomorphic.rs`). | Diagnostic text, source spans, or which `TS` code fires (checker + error reporter). |
| Key remapping (`as` clauses): substitute `name_type`, evaluate, filter `never` (`remap_key_type_for_mapped`). | Reading printer output as a predicate, or constructing raw `TypeKey` from the checker. |
| The four-case tuple-element rebinding and rest flattening. | Relation/assignability for *deferred* mapped types — that is [solver-relations](solver-relations.md) (`try_expand_mapped`, `mapped_target.rs`). |
| Modifier resolution (`+?`/`-?`/`+readonly`/`-readonly`) via `compute_mapped_modifiers` + `resolve_readonly`. | The `keyof` evaluation kernel itself (`keyof.rs`); this shard only *consumes* keys. |
| Mapped-key extraction (`extract_mapped_keys`) and its re-entrancy guard. | Persisting eval results across compilations (interner-instance-local; see Caches). |

## File map

### Evaluation shard (`TypeEvaluator<'a, R: TypeResolver>`)

| Path | Role |
| --- | --- |
| `evaluation/evaluate_rules/mapped.rs` | `evaluate_mapped` entry; deferral gates, distribution, homomorphic source detection, the per-key property-build loops, `as`-clause key remapping, index-signature build, `try_evaluate_mapped_with_as_over_non_literal_constraint`. |
| `evaluation/evaluate_rules/mapped_array.rs` | Homomorphic array/tuple sources: `evaluate_mapped_array*`, `evaluate_mapped_tuple*`, `evaluate_mapped_tuple_element` (the four-case switch on the evaluation side), `rebind_mapped_source`, `evaluate_mapped_over_readonly_source`. |
| `evaluation/evaluate_rules/mapped/key_extraction.rs` | `extract_mapped_keys_impl`, `homomorphic_mapped_source`, `extract_source_from_keyof`, `post_instantiation_mapped_template_source`, `extract_template_index_source`, `mapped_key_from_property/_literal`. |
| `evaluation/evaluate_rules/mapped/keys_guard.rs` | `MappedKeysVisitGuard` — thread-local RAII re-entrancy guard for the key-extraction walk (#13368). |
| `evaluation/evaluate_rules/mapped/keyof_constraint.rs` | `evaluate_keyof_or_constraint` — union/intersection key-set reduction with its own `keyof_constraint_guard` + `stacker`/`with_solver_frame` stack guard. |
| `evaluation/evaluate_rules/mapped/key_types.rs` | `MappedKey` (atom + substitution `TypeId`) and `MappedKeys` (string keys, `has_string`/`has_number`, template-literal keys, symbol keys). |
| `evaluation/evaluate_rules/mapped/keyof_constraint.rs`, `key_extraction.rs` | The `keyof T` → concrete-key-set reduction feeding the iteration loop. |
| `evaluation/evaluate_rules/mapped_template_index.rs` | `try_evaluate_mapped_template_per_concrete_key`, `try_evaluate_remapped_mapped_template_for_index` — `mapped[index]` shortcuts used by `index_access`. |

### Instantiation shard (`TypeInstantiator<'a>`, no resolver)

| Path | Role |
| --- | --- |
| `instantiation/instantiate/mapped.rs` | `instantiate_mapped` — homomorphic union distribution, `any`-source array results, the four-case tuple rebinding (`ElemBinding` enum), array preservation, primitive pass-through, then standard constraint/template substitution + the eager-evaluation deferral gates. |
| `instantiation/instantiate/homomorphic.rs` | `try_expand_substituted_homomorphic_object_mapped` — expand `Partial<{...}>` to a concrete object during instantiation when safe (no Lazy-application / conditional / resolver hazard). |
| `instantiation/instantiate.rs` | `rewrite_single_key_self_indexed_template`, `extract_array_element`, `is_array_or_tuple_like`, `mapped_template_uses_source_index`, `is_primitive_or_primitive_union` — predicates used by `instantiate_mapped`. |

### Shared query helpers (`tsz_solver::type_queries::mapped`)

| Path | Role |
| --- | --- |
| `type_queries/mapped.rs` | `compute_mapped_modifiers`, `merge_colliding_mapped_properties`, `is_identity_name_mapping`, `classify_mapped_source` / `MappedSourceKind`, `classify_identity_mapped`, `evaluate_identity_mapped_passthrough`, `expand_mapped_type_to_properties`, the finite-property helpers (`get_finite_mapped_property_type*`), `reconstruct_mapped_with_constraint`, `remapped_mapped_index_access_result`. |
| `type_queries/mapped_display_order.rs` | `collect_homomorphic_source_property_infos` — source property order for `keyof T` display parity. |
| `types.rs` | `MappedType`, `MappedModifier`, `MappedType::resolve_readonly`, `TupleElement`. |

## The two entry points

```
            generic substitution                       structural evaluation
            (TypeInstantiator, NoopResolver)            (TypeEvaluator<R: TypeResolver>)
                     |                                            |
   instantiate_key  TypeData::Mapped                visit_mapped (evaluate/support.rs)
                     |                                            |
            instantiate_mapped                          evaluate_mapped
        (instantiate/mapped.rs)                    (evaluate_rules/mapped.rs)
                     |                                            |
   homomorphic short-circuits BEFORE                deferral gates, then
   keyof T collapses to a union:                    concrete key iteration:
     - union distribution                             - is_mapped_type_over_type_parameter? defer
     - any-source -> array                            - try_distribute_mapped_over_composite_source
     - tuple/array preservation                       - try_reduce_substituted_homomorphic_mapped
       (ElemBinding 4-case)                           - evaluate_keyof_or_constraint -> MappedKeys
     - primitive pass-through                         - homomorphic array/tuple? evaluate_mapped_array/_tuple
     - homomorphic object expand                      - else per-key property build loop
       (homomorphic.rs)                                 (string keys, then symbol keys)
                     |                                            |
   else: standard substitution +                     build Object / ObjectWithIndex
   eager-evaluation deferral gates                    (merge collisions, sort for display)
```

`instantiate_mapped` runs first when a generic like `Partial<T>` is *applied*; it
either returns a finished shape (the homomorphic cases) or hands a substituted
`MappedType` to `evaluate_mapped` (the final `self.evaluate_type(mapped_type)` at
the bottom of `instantiate_mapped`, gated by the deferral checks). `evaluate_mapped`
also runs standalone when a directly-authored `{ [K in keyof X]: ... }` reaches the
evaluator. The split exists because the instantiator has no `TypeResolver` (it is
`NoopResolver`-backed) and therefore must *defer* anything whose expansion needs
cross-file `Lazy(DefId)` resolution; the evaluator, carrying a real resolver, can
finish those.

## evaluate_mapped: the deferral cascade

`evaluate_mapped` (`mapped.rs`) is a cascade of "can I make this concrete?" gates,
each falling back to `self.interner().mapped(*mapped)` (re-intern the same deferred
node) when it cannot. In order:

1. **Depth guard.** `if self.is_depth_exceeded() { return TypeId::ERROR; }` — the
   evaluator's `RecursionGuard` (profile `TypeEvaluation`, depth 100) may already
   be exhausted from the caller.

2. **Generic remapped defer.** If the mapped has a `name_type` (`as` clause) and
   either the constraint or the `name_type` still contains free type parameters
   (beyond the iteration variable), re-intern deferred. `as` clauses cannot be
   evaluated key-by-key while `T` is generic without losing the `P ↔ F<P> ↔ template`
   correlation that diagnostics need.

3. **Mapped over a type parameter.** `is_mapped_type_over_type_parameter` →
   `constraint_has_keyof_type_param`: when the constraint is `keyof T` (or
   `keyof Partial<T>`, recursing through inner mapped types) for a *type parameter*
   `T`, defer — **unless** `try_evaluate_mapped_over_array_param` finds `T` is
   constrained to an array/tuple, in which case produce the array/tuple shape
   directly (`tsc`'s `instantiateMappedArrayType`/`instantiateMappedTupleType`).
   Intersection constraints (`keyof T & keyof C`) are *excluded* here so the later
   composite-distribution path can run.

4. **Composite-source distribution.** `try_distribute_mapped_over_composite_source`:
   when an instantiated homomorphic source resolves to `A | B` or `A & B` (and the
   *effective* constraint differs from the declared one, i.e. this is an
   instantiated form, not directly-written), distribute:
   `Partial<A | B>` → `Partial<A> | Partial<B>`, `Partial<A & B>` →
   `Partial<A> & Partial<B>`. The shared loop body `distribute_mapped_over_members`
   re-interns one per-member `MappedType` (with `source → member` substituted across
   `template`/`name_type`) and routes each through the cached `evaluate`, so
   identical instantiations collapse on the memo and the recursion guard defers a
   self-referential member instead of diverging.

5. **Primitive short-circuit.** `try_reduce_substituted_homomorphic_mapped`: a
   *generic* homomorphic `Meta<X>` instantiated with a non-object `X` (primitive,
   literal, `never`, unique symbol, enum) reduces to `X`. `is_mapped_short_circuit_source`
   encodes the complement of `tsc`'s `AnyOrUnknown | InstantiableNonPrimitive |
   Object | Intersection` in `instantiateMappedType`. The gate inspects the iteration
   variable's *original* constraint (`type_param.constraint` must be `keyof <TypeParameter>`)
   to distinguish this from a directly-written `{ [K in keyof string]: ... }`, whose
   constraint is `keyof X`, not `keyof <TypeParameter>`.

6. **Key extraction.** `evaluate_keyof_or_constraint(constraint)` then
   `try_extract_keyof_keys_for_mapped_iteration` / `extract_mapped_keys`. If keys
   cannot be made concrete, either fall to `try_evaluate_mapped_with_as_over_non_literal_constraint`
   (iterate constraint *members* directly for `{ [Item in (ObjA|ObjB) as Item['name']]: ... }`)
   or defer.

7. **Key-count limit.** `if key_set.keys.len() + key_set.symbol_keys.len() >
   self.max_mapped_keys()` → `mark_depth_exceeded(); return TypeId::ERROR`.
   `max_mapped_keys()` is `DEFAULT_MAX_MAPPED_KEYS` (500 native, 250 on `wasm32`).

Only past gate 7 does `evaluate_mapped` commit to materializing properties.

## Homomorphic source detection

Two flavors of "homomorphic" matter, both computed in `mapped.rs` around the start
of the materialization phase:

- `is_identity_homomorphic = homomorphic_mapped_source(mapped).is_some()` — the
  *strict* form: constraint is `keyof T` **and** template is exactly `T[K]`.
  `homomorphic_mapped_source` (`key_extraction.rs`) implements two detection methods:
  Method 1 matches the pre-evaluation `keyof T` + `IndexAccess(T, K)` form; Method 2
  matches the post-instantiation form where `keyof T` already collapsed to a literal
  union, by recomputing `evaluate_keyof(obj)` and comparing (exact match for any
  `as`-clause; subset match only for identity name mappings — `Pick`/`Omit` produce
  a filtered subset).

- `is_homomorphic = source_object.is_some()` — the *loose* form: any source `T`
  whose modifiers should propagate, even when the template is not `T[K]`. The
  `source_object` is resolved from `homomorphic_mapped_source`, else
  `extract_source_from_keyof(constraint)`, else `post_instantiation_mapped_template_source`.
  This is what makes `type M1 = { [K in keyof Partial<M0>]: M0[K] }` inherit
  `Partial<M0>`'s optionality even though the template reads `M0[K]`, not
  `Partial<M0>[K]`.

The pair `is_identity_homomorphic || is_homomorphic` is the full
modifier-inheritance condition passed to `compute_mapped_modifiers` for every key.

`should_use_declared_source_property_type` (`is_identity_homomorphic ||
template_reads_source_property`) is a separate fast path: for an identity `T[K]`
template, the source property's *declared* type already equals
`read-type − undefined`, so the loop returns it directly and skips the
`instantiate + evaluate` cycle — except when `source_has_type_params` (the source is
a type parameter or an intersection containing one, where `collect_properties` cannot
capture the deferred index-access constraints).

## Walk-through 1: `Partial<{ a: number; b?: string }>`

`Partial<T> = { [P in keyof T]?: T[P] }` applied to `{ a: number; b?: string }`.

1. `instantiate_mapped` substitutes `T := { a: number; b?: string }`. The constraint
   `keyof T` is `KeyOf(TypeParameter T)`; `T` is bound in the substitution and the
   resolved source is an `Object`, not array/tuple/union/`any`/primitive, so none of
   the early homomorphic blocks fire. It reaches
   `try_expand_substituted_homomorphic_object_mapped` (`homomorphic.rs`), which —
   because `is_identity_name_mapping` holds and there is no Lazy-application or
   conditional hazard — re-interns the mapped with `constraint = keyof {a,b}` and
   `template = {a,b}[P]`, then calls `evaluate_type`. (If that path declines, the
   bottom-of-function `self.evaluate_type(mapped_type)` runs the same evaluator.)

2. `evaluate_mapped` passes every deferral gate. `evaluate_keyof_or_constraint`
   reduces `keyof {a,b}` to `"a" | "b"`; `extract_mapped_keys` yields
   `MappedKeys { keys: ["a","b"], has_string: false, ... }`.

3. `homomorphic_mapped_source` matches Method 1 →
   `source_object = Some({ a: number; b?: string })`, `is_identity_homomorphic = true`.
   `is_identity_name_mapping` is true and the resolved source is an `Object` (not
   array/tuple), so the array/tuple-preservation block is skipped.

4. Source properties are memoized into `source_prop_map` via
   `collect_homomorphic_source_property_infos` (declaration-ordered). `a`: `(optional=false,
   readonly=false, number)`; `b`: `(optional=true, readonly=false, string)`.

5. Per-key loop. For `"a"`: `remap_key_type_for_mapped` returns `"a"` unchanged (no
   `name_type`). `compute_mapped_modifiers(mapped, is_homomorphic=true,
   source_optional=false, source_readonly=false)` with `optional_modifier = Some(Add)`
   → `(optional=true, readonly=false)`. `should_use_declared_source_property_type`
   holds → `property_type = number` (declared). For `"b"`: same, `property_type = string`.
   `homomorphic_removes_optional` is false (`+?`, not `-?`), so no undefined-strip.

6. Result: `{ a?: number; b?: string }`, built with `self.interner().object(properties)`
   after `merge_colliding_mapped_properties` (no collisions) and
   `sort_mapped_properties_for_display`.

## Key remapping (`as` clauses)

`remap_key_type_for_mapped` (`mapped.rs`) is the single chokepoint for `as` clauses:

```rust
let Some(name_type) = mapped.name_type else { return Ok(Some(key_type)); };
let subst = TypeSubstitution::single(mapped.type_param.name, key_type);
let remapped = instantiate_type_preserving_cached(.., name_type, &subst);
let remapped = self.evaluate(remapped);
if remapped == TypeId::NEVER { return Ok(None); }   // key filtered out
Ok(Some(remapped))
```

`Ok(None)` (remapped to `never`) means the source key is *dropped* — this is how
`as` clauses implement key filtering (`{ [K in keyof T as Exclude<K, "x">]: T[K] }`).
The caller `continue`s past that key. `Err(())` means the remap could not be
processed; the caller defers the whole mapped type.

In the per-key loop the remapped key is decoded into one or more `MappedKey`s:
identity (`remapped == key_literal`), a single literal (`mapped_key_from_literal`),
or a union of literals (each member decoded; empty → defer). Symbol keys take the
parallel `key_set.symbol_keys` loop, decoding `UniqueSymbol` / union-of-`UniqueSymbol`
results. When several source keys remap to the *same* output name,
`merge_colliding_mapped_properties` (`type_queries/mapped.rs`) unions their value
contributions but keeps the **first** source key's modifier/naming metadata in
declaration order — matching `tsc`'s `resolveMappedTypeMembers`, which only updates
`keyType`/`nameType` on collision and never recomputes the property symbol's flags.

### Template-literal remapping for indexed reads

`mapped_template_index.rs` handles `mapped[index]` when the `as` clause produces
*open* template-literal patterns that cannot materialize as concrete properties.
For `` { [K in keyof T as `${K}${string}`]: T[K] }["axyz"] ``,
`try_evaluate_remapped_mapped_template_for_index` re-derives each source key, checks
the requested index against the remapped pattern via `remapped_key_matches_index`
(a real `SubtypeChecker`), and substitutes the *original* source key into the
template — keeping the correlation `tsc` preserves. The sibling
`try_evaluate_mapped_template_per_concrete_key` handles the all-literal-key case for
`mapped[keyof mapped]`.

## Tuple rebinding: the four-case switch

The hardest part of both shards is mapping over a tuple while preserving structure
(fixed/optional/rest/variadic/labeled elements). A naive `T[number]` would widen a
rest element to the union of *all* element types, and `T["i"]` is ambiguous for any
fixed element that follows a rest (index `i` could land in the rest range or a
suffix slot). Both shards solve this with the same per-element switch.

| Case | Condition | Rebind | Key | Result wrap |
| --- | --- | --- | --- | --- |
| `RestArray` | `rest` of `Array<E>` / `readonly E[]` | source → `Array<E>` | `number` | re-wrap result in `Array<>` |
| `OpaqueRest` | `rest` of anything else (type param, lazy ref) | none (keep `T[K]`) | `number` | keep evaluated form |
| `SuffixFixed` | fixed element *after* a seen rest | source → `[elem_type]` proxy | `"0"` | bare type |
| `PrefixFixed` | fixed element *before* any rest | none | `"<i>"` literal | bare type |

On the **instantiation** side (`instantiate/mapped.rs`) this is the explicit
`enum ElemBinding { RestArray(TypeId), OpaqueRest, SuffixFixed, PrefixFixed }` plus a
`rebind_source` closure that calls `substitute_exact_type_db(new_template, resolved,
new_source, ..)` to swap the resolved source tuple for the per-element proxy. On the
**evaluation** side (`mapped_array.rs::evaluate_mapped_tuple_element`) the same cases
appear as inline `if`/`else` arms using `evaluate_mapped_template_with_source_rebind`
and `substitute_exact_type`. They are deliberately kept in lockstep — the doc
comments on both cross-reference `tsc`'s `instantiateMappedTupleType`.

The key invariant for `OpaqueRest`: an opaque variadic rest (`...T`) must keep the
*source tuple* in the indexed access. Rewriting it to `T[number]` would lose the
relationship that **reverse inference** (see [solver-inference](solver-inference.md))
uses to infer `T` from mapped tuple rest elements. That is why `OpaqueRest` binds
`K = number` on the *existing* template instead of rebinding the source.

### Variadic spread of a tuple

A rest whose inner type is itself a `Tuple` (`[...[number, string]]`) is *not* one of
the four binding cases — it is handled earlier. `evaluate_mapped_tuple_element`
detects `rest_inner_kind == Tuple(inner)`, calls `rebind_mapped_source` to bind the
inner tuple as `T`, and **recurses** via `evaluate_mapped_tuple`, returning a tuple
in the rest's `type_id`. The outer `evaluate_mapped_tuple` then flattens that inner
tuple back into `mapped_elements` (the `mapped_element.rest && lookup == Tuple`
branch), guarded by `MAX_REPRESENTABLE_TUPLE_LENGTH` (10,000) — overflow calls
`mark_tuple_too_large()` and returns `TypeId::ERROR`.

### Optional and readonly on tuples

Per-element optionality: a rest **absorbs** `+?` as `inner | undefined` (a rest
cannot syntactically combine with `?`), while fixed elements toggle their per-element
`optional` flag (`Add` → true, `Remove` → false, `None` → keep `elem.optional`).
The `-?`-then-strip rule (`strip_removed_optional_undefined`) runs only for fixed
elements.

Tuple-level readonly is a property of the whole tuple via the `ReadonlyType` wrapper,
resolved once by `MappedType::resolve_readonly(source_readonly)`: `+readonly` →
readonly, `-readonly` → mutable, absent → copy the source's readonly-ness. The
evaluation side threads `original_source` vs `mapped_source` through
`evaluate_mapped_tuple_with_readonly_source`, calling `rebind_mapped_source` when the
two differ (the readonly-source case from `evaluate_mapped_over_readonly_source`).

## Walk-through 2: `MyPartial<[number, ...string[]]>`

`MyPartial<T> = { [P in keyof T]?: T[P] }` applied to `[number, ...string[]]`.

1. `instantiate_mapped`: `T := [number, ...string[]]`, the resolved source is a
   `Tuple`, `is_identity_name_mapping` holds, so the tuple block runs (`tuple_source
   = Some((tuple_id, false))`). `new_template = self.instantiate(template)` puts the
   resolved tuple wherever `T` appeared.

2. Element 0 (`number`, fixed, no rest seen): `PrefixFixed`. Key `"0"`,
   `new_template` unchanged. `subst = {P: "0"}`, `[number,...string[]]["0"]` =
   `number`. `optional_modifier = Add` → `(number, optional=true)`.

3. Element 1 (`...string[]`, rest of `Array<string>`): `RestArray(string[])`.
   `rebind_source(string[])` swaps the source to `string[]`, key `number`,
   `(string[])[number]` = `string`. The `RestArray` + `Add` arm wraps as
   `Array<string> | undefined`? — no: per the table, `RestArray` re-wraps in
   `Array<>` and the `Add` arm is `union2(array(string), undefined)`. Result element
   `type_id = string[] | undefined`, `rest = true`, `optional = elem.optional`.

4. `tuple_type = self.interner.tuple([num_elem, rest_elem])`. `resolve_readonly(false)`
   is false (no `+readonly`), so no `ReadonlyType` wrap. Final:
   `[(number | undefined)?, ...(string[] | undefined)]` — actually the fixed element
   stays `number` with `optional=true`, the rest carries `string[] | undefined`.
   Structure preserved; no widening to `(number | string)[]`.

## Homomorphic special cases during instantiation

`instantiate_mapped` carries four homomorphic short-circuits that must run *before*
`keyof T` collapses, all gated by `!self.preserve_meta_types` and
`is_identity_name_mapping`:

1. **Union distribution** — resolved source is a non-array/tuple `Union`: re-intern
   one mapped per member (shadowing the iteration variable across the splice so the
   constraint-fallback does not rewrite `K`), return `union(results)`. Primitive
   members short-circuit to themselves.

2. **`any`-source array result** — `T := any` with an array/tuple constraint:
   substitute `K → number`, wrap in `Array<>`. The guard
   `mapped_template_uses_source_index` + `is_identity_template` is load-bearing:
   `tsc` returns bare `any` *only* for an identity `T[K]` template; a non-identity
   `Box<T[K]>` falls through to `{ [x: string]: Box<any> }`. tsz must **not**
   unconditionally return `any`, or `Objectish<any>` would become assignable to
   `any[]`.

3. **Tuple / array preservation** — the `ElemBinding` switch (above) and
   `extract_array_element` for `Array` / `ReadonlyType(Array)` / `ObjectWithIndex`
   with a readonly numeric index.

4. **Primitive pass-through** — `template_uses_source_index &&
   is_primitive_or_primitive_union(resolved)` returns `resolved` unchanged.

Then `homomorphic.rs::try_expand_substituted_homomorphic_object_mapped` handles the
concrete-object case (Walk-through 1, step 1), declining when the template/`name_type`
carries a `Lazy` application or a `Conditional` the `NoopResolver` cannot expand.

### The eager-evaluation deferral gates

The bottom of `instantiate_mapped` decides whether to eagerly `evaluate_type` the
substituted mapped or leave it deferred. It re-interns deferred when any of these
hold:

- `preserve_meta_types`;
- `conditional_condition_needs_resolver(new_template)` — a template `Conditional`
  whose condition references a `Lazy(DefId)` or lazy `Application(Lazy, args)` the
  `NoopResolver` cannot decide (a per-key subtype check would silently fail and a
  `[keyof T]` filter would collapse to `never`);
- `template_has_lazy_application_in_composite(new_template)` — Lazy-application in a
  union/intersection template (recursive aliases like `Spec<T[P]>`);
- `name_type_has_lazy_application` — the same hazard in an `as` clause;
- `mapped_constraint_needs_resolver(new_constraint)`;
- `contains_type_parameters(new_constraint)` — still-generic `keyof __infer_0`
  during call inference; premature evaluation would resolve `keyof T` through `T`'s
  constraint and destroy the homomorphic pattern reverse-mapped inference needs.

Otherwise `self.evaluate_type(mapped_type)` finishes the job. This is the single most
important boundary between the two shards: the instantiator defers exactly the cases
that need a `TypeResolver`, and the resolver-backed evaluator picks them up later.

## Modifier arithmetic

Both shards route every modifier decision through
`tsz_solver::type_queries::mapped::compute_mapped_modifiers`:

```rust
let optional = match mapped.optional_modifier {
    Some(Add) => true, Some(Remove) => false,
    None => if is_homomorphic { source_optional } else { false },
};
let readonly = match mapped.readonly_modifier { /* same shape */ };
```

`tsc`'s `getTypeOfMappedSymbol` instantiates the template with the *read* type `T[K]`
(which includes `| undefined` for an optional source key) and only afterwards strips
the resulting top-level `undefined` via `getTypeWithFacts(type, NEUndefined)`. tsz
mirrors this exactly: the per-key loop always instantiates with the read type (so a
distributive template `V extends Validator<infer U> ? U : any` sees the `undefined`
and distributes to `any`), and `strip_removed_optional_undefined` removes the
top-level `undefined` *afterwards* when `homomorphic_removes_optional && source_optional`.
`strip_removed_optional_undefined` is a no-op under `exactOptionalPropertyTypes`,
because tsz does not yet model the missing-marker separately from an explicit
`| undefined`.

For an **index signature** the source `readonly` lives in the source object's
`string_index`/`number_index` slot, never in the named-property list.
`get_mapped_modifiers` reads it via `source_index_signature_readonly` (with the
number→string fallback `tsc` uses), so `{ [K in keyof T]: T[K] }` over
`{ readonly [k: string]: V }` keeps a readonly index signature. Index signatures are
never optional in `tsc`, so their `source_optional` is always `false`.

## Caches and invariants

| Cache / guard | Where | What it protects | Invalidation |
| --- | --- | --- | --- |
| `instantiate_type_cached` / `instantiate_type_preserving_cached` | `query_db` (`QueryDatabase`) | Memoizes template/`name_type` substitution per `(TypeId, substitution)`. | Interner-instance-local; dies with the `QueryDatabase`. |
| `TypeEvaluator::cache` (eval memo) | `evaluate.rs` | Per-evaluator `(TypeId) → TypeId`. `evaluate_mapped` results land here. | Plain `NoopResolver` evaluators may also read the *persistent* eval memo (`with_persistent_eval_memo_reads`); mode-flagged evaluators revoke it because their results are not a pure function of `(TypeId, options)`. |
| `EXTRACT_MAPPED_KEYS_VISITING` | `mapped/keys_guard.rs` | Thread-local in-flight set; re-entry on the same `TypeId` returns `None` (defer), matching "cannot extract keys." | RAII `MappedKeysVisitGuard::drop` — clears on the normal path **and on unwind** (#13368), so a caught panic mid-extraction (LSP, `try_tsz`) cannot leak a stale interner-local key into the next compilation on a reused worker thread. |
| `keyof_constraint_guard` | `evaluate.rs` field, used in `keyof_constraint.rs` | Cycle detection across a `Lazy(A) → Lazy(B) → Lazy(A)` constraint chain; all intermediate types stay entered until the chain terminates. | Per-evaluator; depth-capped by the `TypeEvaluation` profile (100). |
| `with_solver_frame` / `stacker::maybe_grow` | `keyof_constraint.rs`, `recursion` | Cross-operation stack-frame breaker (#7574); leaves the current type opaque on exhaustion. | N/A (stack, not a cache). |
| `RecursionGuard` (`guard`) | `evaluate.rs` | On `Cycle`, a `TypeData::Mapped` re-interns *itself* (`memo_insert(type_id, type_id)`) rather than collapsing to `{}`, preserving self-referential constraint structure. | Per-evaluator. |

Invariants worth stating explicitly:

- **Determinism over source order.** `evaluate_mapped` de-dups string-literal keys
  with `seen.insert(k.name)` *while preserving constraint order*, because `tsc` walks
  the constraint union in source order and the type printer's output for `T[keyof T]`
  must follow it. Homomorphic source order comes from
  `collect_homomorphic_source_property_infos` (declaration- or display-ordered).
- **`ObjectFlags::MAPPED_CONSTRAINT_KEYS`.** A *non-homomorphic* mapped type that
  produces index signatures gets this flag, because its `keyof` is the constraint key
  space, not `keyof T`. Homomorphic maps stay unflagged so `keyof` still reads `keyof T`.
- **Idempotent deferral.** Every "give up" path re-interns the *same*
  `self.interner().mapped(*mapped)`, so a deferred mapped node has a stable `TypeId`
  and the relation layer ([solver-relations](solver-relations.md)) can structurally
  compare it via `try_expand_mapped` without infinite re-evaluation.

## Edge cases and tsc parity

- **`Partial<number>` → `number`.** The primitive short-circuit (gate 5 /
  `evaluate_identity_mapped_passthrough`): an identity homomorphic map over a
  non-object reduces to the source. `Partial<any>` with no array constraint instead
  becomes `{ [x: string]: any; [x: number]: any }` (not bare `any`), and `Partial<any>`
  with an array/tuple constraint passes `any` through. `unknown`/`never`/`error`
  without an array constraint do **not** pass through.
- **`Partial<[a, b]>` is a tuple, `Partial<a[]>` is an array.** Structural identity is
  preserved via the array/tuple-preservation block, gated by `is_identity_name_mapping`
  — an `as` clause breaks shape preservation (it routes through the object path) but
  still inherits modifiers.
- **Readonly array under an `as` clause.** `evaluate_mapped_over_readonly_source`
  returns `None` (object-path fallback) for `-readonly` under an `as` clause: `tsc`
  yields a hybrid object with a writable index but *no* mutable-array methods, which
  tsz cannot represent as an array without inventing `push` or rejecting valid writes.
  This is an honest approximation, not a false positive.
- **`ReadonlyArray<T>` vs `{ readonly [k: number]: V }`.** The array shortcut requires
  array-marker methods (`slice`/`concat`) via `object_shape_has_readonly_array_markers`,
  not just a readonly numeric index — otherwise mapping a bare numeric-index object
  would drop its `readonly` modifier by reshaping it into an array.
- **`Symbol.iterator` survival.** Homomorphic maps populate `source_symbol_prop_names`
  for *every* homomorphic source (not only the `as`-clause path), so `T[Symbol.iterator]`
  resolves to the declared method type instead of falling back to the synthetic
  `__unique_<id>` atom and silently dropping the iterator.
- **`"0"` stays string-named.** A numeric-named string-literal key keeps
  `is_string_named` so `keyof` yields `"0"`, not `0` (`type_is_numeric_string_literal`).
- **Colliding `as` keys union values, keep first modifiers** — `tsc`'s
  `resolveMappedTypeMembers` (see Key remapping above).
- **`{ [P in any]: never }`.** Special-cased to a string+number index set only for the
  `never` template, to avoid changing the evaluated representation (and therefore the
  error-message display) of `{ [P in any]: V }` for non-`never` `V`. Subtype checks
  against `{ [P in any]: V }` are handled by `try_expand_mapped` in the relation layer.
- **ts-essentials `ReadonlyKeys`/`WritableKeys`.** `rewrite_single_key_self_indexed_template`
  restores homomorphic modifier inheritance for a self-indexed `{ [Q in P]: T[P] }`
  whose constraint parameter `P` is substituted by a single key, before the standard
  substitution collapses `T[P]` to `T["k"]`.

## Cross-references

- [solver-evaluation](solver-evaluation.md) — the `visit_mapped` dispatch and the
  evaluator's recursion guard / memo that this shard runs inside.
- [solver-instantiation](solver-instantiation.md) — `instantiate_key`, the
  shadowing scope, and `preserve_meta_types`, all consumed by `instantiate_mapped`.
- [solver-inference](solver-inference.md) — reverse-mapped inference, which depends on
  the `OpaqueRest` source-preservation rule.
- [solver-relations](solver-relations.md) — `try_expand_mapped` / `mapped_target.rs`:
  how a *deferred* mapped type is checked structurally without expansion.
- [solver-types-intern-def](solver-types-intern-def.md) — `MappedType`, `TupleElement`,
  `MappedModifier`, and the `TypeData::Lazy(DefId)` references the deferral gates guard.
- [checker-type-of-symbol-and-symbol-types](checker-type-of-symbol-and-symbol-types.md),
  [checker-declarations-modules](checker-declarations-modules.md) — the checker
  boundary (`evaluate_mapped_type_with_resolution`) that pre-resolves Lazy DefIds and
  delegates array/tuple preservation back to this shard via `classify_mapped_source`.
- [solver-caches-objects-contextual-compat](solver-caches-objects-contextual-compat.md)
  — `collect_properties_cached` and the object-shape caches the key-extraction walk reads.
