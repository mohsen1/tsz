# Type Inference: Candidate Collection, Contextual Inference, and Priorities

Type inference is the part of the solver that turns a generic signature plus a
list of concrete argument types into a set of bindings for the signature's type
parameters. Given `function identity<T>(x: T): T` called as `identity("hello")`,
inference is the machinery that decides `T = string`. Given the much harder
`function then<U>(cb: (v: T) => U): Promise<U>`, it is the machinery that walks a
callback's structure, defers the type parameters it cannot yet pin down, runs a
second pass once the contextual types are known, and finally widens or preserves
literals exactly the way `tsc` does. The engine lives entirely in the solver
under `crates/tsz-solver/src/inference`, and its public surface is one struct,
`InferenceContext` (re-exported from `inference/mod.rs`).

The engine is deliberately mechanical: it collects *candidates* (lower bounds),
*contra-candidates* (from contravariant positions), and *upper bounds* for each
inference variable, each candidate stamped with an `InferencePriority`, and then
runs a resolution phase that picks a winner per variable. It does not know about
call expressions, overloads, or source spans — those belong to the checker and
to the `CallEvaluator` orchestration layer. This document traces both the
collection algorithm (`infer_from_types` / `constrain_types`) and the resolution
algorithm (`resolve_with_constraints` / `fix_current_variables`), names the real
functions, and calls out the `tsc`-parity edge cases baked into each.

It is a sibling to [solver-instantiation](solver-instantiation.md) (which applies
the bindings inference produces), [solver-relations](solver-relations.md) (the
subtype kernel inference's bound validation calls into),
[solver-evaluation](solver-evaluation.md) (which owns conditional-type `infer T`
matching — a different mechanism), and [solver-operations](solver-operations.md)
(the `CallEvaluator` that drives call-site inference). On the checker side it
pairs with [checker-calls-signatures-generics](checker-calls-signatures-generics.md).

## Owns / Must not own

**The inference engine (`InferenceContext`) owns:**

- The union-find table of inference variables (`InferenceVar`) and the
  per-variable `InferenceInfo` (`candidates`, `contra_candidates`,
  `upper_bounds`, `resolved`).
- Structural candidate collection: walking a `(source, target)` type pair in
  parallel and recording bounds (`infer_from_types` in `infer_matching.rs`).
- Inference priorities (`InferencePriority`) and the priority-based filtering and
  combination rules that decide union-vs-supertype.
- Covariant/contravariant resolution, best-common-type (BCT), literal widening
  policy, fixing (`fix_current_variables`), and constraint strengthening (SCC
  unification + fixed-point propagation).
- Recursion and step guards: `MAX_INFER_DEPTH`, `MAX_APP_EXPANSION_DEPTH`,
  `MAX_CONSTRAINT_ITERATIONS`, and the visited-pair cycle breaker.

**The inference engine must not own:**

- Argument collection, overload selection, two-pass scheduling decisions, or
  diagnostics. Those belong to the `CallEvaluator` in
  `operations/generic_call` and to the checker.
- The relation kernel. Bound validation and co/contra assignability tests call
  *back out* through a closure (`is_subtype` / `is_assignable_to_strict`) or the
  engine's own simplified BCT subtype helper; the engine never re-implements the
  full relation algorithm.
- Conditional-type `infer T` extraction (`T extends [infer H, ...] ? H : ...`).
  That is owned by [solver-evaluation](solver-evaluation.md)
  (`evaluation/evaluate_rules/infer_pattern.rs` and friends). The reserved
  `infer_from_conditional` in `infer_resolve.rs` is `#[allow(dead_code)]`.
- Type construction policy. The engine asks the interner (`TypeDatabase`) to
  build unions/intersections; it does not intern raw `TypeKey`.

## Module map

| Path | Role |
| --- | --- |
| `crates/tsz-solver/src/inference/mod.rs` | Re-exports `InferenceContext`; hosts `infer_type_arguments_from_param_args`, the standalone primitive used for type-predicate instantiation. |
| `crates/tsz-solver/src/inference/infer.rs` | `InferenceContext`, `InferenceVar`, `InferenceInfo`, `InferenceCandidate`, `ConstraintSet`, `InferenceError`. Union-find table, candidate insertion (`add_candidate`, `add_contra_candidate`, `add_upper_bound`), occurs-check, query helpers. |
| `crates/tsz-solver/src/inference/infer_matching.rs` | The structural collector: `infer_from_types`, `infer_objects`, `infer_functions`, `infer_callables`, `infer_unions`, `infer_applications`, `infer_from_mapped_type`, `infer_from_template_literal`. |
| `crates/tsz-solver/src/inference/infer_matching_tuples.rs` | Variadic / spread tuple matching (`infer_tuples`, `infer_source_params_against_rest_tuple`). |
| `crates/tsz-solver/src/inference/infer_resolve.rs` | Resolution: `compute_constraint_result`, `resolve_from_candidates`, `resolve_with_constraints[_by]`, `fix_current_variables_with`, `strengthen_constraints`, `unify_circular_constraints`, `get_current_substitution`, co/contra resolution. |
| `crates/tsz-solver/src/inference/infer_bct.rs` | Best-common-type: `best_common_type`, `get_common_supertype_for_inference`, the simplified `is_subtype` used for bound validation. |
| `crates/tsz-solver/src/inference/infer_candidate_kinds.rs` | `union_object_and_array_literal_candidates` and object-literal candidate classification. |
| `crates/tsz-solver/src/inference/infer_variance.rs` | Reserved variance analysis (`compute_variance`, mostly `#[allow(dead_code)]`). |
| `crates/tsz-solver/src/inference/partially_inferable.rs` | `get_partially_inferable_type` — replaces implicit-`any` callback params with `unknown` before reverse-mapped inference. |
| `crates/tsz-solver/src/inference/template_anchor.rs` | `find_leftmost_occurrence`, `find_next_anchor_alternatives` — concrete anchors for template-literal capture. |
| `crates/tsz-solver/src/inference/template_segment_prefix.rs` | `match_template_segment_prefix` — greedy/non-greedy prefix matching of template segments. |
| `crates/tsz-solver/src/operations/constraints/walker.rs` | `constrain_types` / `constrain_types_impl` — the `CallEvaluator`-side constraint walker keyed by placeholder `TypeId` (the `var_map`). |
| `crates/tsz-solver/src/operations/generic_call/resolve.rs` | `resolve_generic_call_inner` — sets up the context, runs Round 1 / fixing / Round 2, then calls `finish_generic_call_resolution`. |
| `crates/tsz-solver/src/operations/generic_call/resolve/finalize.rs` | `finish_generic_call_resolution` — final per-parameter resolution, default/constraint fallback, post-resolution widening. |

The two structural collectors deserve a note. `infer_from_types`
(`infer_matching.rs`) keys inference variables **by type-parameter name** through
`find_type_param`; `constrain_types` (`constraints/walker.rs`) keys them **by
placeholder `TypeId`** through a `var_map`. The `CallEvaluator` uses
`constrain_types` because it has already alpha-renamed each type parameter to a
unique `__infer_N` placeholder; `infer_from_types` is the lower-level primitive
used directly when no placeholder renaming is in play (for example the
type-predicate path in `mod.rs`). Both populate the same `InferenceContext` and
share the same resolution phase.

## The data model

Every type parameter under inference is an `InferenceVar(u32)` — a key into an
`ena` `InPlaceUnificationTable<InferenceVar>`. Each root stores one
`InferenceInfo`:

```
struct InferenceInfo {
    candidates:        Vec<InferenceCandidate>,   // covariant lower bounds: source <: T
    contra_candidates: Vec<InferenceCandidate>,   // from contravariant positions (params)
    upper_bounds:      Vec<TypeId>,               // T <: U (extends + contextual bounds)
    resolved:          Option<TypeId>,            // set once the var is "fixed"
}
```

`unify_values` (the `UnifyValue` impl) merges two roots by `extend_dedup`-ing the
three vectors and taking the later `resolved`. This is how `unify_vars`
(circular-constraint SCC merging) collapses a cycle of type parameters into one
equivalence class without losing any collected evidence.

Each candidate is an `InferenceCandidate` carrying not just a `type_id` but a
bundle of provenance flags that the resolution phase reads to match `tsc`'s
widening behaviour exactly:

| Field | Meaning / why it exists |
| --- | --- |
| `priority: InferencePriority` | The site that produced the candidate; drives filtering and union-vs-supertype. |
| `is_fresh_literal` | The candidate is a literal from a fresh expression context, eligible for `getWidenedLiteralType` widening. Set false for `as const`, type annotations, and readonly sources. |
| `from_object_property` / `object_property_index` / `object_property_name` | Candidate came from an object property; index gives deterministic tie-break order. |
| `from_index_signature` | Candidate came from matching properties against `{ [k: string]: T }`; forces union semantics. |
| `source_is_type_annotation` | Candidate came from `expr as T` / a predicate type; non-fresh, never widened. |
| `from_array_element` | Candidate came from `T[]` vs an array literal; controls BCT first-wins and `NoInfer<T>` widening. |
| `from_readonly_source` | Candidate descended through a `readonly T[]` / `as const` source; literals not widened. |

The freshness computation in `add_candidate_with_context` (`infer.rs`) is the
heart of `tsc`'s `RequiresWidening` flag: a candidate is a fresh literal only
when it is a literal (or, in an array-element context, an
`array_element_union_widens_literals` type), is *not* from a non-fresh object
property, is *not* a type annotation, and is *not* in a readonly source context.

## Inference priorities

`InferencePriority` (in `crates/tsz-solver/src/types.rs`) is the same ordering
`tsc` uses, encoded as a bit-per-level so the enum's `Ord` is the priority order.
Smaller (`1 << 0`) is **higher** priority:

| Priority | Value | Produced when |
| --- | --- | --- |
| `NakedTypeVariable` | `1 << 0` | A bare type parameter appears directly (`x: T`). Highest. |
| `HomomorphicMappedType` | `1 << 1` | Inference through a structure-preserving mapped type. |
| `PartialHomomorphicMappedType` | `1 << 2` | Partially homomorphic mapped type. |
| `MappedType` | `1 << 3` | Generic mapped type `{ [K in keyof T]: U }`. |
| `ContravariantConditional` | `1 << 4` | Conditional in a contravariant position. |
| `ReturnType` | `1 << 5` | Contextual type from a return position (Round 2 callbacks). |
| `LowPriority` | `1 << 6` | Fallback inference. |
| `LiteralKeyof` | `1 << 7` | Reverse `keyof T` inference (`f("a")` synthesising `{ a: any }`). |
| `Circular` | `1 << 8` | Candidate propagated across a constraint cycle. Lowest; excluded from passes. |

`should_process_in_pass` and `next_level` drive the conceptual multi-pass walk;
`NORMAL` is `ReturnType`, `HIGHEST` is `NakedTypeVariable`, `LOWEST` is
`LowPriority`. The resolution phase uses priority in two distinct ways:

1. **Filtering.** `filter_candidates_by_priority` (`infer_resolve.rs`) keeps only
   the candidates at the single best (numerically lowest) priority, discarding
   the rest. A `NakedTypeVariable` candidate always shadows a `ReturnType` one.
2. **Combination.** A set of `PriorityImpliesCombination` priorities
   (`ReturnType`, `LowPriority`, `MappedType`, `LiteralKeyof`) tells
   `resolve_from_candidates` to *union* the candidates rather than compute a
   common supertype. This mirrors `tsc`'s `priority & PriorityImpliesCombination`
   branch in `getCovariantInference`. For `f<T>(x: T, y: T)` called `f(1, "")` the
   candidates are `NakedTypeVariable`, so the first non-superseded candidate wins
   (`T = number`) and the string gets `TS2322`; for an index-signature or
   return-type fan-out the candidates union.

A second priority rule lives in `compute_constraint_result`: a concrete
contra-candidate is discarded if its priority is strictly worse than the best
covariant priority, so a low-priority `LiteralKeyof` contra can never override a
high-priority `NakedTypeVariable` covariant candidate.

## Candidate collection: `infer_from_types`

`infer_from_types(source, target, priority)` (`infer_matching.rs`) is the
structural walker. Its contract: read inference bindings off the **target**'s
type parameters from the concrete **source**. The first lines are the guards:

```
if self.infer_depth >= Self::MAX_INFER_DEPTH { return Ok(()); }   // 20
if !self.infer_visited.insert((source, target)) { return Ok(()); } // cycle break
self.infer_depth += 1;
... infer_from_types_inner ...
self.infer_depth -= 1;
```

`MAX_INFER_DEPTH = 20` plus the `infer_visited` set break self-referential
interface recursion (the canonical witness in the code comments is
`ArrayIterator<T>` whose `[Symbol.iterator](): ArrayIterator<T>` returns itself).

`infer_from_types_inner` is a large `match (source_key, target_key)`. The order
of cases matters because earlier cases short-circuit:

1. **`NoInfer<T>` block.** If the target is `TypeData::NoInfer(_)`, return
   immediately — inference does not descend (TypeScript 5.4 `NoInfer`). A
   `NoInfer` source is unwrapped.
2. **Case 1 — target is an inference variable** (`TypeData::TypeParameter` whose
   name resolves via `find_type_param`): `add_candidate(var, source, priority)`.
   This is the lower-bound base case `source <: T`.
3. **Case 2 — source is an inference variable**: this is the contravariance hook.
   If `in_contra_mode` (we are inside a function-parameter walk), add `target` as
   a **contra-candidate**; otherwise add it as an **upper bound** `T <: target`.
4. **`Lazy(DefId)` resolution.** Both source and target `Lazy` references are
   resolved (`resolve_lazy_for_inference`) before structural dispatch, because a
   `Lazy` is opaque and cannot be matched shape-to-shape.
5. **Structural recursion** for matching shapes: objects (`infer_objects`),
   functions (`infer_functions`), callables (`infer_callables`), arrays, tuples
   (`infer_tuples`), unions (`infer_unions`), intersections, applications
   (`infer_applications`), index access, keyof, mapped, template literals.
6. **Decomposition fallbacks** when only one side is a union/intersection. A
   non-union source against a union target partitions the target's parameterized
   arms into *naked* (bare `TypeParameter`) and *structured*, and prefers
   structured arms that share outer structure with the source
   (`types_share_outer_structure`). This is the Promise `.then` case: a source
   `Promise<any>` against `T | PromiseLike<T>` infers `T = any` from the
   thenable arm, not `T = Promise<any>` from the naked arm.
7. **Application expansion.** A `TypeData::Application` source or target that did
   not match directly is expanded via `try_expand_application`, guarded by
   `app_expansion_depth < MAX_APP_EXPANSION_DEPTH` (5) so a recursive alias like
   `type Spec<T> = { [P in keyof T]: Spec<T[P]> }` cannot loop forever.

### Functions and contravariance

`infer_functions` (`infer_matching.rs`) is where variance is enforced. It sets
`self.in_contra_mode = true`, then for each parameter pair calls
`infer_from_types(target_param.type_id, source_param.type_id, priority)` — **note
the swap**: target becomes source. With both `in_contra_mode` and the swap, a
type parameter sitting in a parameter position lands in case 2's
`add_contra_candidate` branch. Return types are inferred un-swapped (covariant).
Rest parameters are special: a target rest that is a bare type parameter
(`(...args: A) => R` with `A extends any[]`) collects the remaining source params
into a **tuple** (`A = [number, number]`), and a tuple-typed rest routes through
`infer_source_params_against_rest_tuple` so variadic arity (`bind`-style
`[...T, ...U]`) is preserved.

Contra-candidates are not the same as upper bounds. When only contra-candidates
exist, resolution uses **intersection** (`resolve_from_contra_candidates`),
matching `tsc`'s `getCommonSubtype`/`getIntersectionType`. The reason is in the
case-2 comment: decomposing `{kind: T}` against `{kind:'a'} | {kind:'b'}` must
not produce two hard upper bounds `'a'` and `'b'` (which would force `T` to be
both and emit a false `TS2345`); instead it produces contra-candidates resolved
to `'a' & 'b'` = `never` or, more usefully, lets a covariant candidate win.

### Objects, mapped types, and reverse-mapped inference

`infer_objects` matches properties by name. The mapped-type cases are the most
intricate. When the target is `{ [K in keyof T]: Template<T[K]> }` and the source
is a concrete object, `infer_from_mapped_type` walks each source property,
substitutes the property name for `K`, and recurses. The reverse case — target
`T[K]` where `T` is an inference variable and `K` is a concrete literal — is
captured by the `IndexAccess` arm, which pushes `(key_atom, source)` into
`reverse_mapped_properties[var]`. After the mapped loop those pairs are flushed
into a single object candidate for `T`. `get_partially_inferable_type`
(`partially_inferable.rs`) runs first to rewrite implicit-`any` callback
parameters to `unknown`, so a method-shorthand source like `{ contains(k){...} }`
infers `T[K] = unknown` instead of leaking `any`.

### Template literals

`infer_from_template_literal` handles `` `prefix${infer T}suffix` `` style
targets. A `string` or `any` source assigns that type to every infer variable in
the pattern. A literal string source is matched against the spans with
`match_template_pattern`, and each captured substring becomes a
`literal_string` candidate. Non-greedy captures use `template_anchor.rs`'s
`find_next_anchor_alternatives` / `find_leftmost_occurrence` to find the next
concrete separator, and `template_segment_prefix.rs` matches fixed prefixes.

## The `CallEvaluator` constraint walker: `constrain_types`

`constrain_types` (`operations/constraints/walker.rs`) is the parallel collector
the call-resolution orchestration uses. It takes a `var_map: FxHashMap<TypeId,
InferenceVar>` mapping each unique placeholder type to its variable. Its base
cases mirror `infer_from_types`:

```
if let Some(&var) = var_map.get(&target) { ctx.add_candidate(var, source, priority); return; }
if let Some(&var) = var_map.get(&source) {
    if ctx.in_contra_mode { ctx.add_contra_candidate(var, target, priority); }
    else { ctx.add_upper_bound(var, target); }
    return;
}
```

It has its own guards distinct from the engine's depth guard:
`MAX_CONSTRAINT_STEPS = 20_000` (a global step budget tracked in
`constraint_step_count`), `MAX_CONSTRAINT_RECURSION_DEPTH = 100`, and a
`constraint_pairs` visited set. When `source == any`,
`propagate_type_to_placeholders` flows `any` only to naked placeholders and
union/intersection members, never into structural shapes — this is `tsc`'s
`propagationType` rule and is why passing `any` to `f<T extends X>(v: T)` infers
`T = any`, not `T = X`.

## Resolution: from candidates to a binding

After all candidates are collected, each variable is resolved.
`compute_constraint_result` (`infer_resolve.rs`) is the shared engine. Its
pipeline:

```
                   ┌──────────────────────────────────────────────┐
 candidates ─────► │ discard self-referential (occurs_in)         │
 contra_cands ───► │ filter unknown/error/any vs informative      │
 upper_bounds ───► │   upper bounds (keep `any` only if it's the  │
                   │   only meaningful candidate)                 │
                   │ expand cyclic upper bounds                   │
                   │ drop low-priority contras below best covar.  │
                   └──────────────────────────────────────────────┘
                                       │
        ┌──────────────────────────────┼──────────────────────────────┐
        ▼                              ▼                               ▼
  covariant cands              only contra cands               only upper bounds
  resolve_from_candidates      resolve_from_contra             single bound, or
        │                      (intersection)                  intersection of bounds
        ▼                              │                               │
  if contra cands too:                 │                               │
  resolve_covariant_against_contra ◄───┘                               │
        │                                                              ▼
        ▼                                              (no candidates, no bounds)
  validate upper bounds (first_failed_upper_bound)            → UNKNOWN
  validate self-referential bounds
  occurs-check → store resolved
```

### `resolve_from_candidates`: union vs supertype, widening

This function (the BCT entry point) decides the covariant result:

1. `filter_candidates_by_priority` keeps only the best-priority candidates.
2. `never` candidates are dropped; if all were `never`, the result is `never`.
3. `preserve_literals` is computed: true if the variable is `const`, if the
   declared `extends` constraint implies literals (`T extends "a" | "b"`), is a
   primitive (`T extends string`), or contains a primitive-constrained type
   parameter. Critically it reads the **declared** constraint, not `upper_bounds`
   — a contextual `Box<boolean>` upper bound must not preserve `false`.
4. If the best priority is in `PriorityImpliesCombination`, or all candidates are
   index-signature derived, the result is a **subtype-reduced union** — except
   when every candidate is a non-fresh literal, where `union_from_slice` keeps the
   precise `1 | 2` (the `as const` / issue #9714 case). Otherwise
   `best_common_type`.
5. Otherwise (`NakedTypeVariable` etc.), candidates are widened *before* the
   tournament (`getWidenedLiteralType` semantics), object/array literal
   candidates are unioned (`union_object_and_array_literal_candidates`), and
   `get_common_supertype_for_inference` runs the tournament. `should_widen` is
   gated by `!preserve_literals && !is_const && !has_non_fresh &&
   !skip_literal_widening`.

`best_common_type` (`infer_bct.rs`) itself: a homogeneous fast path, dedup +
`any`-dominates, then a `find_common_base_type` pass (`string | "hello"` →
`string`), then an O(N) tournament reduction (`best` advances whenever
`is_subtype(best, candidate)`) verified by `is_suitable_common_type`, then a
common-base-class search (`[Dog, Cat]` → `Animal`), and finally a union.
`get_common_supertype_for_inference` is the stricter inference variant that
strips nullable types, unions same-base literals, runs the tournament, and adds
nullables back — with `array_element_first_wins` forcing leftmost-wins when a
candidate came from an array element (issue #9667).

### Co/contra arbitration

When both covariant and contra-candidates survive,
`resolve_covariant_against_contra` decides. It widens the covariant result
(`widen_type_for_inference`, so `{a:1,b:2}` becomes `{a:number,b:number}` before
the excess-property-sensitive subtype test), then keeps the covariant result iff
it is informative *and* assignable to some contra-candidate; otherwise it falls
back to the contra intersection. This is `tsc`'s `getInferredType` rule and is
why `create(f, { value: "C" })` infers `P` from the function parameter (contra
`Props`) rather than the object literal when the literal is not assignable to
`Props`. The bound-validation closure here is the **checker's** real
`is_assignable_to`, passed in as `external_is_subtype` so `Lazy` interface/class
types can be compared through their extends chains — the engine's own
`is_subtype` cannot.

## Two rounds and fixing

For a generic call with a context-sensitive argument (a lambda whose parameter
types depend on what is being inferred), the `CallEvaluator`
(`generic_call/resolve.rs`) runs the classic `tsc` two-round protocol:

```
Round 1 ── constrain_types over non-deferred args  ──► candidates collected
   │
   ▼
fix_current_variables_with(checker.is_assignable_to)  ──► resolved set on vars
   │      (only vars with candidates; never-only vars left unfixed)
   ▼
build fixed_subst from probe()d vars
   │
   ▼
Round 2 ── instantiate deferred-arg params with fixed_subst,
           constrain_types over the now-contextually-typed lambdas
   │
   ▼
finish_generic_call_resolution  ──► per-param resolve + default/constraint fallback
```

`fix_current_variables_with` (`infer_resolve.rs`) is the round boundary. For each
variable with candidates it computes the current best type (the same
covariant/contra logic as `compute_constraint_result`) and stamps `resolved`,
which prevents Round 2's lower-priority `ReturnType` candidates from overriding
it. Two `tsc`-parity escape hatches: a variable whose only candidate is `never`
is **not** fixed (Round 2 might supply something better), and a variable whose
candidates would `occurs_in` itself is left for final resolution.

`get_current_substitution` then maps each placeholder atom to its
resolved-or-best-candidate type, and the orchestration instantiates the deferred
parameters with `fixed_subst` so the lambda `(a) => ...` sees concrete parameter
types in Round 2.

### The top-level-in-return-type widening gate

A subtle `tsc` rule is encoded in `top_level_in_return_type_unfixed` (a set of
roots). When a type parameter appears at the top level of the return type and has
not been fixed, fresh literal candidates are **not** widened during covariant
resolution (`skip_literal_widening` in `resolve_from_candidates`). This preserves
`U = 1` across the Round 1 → Round 2 boundary so a deferred callback's contextual
type for `(a: T) => U` is `(a: number) => 1`, matching `tsc`, rather than
`(a: number) => number`. The orchestration sets this with
`mark_top_level_in_return_type_unfixed` driven by
`type_param_preserves_inferred_literal`.

## Constraint strengthening and circular constraints

Before final resolution, `strengthen_constraints` (`infer_resolve.rs`) runs:

1. `unify_circular_constraints` builds a directed graph of `extends` edges
   between type parameters (`T extends U` adds `T → U`), runs **Tarjan's SCC**
   algorithm (the inline `strongconnect` / `TarjanState`), and `unify_vars`-merges
   every SCC with more than one member into a single inference variable. So
   `T extends U, U extends T` becomes one equivalence class.
2. A fixed-point loop (bounded by `MAX_CONSTRAINT_ITERATIONS = 100`) propagates
   candidates **up** the extends chain via `propagate_candidates_to_upper`: if
   `T <: U` and `T` has a candidate `C`, then `C` becomes a candidate of `U` at
   `Circular` priority (the lowest, so it never shadows a real inference).

Cyclic upper bounds that reference the variable's own parameter family
(`T extends I2<T>`) are detected by `upper_bound_cycles_param` and expanded by
`expand_cyclic_upper_bound` rather than treated as hard bounds; truly
self-referential bounds (`occurs_in` the root) are deferred and re-checked after
a value is known, by instantiating the constraint with the resolved value and
testing `is_subtype` (`resolve_with_constraints`, the `self_referential_bounds`
path). This mirrors `tsc`'s `nonFixingMapper` constraint re-check.

## Default type arguments and fallback

When a variable has no usable candidates, `finish_generic_call_resolution`
(`finalize.rs`) and the `compute_contextual_types` path in `normalization.rs`
apply a strict fallback order. The canonical order, from `normalization.rs`
Pass 2 and `finalize.rs`:

```
inferred candidate  >  single concrete upper bound  >  default  >  constraint  >  UNKNOWN
```

- **Default before constraint.** `<T = TypegenDisabled>` with constraint
  `TypegenEnabled | TypegenDisabled` resolves to the *default* when no inference
  happened, because the default is "what the type IS when no argument is
  provided", while the constraint is only an upper bound. The default is
  instantiated with the already-resolved parameters (`eval_type_param_default`)
  and only used if it no longer contains type parameters.
- **`defaulted_placeholders`.** Type parameters carrying a default are tracked so
  the union-inference machinery in `constrain_types` does not over-widen them.
- **`default_fallback_tp_names`.** When a parameter falls back to its default,
  its name is recorded so the post-resolution argument check does *not* validate
  arguments against the default — a default is a fallback, not a constraint.
- **Last resort.** If neither default nor constraint resolves to a concrete type,
  the inference placeholder `__infer_N` itself is reused so callbacks get a unique
  placeholder type rather than the callee's raw type parameter (avoiding name
  collisions with outer-scope parameters).

An unconstrained type parameter with no default and no inference resolves to
`UNKNOWN` (or `unknown`), matching `tsc`.

## Caches and invariants

The inference engine is intentionally light on persistent caches — most state is
operation-local and dropped with the `InferenceContext`. The caches and the
invariants that protect correctness:

| Cache / guard | Scope | Invalidation |
| --- | --- | --- |
| `subtype_cache: RefCell<FxHashMap<(TypeId,TypeId), bool>>` | One inference request | Dropped with the context. Memoizes the simplified BCT/bounds `is_subtype`. |
| `active_subtype_checks: RefCell<FxHashSet<(TypeId,TypeId)>>` | One inference request | Coinductive cycle break: a pair already in flight is assumed compatible. |
| `infer_visited: FxHashSet<(TypeId,TypeId)>` | One `infer_from_types` walk | Prevents re-visiting a `(source,target)` pair, breaking self-referential recursion. |
| `infer_depth` (≤ `MAX_INFER_DEPTH` = 20) | One walk | Hard depth cap independent of the visited set. |
| `app_expansion_depth` (≤ `MAX_APP_EXPANSION_DEPTH` = 5) | One walk | Bounds recursive-alias `Application` expansion. |
| `constraint_step_count` (≤ `MAX_CONSTRAINT_STEPS` = 20_000) | One call resolution | Global budget on `constrain_types` steps. |
| `constraint_recursion_depth` (≤ 100) | One call resolution | Stack-depth cap for `constrain_types`. |
| `constraint_pairs` visited set | One call resolution | De-dups `(source,target)` constraint pairs. |
| `reverse_mapped_properties` | One walk | Accumulates `(key, value)` pairs flushed into one object candidate per mapped-type loop. |
| `vars_with_substituted_candidates` | One resolution | Marks vars whose candidates were rewritten after higher-order placeholder substitution, so stale placeholders are dropped. |

Key invariants:

- **Root-keyed metadata.** `declared_constraints`, `literal_preserving_*`,
  `top_level_in_return_type_unfixed`, and `implied_arities` are all keyed by the
  **union-find root** (`table.find(var)`), set via helpers that normalize first
  (`set_declared_constraint`, `set_implied_arity`). This survives later
  `unify_vars` merges; a constraint set on a child variable would otherwise be
  lost when the child unifies into another root.
- **Occurs-check before store.** Every resolution path runs `occurs_in(root,
  result)` and refuses to store a result that references the variable's own
  parameter — preventing infinite types like `T = Array<T>`.
- **`any` is uninformative but not silent.** `any` candidates are dropped only
  when an informative upper bound *and* a concrete candidate both exist; passing
  `any` to a constrained parameter still infers `T = any`. In contra resolution
  `any` is filtered out when non-`any` candidates exist.
- **The printer is never read.** Resolution decisions read `TypeData` and the
  candidate flags, never rendered type strings, per the repo's anti-hardcoding
  rule.

## Edge cases and `tsc` parity

- **First-wins vs union (`f<T>(x: T, y: T)`).** `f(1, "")` infers `T = number`
  (first non-superseded candidate) because the candidates are `NakedTypeVariable`,
  so `priority_implies_combination` is false and the tournament picks a single
  winner; the `string` argument then gets `TS2322`. Compare `makeRecord` over a
  mapped type, where `MappedType` priority unions to `Box<number> | Box<string>`.

- **Literal preservation.** `<T extends string>(a: T): T` called `f("z")` keeps
  `T = "z"` because the declared constraint is primitive
  (`declared_constraint_is_primitive`). `<T>(x: T): T` keeps `"z"` only via the
  top-level-return widening gate (`type_param_preserves_inferred_literal`). A
  literal inferred from a nested position (callback return, array element) is
  widened to its primitive.

- **`as const` / readonly sources.** `new Set([1, 2] as const)` infers
  `Set<1 | 2>` because `in_readonly_source_context` marks the element candidates
  non-fresh, suppressing widening. The same flag flows through
  `from_readonly_source`, consulted in `resolve_covariant_against_contra` so a
  direct readonly argument is not replaced by a mutable callback parameter
  candidate.

- **`NoInfer<T>`.** A `NoInfer` target halts descent in `infer_from_types`, so no
  candidate is collected from that position. Array-element fresh literals in
  `NoInfer<T>` positions are then widened in `finalize.rs` to match `tsc`'s BCT.

- **Promise `.then` thenable arm.** Inferring `Promise<any>` against the
  callback-return target `T | PromiseLike<T>` prefers the structured
  `PromiseLike<T>` arm (`types_share_outer_structure`), giving `T = any` instead
  of `T = Promise<any>`.

- **Distributive `Extract`-style targets.** A target conditional `T extends U ? T
  : Y` whose `check_type == true_type` is a naked inference parameter infers the
  source directly against that parameter (the `Conditional` arm in
  `infer_from_types_inner`), so `Extract<K, U>` parameters surface `never` rather
  than the raw `keyof T` constraint.

- **Variadic tuple rest (`bind`).** A target rest `[...T, ...U]` distributes a
  source tuple between adjacent variadic elements using `implied_arities`
  (`set_implied_arity` / `implied_arity_for_type`) and
  `constraint_fixed_arity_for_type`, mirroring `tsc`'s `impliedArity`.

- **Higher-order generic arguments (TS 3.4).** Passing `list<T>` into another
  generic creates `__infer_src_*` source variables; `finish_generic_call_resolution`
  resolves them first and `substitute_source_vars_in_targets` rewrites the outer
  candidates so resolution sees `T[]` instead of an opaque `__infer_src_3`.

- **Bound-violation fallback split.** When a `BoundsViolation` arises from a
  Round 2 callback-return inference (`all_candidates_are_return_type` and
  `saw_deferred_arg`), `finalize.rs` keeps the inferred lower bound and lets the
  return expression report `TS2322`, rather than falling back to the constraint
  and reporting `TS2345` on the whole callback — matching `tsc`'s diagnostic
  placement.

## A worked example

Trace `function map<T, U>(arr: T[], f: (x: T) => U): U[]` called as
`map([1, 2, 3], x => x.toString())`:

1. The `CallEvaluator` (`resolve_generic_call_inner`) allocates `InferenceVar`s
   for `T` and `U`, mints unique placeholders `__infer_0` (T) and `__infer_1`
   (U), and seeds `var_map`. Neither parameter has a constraint, so no upper
   bound is added.
2. **Round 1.** `arr` is not deferred. `constrain_types(arr_source = number[],
   target = __infer_0[])` recurses into the array element:
   `add_candidate(T_var, number, NakedTypeVariable)`. The lambda `x =>
   x.toString()` *is* context-sensitive (`is_contextually_sensitive`), so it is
   deferred — no candidate for `U` yet.
3. **Fixing.** `fix_current_variables_with` resolves `T`: one fresh literal-free
   candidate `number`, `NakedTypeVariable` priority, no contra-candidates →
   `resolve_from_candidates` returns `number`. `T` is stamped `resolved =
   number`. `U` has no candidates and is left unfixed.
4. **fixed_subst.** `{ __infer_0 → number, T → number }`.
5. **Round 2.** The deferred parameter `(x: __infer_0) => __infer_1` is
   instantiated with `fixed_subst` to `(x: number) => __infer_1`. The lambda is
   re-checked with `x: number`, its body `x.toString()` yields `string`, and
   `constrain_return_context_structure` adds `add_candidate(U_var, string,
   ReturnType)`.
6. **Finalize.** `finish_generic_call_resolution` resolves `U`: candidate
   `string` at `ReturnType` priority → `priority_implies_combination` is true, the
   single candidate resolves to `string`. Bindings `{ T = number, U = string }`,
   call result type `string[]`.

If instead `T` had been ambiguous (`map([1, "a"], ...)`), the two array-element
candidates `number` and `string` would resolve via `get_common_supertype_for_inference`
to `number | string` (no common supertype, union fallback), and `U` would still
flow from the Round 2 return type.

## Cross-references

- The orchestration that *requests* inference and turns the result into a
  `CallResult` or diagnostic:
  [checker-calls-signatures-generics](checker-calls-signatures-generics.md) and
  [solver-operations](solver-operations.md).
- Applying the bindings inference produces (substitution, instantiation):
  [solver-instantiation](solver-instantiation.md).
- The relation kernel the bound-validation closures call into:
  [solver-relations](solver-relations.md) and
  [checker-assignability-gateway](checker-assignability-gateway.md).
- Conditional-type `infer T` extraction (a separate mechanism in evaluation):
  [solver-evaluation](solver-evaluation.md).
- The identity handles (`TypeId`, `Atom`, `DefId`) and the interner inference
  builds types through: [solver-types-intern-def](solver-types-intern-def.md) and
  [solver-caches-objects-contextual-compat](solver-caches-objects-contextual-compat.md).
- Where it all sits in the run:
  [end-to-end-timeline](end-to-end-timeline.md).
