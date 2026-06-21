# Evaluation of Conditional, Mapped, Template, Infer, keyof, and Index Types

The evaluation engine is the solver's *type-level interpreter*. Where the rest
of the solver answers relational questions ("is `A` assignable to `B`?"),
evaluation answers a reductional one: "what concrete type does this meta-type
compute to?". TypeScript's conditional (`T extends U ? X : Y`), mapped
(`{ [K in C]: V }`), template-literal (`` `a${T}b` ``), `infer`, `keyof`, and
indexed-access (`T[K]`) constructs are type-level functions; the engine walks a
`TypeId`, recognizes which meta-type it is, and rewrites it toward a structural
fixed point — distributing conditionals over unions, expanding mapped keys,
extracting `infer` bindings, intersecting `keyof` key spaces, and indexing into
shapes — while a thick stack of recursion, fuel, and budget guards keeps
pathologically recursive utility libraries (ts-toolbelt, ts-essentials,
type-fest, zod) from hanging the compile.

Everything lives under `crates/tsz-solver/src/evaluation`. The single owning
struct is `TypeEvaluator` (in `evaluation/evaluate.rs`); the per-category rewrite
rules live in `evaluation/evaluate_rules/*`; the orchestration helpers (visitor
dispatch, argument expansion, union/intersection simplification, the persistent
caches) live in `evaluation/evaluate/*`. This document traces a `TypeId` from the
public entry point through the visitor dispatch into each rule, names the real
functions that run, and pins the caches, fuel guards, and `tsc`-parity edge
cases that the implementation encodes. It is the middle-tier companion to
[solver-relations](solver-relations.md), [solver-inference](solver-inference.md),
[solver-instantiation](solver-instantiation.md), and
[solver-types-intern-def](solver-types-intern-def.md).

---

## Owns / Must not own

**The evaluation engine owns:**

- Recognizing meta-type `TypeData` variants and rewriting them: `Conditional`,
  `Mapped`, `IndexAccess`, `KeyOf`, `TemplateLiteral`, `StringIntrinsic`,
  `Application`, `TypeQuery`, `Lazy`, plus structural recursion into `Union`,
  `Intersection`, `Array`, and `Tuple` so nested meta-types reduce.
- Conditional-type distribution over unions, tail-call elimination for
  tail-recursive conditional aliases, and the deferral rules that keep a
  conditional opaque while its check type is still generic.
- `infer` extraction (`match_infer_pattern`) and substitution
  (`substitute_infer`), including constraint filtering of inferred bindings.
- Mapped-type homomorphism detection, modifier add/remove (`+?`/`-?`,
  `+readonly`/`-readonly`), key remapping via `as` clauses, and array/tuple
  shape preservation.
- `keyof` over objects, intersections (`keyof (A & B) = keyof A | keyof B`),
  unions (`keyof (A | B) = keyof A & keyof B`), index signatures, tuples,
  primitives, and deferred forms.
- The fuel, depth, iteration, divergence, and cross-instance budgets that bound
  evaluation, and the family of caches that make repeated reductions O(1).

**It must not own** (these belong to sibling solver modules):

- The structural subtype kernel. Conditional evaluation *asks*
  `SubtypeChecker::is_subtype_of` whether the check type extends the extends type
  (see [solver-relations](solver-relations.md)); it does not reimplement the
  relation.
- Generic substitution. Instantiating a body with arguments routes through
  `instantiate_generic_cached` / `instantiate_type` (see
  [solver-instantiation](solver-instantiation.md)); the evaluator calls it but
  does not own the substitution walk.
- Type construction and interning. New `TypeId`s come from the
  `TypeDatabase`/`TypeInterner` constructors (`union`, `intersection`,
  `conditional`, `mapped`, `keyof`, `index_access`, `literal_string`); see
  [solver-types-intern-def](solver-types-intern-def.md).
- Property collection. Mapped/keyof evaluation calls
  `collect_properties_cached` and `collect_homomorphic_source_property_infos`
  rather than re-deriving member sets (see
  [solver-caches-objects-contextual-compat](solver-caches-objects-contextual-compat.md)).
- Diagnostics. Evaluation never emits a diagnostic; it returns `TypeId::ERROR`,
  or sets flags (`mark_union_too_complex`, `mark_exceeded`) the checker reads to
  produce TS2589/TS2590. The checker owns source locations and the message; the
  assignability gateway owns relation-failure reasons (see
  [checker-assignability-gateway](checker-assignability-gateway.md)).

---

## Module map

| Path | Role |
| --- | --- |
| `evaluation/evaluate.rs` | The `TypeEvaluator` struct, the `evaluate`/`evaluate_guarded`/`evaluate_guarded_inner` driver loop, the recursion/fuel/epoch guards, and `memo_insert`. |
| `evaluation/evaluate/api.rs` | Public convenience wrappers: `evaluate_type`, `evaluate_conditional`, `evaluate_index_access`, `evaluate_mapped`, `evaluate_keyof`, `evaluate_type_with_resolver`. |
| `evaluation/evaluate/support.rs` | `visit_type_key` visitor dispatch, type-arg expansion (`expand_type_args`, `try_expand_type_arg`), union/intersection simplification (`remove_redundant_members`), tuple/array element evaluation. |
| `evaluation/evaluate/application.rs` | `evaluate_application`: callee normalization, per-`DefId` depth, the `application_eval_cache`. |
| `evaluation/evaluate/closed_eval.rs` | The substitution-independent persistent `closed_eval_cache` (read/write gates). |
| `evaluation/evaluate/query_budget.rs` | The cross-instance per-query `evaluate` operation budget (`EvalQueryFrame`). |
| `evaluation/cross_eval_guard.rs` | Thread-local cross-evaluator cycle breaker and per-query result memo. |
| `evaluation/recursive_growth.rs` | Divergent-recursion (TS2589) detection by per-step argument-weight growth. |
| `evaluation/request.rs` / `result.rs` / `session.rs` | Typed request/result wrappers and the cross-context `EvaluationSession` (global instantiation depth/fuel). |
| `evaluate_rules/conditional.rs` (+ `conditional/*`) | `evaluate_conditional`, distribution, tail-call elimination, branch deferral, `infer` dispatch. |
| `evaluate_rules/mapped.rs` (+ `mapped/*`) | `evaluate_mapped`, homomorphism, modifiers, key remapping, array/tuple preservation. |
| `evaluate_rules/keyof.rs` | `evaluate_keyof`. |
| `evaluate_rules/index_access.rs` (+ `index_access_*`) | `evaluate_index_access` and the `IndexAccessVisitor`. |
| `evaluate_rules/template_literal.rs` | `evaluate_template_literal` (Cartesian product expansion). |
| `evaluate_rules/string_intrinsic.rs` | `Uppercase`/`Lowercase`/`Capitalize`/`Uncapitalize`. |
| `evaluate_rules/infer_pattern.rs` (+ `infer_pattern_*`) | `match_infer_pattern`, `substitute_infer`, constraint filtering. |
| `evaluate_rules/substitute.rs` | `substitute_exact_type` — the exact-`TypeId` rewrite used by distribution. |

The recursion/fuel constants (`MAX_DEF_DEPTH`, `MAX_TAIL_RECURSION_DEPTH`,
`MAX_GLOBAL_EVAL_DEPTH`, `MAX_EVALUATION_FUEL`, `EVAL_FUEL_CHECK_INTERVAL`,
`DEFAULT_MAX_EVAL_OPS_PER_QUERY`) are centralized in `crate::limits`
(`crates/tsz-solver/src/limits/mod.rs`), which also documents how each maps onto
a `tsc` counter.

---

## The driver loop

The public surface is in `evaluate/api.rs`. `evaluate_type(interner, type_id)`
constructs a fresh `TypeEvaluator` and calls `evaluate`; the request-shaped
`evaluate_type_with_request` threads the option-sensitive
`no_unchecked_indexed_access`/`exact_optional_property_types` flags (carried by
`EvaluationRequest` in `evaluation/request.rs`) before evaluating. Every public
wrapper builds an evaluator and immediately drops it after one root call.

`TypeEvaluator::evaluate` (in `evaluate.rs`) is the hot front door. It is laid
out so the common case — an already-reduced or intrinsic type — pays almost
nothing:

```
evaluate(type_id):
  1. type_id.is_intrinsic()                  -> return type_id          (no guard)
  2. self.cache.get(type_id)                 -> return cached           (local memo)
  3. interner.structurally_eval_inert_cached -> return type_id          (shared fixed point)
  4. try_closed_eval_read(type_id)           -> return cached           (persistent, subst-independent)
  5. persistent_memo_reads + lookup_eval_memo-> return cached           (cross-evaluator, plain ctx)
  6. guard.is_exceeded()                      -> return ERROR
  7. enter_eval_query_budget()                -> None => return type_id  (cross-instance op budget)
  8. depth >= 10 ? global_eval_depth_enter()  -> silent bail if over MAX_GLOBAL_EVAL_DEPTH
  9. evaluate_guarded(type_id)                -> the real work
 10. commit_closed_eval_writes(...)           (top-level frame only)
```

Step 3 is the resolver-independent *structural fixed point* (issues #13250 /
#8356): a type holding none of the rewritable kinds and no
substitution-dependent leaf evaluates to itself under every evaluator and every
resolver, so once any evaluator records `is_structurally_eval_inert`, all later
evaluators — including the resolver-backed ones that drop their local cache on
each relation call — short-circuit the entire walk with one shared bit lookup.

`evaluate_guarded` wraps the work in `crate::recursion::with_solver_frame`, which
pairs a `stacker::maybe_grow` (so deep conditional/mapped chains grow the OS
stack instead of crashing) with the cross-operation
`MAX_SOLVER_STACK_FRAMES` (= 2000) breaker that bounds the interleaved
`evaluate -> subtype -> instantiate -> evaluate` cycle whose frames slip past
every per-instance guard (issue #7574). On exhaustion it calls
`mark_silent_depth_bailed` and returns `type_id` unchanged.

`evaluate_guarded_inner` runs the real dispatch. It enters the per-instance
`RecursionGuard<TypeId>` (`guard.enter`), which returns one of four
`RecursionResult` variants (`evaluation`/`recursion.rs`):

| `RecursionResult` | Meaning | Evaluator response |
| --- | --- | --- |
| `Entered` | within depth and iteration limits | proceed to `visit_type_key` |
| `Cycle` | this `TypeId` is already on the stack | `mark_deep_recursion_seen`; keep `Mapped` deferred (return `type_id`); else return `type_id` (or `ERROR` under the TS2589 app-cycle detection pass) |
| `DepthExceeded` | depth >= `max_depth` | escalate to `ERROR` if `has_real_instantiation_depth()`, else `clear_exceeded` + `mark_silent_depth_bailed` and leave `type_id` opaque |
| `IterationExceeded` | iterations > `max_iterations` | `mark_deep_recursion_seen`; leave `type_id` opaque |

The guard uses `RecursionProfile::TypeEvaluation` (`max_depth = 100`,
`max_iterations = 100_000`). After the guard, every
`EVAL_FUEL_CHECK_INTERVAL` (= 128) iterations the loop samples the per-file fuel
via `interner.consume_evaluation_fuel`; exhausting `MAX_EVALUATION_FUEL`
(= 2_000_000, tsc's `instantiationCount` analog) marks the guard exceeded and
returns `ERROR`. Then it dispatches through `visit_type_key` and memoizes the
result with `memo_insert`.

### Visitor dispatch

`visit_type_key` (in `evaluate/support.rs`) is the central `match` over
`TypeData`:

```
Conditional(id)   -> visit_conditional   -> evaluate_conditional
IndexAccess(o,i)  -> visit_index_access   -> evaluate_index_access
Mapped(id)        -> visit_mapped         -> evaluate_mapped
KeyOf(operand)    -> visit_keyof          -> evaluate_keyof (+ display-alias for named operands)
TypeQuery(sym)    -> visit_type_query      (resolve_type_query -> constructor type for classes)
Application(id)   -> visit_application     -> evaluate_application
TemplateLiteral   -> visit_template_literal-> evaluate_template_literal
Lazy(def_id)      -> visit_lazy            (resolve_lazy, this-binding, default-arg instantiation)
StringIntrinsic   -> visit_string_intrinsic-> evaluate_string_intrinsic
Intersection(l)   -> visit_intersection   -> evaluate_intersection
Union(l)          -> visit_union          -> evaluate_union
Array(e)          -> visit_array           (only meta-type elements re-evaluated)
Tuple(l)          -> visit_tuple           (flatten concrete spreads, distribute tuple unions)
NoInfer(inner)    -> evaluate(inner)       (strip outermost wrapper)
_                 -> type_id               (pass through unchanged)
```

`visit_union` / `visit_intersection` recursively evaluate each member
(`evaluate_compound_member`, which preserves an outer `NoInfer<>` wrapper), then
re-intern and run *subtype-based simplification*. The simplification core is
`remove_redundant_members` (in `support.rs`), an O(n²) pass over up to
`MAX_SIMPLIFICATION_SIZE` (= 25) members that runs a `SubtypeChecker` with
`bypass_evaluation = true` and drops a member structurally subsumed by another.
It carries a dense thicket of `tsc`-parity exceptions: bare primitive keywords
are kept (`union_member_removable_as_subtype` mirrors tsc's `removeSubtypes`
`hasEmptyObject || StructuredOrInstantiable` gate), branded-primitive idioms
(`string & {}`) survive (`is_branded_primitive_pair`), members carrying a unique
index signature survive (`has_index_signature_not_in`), and intersection
modifier AND-merging is preserved (`intersection_drop_changes_modifiers`). The
`is_complex_type` guard keeps any member containing an unresolved
`TypeParameter`/`Conditional`/`Mapped`/`IndexAccess`/`KeyOf` out of
simplification entirely, because `bypass_evaluation` cannot judge it soundly.

---

## Conditional types

`evaluate_conditional` (in `evaluate_rules/conditional.rs`) is the largest single
rule — a `loop` that performs **tail-call elimination**: when a chosen branch
evaluates to another `Conditional` (or an `Application` whose body is a
conditional), the loop re-enters with the new operands instead of recursing,
allowing up to `MAX_TAIL_RECURSION_DEPTH` (= 1000, exact parity with tsc's
`tailCount`) iterations within one stack frame. The loop keeps a
`tail_seen: FxHashSet<(check, extends, true, false)>` for cycle detection;
re-seeing an exact state, or exceeding the tail budget, calls `mark_depth_exceeded`
and returns `ERROR` (the checker's TS2589).

The algorithm, in order:

1. **Application-level `infer` pre-match** (`try_application_infer_match`): when
   both sides are `Application`s sharing a base (e.g. `Promise<string>` vs
   `Promise<infer U>`), match type arguments directly *before* expanding the
   interface, because structural expansion of complex generics like `Promise`
   loses the ability to match arguments. Result drives the true or false branch.
2. **Early generic deferral**: if the check side is already a deferred
   conditional over generic inputs and the extends side has no `infer`, re-intern
   the conditional and defer.
3. `resolve_operands` evaluates check and extends. `any extends X ? T : F`
   returns the union of both branches. `never` as a distributive check returns
   `never`.
4. **Distribution** (`distribute_conditional`): a *distributive* conditional
   (naked type-parameter check, recorded at lowering) whose check evaluates to a
   `Union` fans out one conditional per member. `NonNullable<T> = T extends null |
   undefined ? never : T` over `A | B` becomes `(A extends … ? … : A) | (B …)`.
   The per-member substitution uses `substitute_exact_type` (exact-`TypeId`
   rewrite, `evaluate_rules/substitute.rs`), gated by one `cached_contains_type_by_id`
   containment walk per branch so a branch that never references the
   distribution variable is not re-walked. The cap is
   `MAX_CONDITIONAL_DISTRIBUTION_SIZE` (= 250); exceeding it marks the guard
   exceeded and returns `ERROR`.
5. **Bare `infer` extends** (`extends_type` is `Infer`): always matches; bind the
   infer variable to the check type and take the true branch. Deferred when the
   check is still a free parameter (except during the TS2589 depth-detection
   pass, which drives the recursion).
6. **Array/tuple/object extends fast paths**: `T extends (infer U)[]`,
   `T extends [infer U]`, `T extends { … }` are matched through dedicated helpers
   (`eval_conditional_array_infer`, `eval_conditional_tuple_infer`,
   `eval_conditional_object_infer`) before the general structural path.
7. **Naked type-parameter check**: `T extends U` with `T` a bare `TypeParameter`
   stays *deferred* (re-interned `ConditionalType`) because `T` may instantiate to
   different subtypes of its constraint — tsc never eagerly resolves on the
   constraint here. Special simplifications: `T extends never ? X : Y → Y`, and
   the identity `T extends T ? X : Y → X` for non-distributive forms.
8. **General `infer` matching** (`extends_has_infer`): run `match_infer_pattern`
   on `(check, extends)`; on success substitute the bindings into the true branch
   (`substitute_infer`) and dispatch a tail call. On failure, try the check
   type's constraint, then the permissive-instantiation gate.
9. **Subtype check** (`check_conditional_subtype`): the structural fallback. If
   the check is a subtype of the extends, take the true branch; otherwise take
   the false branch *only if the relation is definitive*.

### The permissive-instantiation gate

A conditional whose check type is still generic must not vacuously take its false
branch on a failed relation. `permissive_false_branch_is_definitive` mirrors
tsc's `getConditionalType` gate: substitute every named type parameter with `any`
(tsc's `wildcardType`), evaluate both sides, and only treat the false branch as
definitive if the relation *still* fails. The implementation adds a *wildcard
fidelity guard*: if the `any` substitution leaves a permissive form that is still
a deferred index-like generic marker (`keyof <conditional>` that did not reduce
under `any`), the `any` form no longer relates the way tsc's symbolic
`wildcardType` would, so the conditional must stay deferred (the react-redux
`Matching`/`Shared` mapped conditionals). `is_generic_conditional_check_type`
(in `type_queries`) decides whether the gate applies.

### `check_conditional_subtype`

The structural subtype query (in `conditional/phases.rs`) consults the
evaluator-local `conditional_subtype_cache` keyed on `(check, extends)`, then
runs through a thread-local `ConditionalSubtypeDepthGuard` capped at depth 50.
The guard is RAII so the depth is restored even on a caught panic-unwind, keeping
relations schedule-independent across reused batch-worker threads (issue #13368).
At excessive depth it conservatively returns `false` (the deferred/false branch).
It has `tsc`-parity fast paths: a primitive is never a subtype of `Function`
(`is_primitive_vs_function`, preventing autoboxing from finding spurious
structural compatibility), the global `Function` intrinsic *does* satisfy callable
targets in conditional position (`function_intrinsic_extends_callable_target`),
and object literals with conflicting required-property literals are definitively
disjoint (`object_literals_have_conflicting_required_property`, for discriminant
`Extract`).

### Walk-through: `NonNullable<string | null>`

```
type NonNullable<T> = T extends null | undefined ? never : T
NonNullable<string | null>
```

1. `evaluate(Application(NonNullable, [string | null]))` -> `evaluate_application`
   resolves `NonNullable`'s `DefId`, instantiates the body with
   `T = string | null` -> `Conditional{ check: string | null,
   extends: null | undefined, true: never, false: string | null }`,
   distributive.
2. `evaluate_conditional`: check evaluates to the union `string | null`,
   distributive, so `distribute_conditional` runs. `false_type` references the
   check, so `false_needs_subst` is true.
3. Member `string`: `string extends null | undefined ? never : string` ->
   `check_conditional_subtype(string, null | undefined)` is false, definitive
   (extends has no type params) -> false branch -> `string`.
4. Member `null`: `null extends null | undefined ? never : null` -> subtype is
   true -> true branch -> `never`.
5. Results `[string, never]` -> `interner.union_from_slice` -> `string`.

---

## `infer` extraction

`match_infer_pattern` (in `evaluate_rules/infer_pattern.rs`) is the recursive
structural matcher that binds `infer` variables. It takes the source, the pattern
(the extends clause), a `bindings: FxHashMap<Atom, TypeId>`, an
`InferPatternVisited` set (memoizing `(source, pattern)` pairs to break cycles),
and a `SubtypeChecker`. Key behaviors:

- `source == NEVER` binds the infer defaults (`bind_infer_defaults`).
- A `Union` source distributes through `match_infer_pattern_union_members`; an
  `Intersection` source matches through whichever constituent structurally fits
  the pattern (so `ReturnType` over an intersection-of-callable still reduces).
- `Infer(info)` in the pattern binds via `bind_infer`, which honors the infer
  variable's constraint.
- `Function`/`Callable`/`Array`/`Tuple` patterns recurse position-wise; a tuple
  source matched against an array pattern projects every element.

After a successful match, `substitute_infer` rewrites the true branch with the
bindings. When the same infer variable is bound from multiple positions, the
solver's inference machinery (see [solver-inference](solver-inference.md)) owns
the candidate-merging policy; the evaluator owns only the structural extraction
and the constraint-filtering of a single binding (`filter_inferred_by_constraint`).

---

## Mapped types

`evaluate_mapped` (in `evaluate_rules/mapped.rs`) computes
`{ [K in C]: Template }`. The high-level algorithm: extract the key set from the
constraint `C`, substitute each key `K` into the template, apply
optional/readonly modifiers, and build an object (or array/tuple). The hard parts
are *when to defer*, *homomorphism*, *modifiers*, and *key remapping*.

### Deferral

The rule defers (returns a re-interned `Mapped`) in several cases:

- A remapping (`name_type` / `as`) clause whose constraint or name type still
  contains type parameters (`contains_type_parameters_db`,
  `contains_type_parameters_except_name_db`).
- A mapped type *over a bare type parameter* (`is_mapped_type_over_type_parameter`)
  — `{ [K in keyof T]: T[K] }` with `T` generic stays deferred, *except* when `T`
  is constrained to an array/tuple, where `try_evaluate_mapped_over_array_param`
  produces an array/tuple result (tsc's `instantiateMappedArrayType`).
- Key extraction failing to yield concrete keys.

`try_reduce_substituted_homomorphic_mapped` mirrors tsc's `instantiateMappedType`
short-circuit: a generic homomorphic mapped type instantiated with a *non-object*
source (primitive, literal, `never`, unique symbol, enum) reduces to that source,
distinguished from a literally-written `{ [K in keyof string]: … }` by inspecting
the iteration variable's *original* constraint.

### Homomorphism and modifiers

`homomorphic_mapped_source` (in `mapped/key_extraction.rs`) detects the
homomorphic shape `{ [K in keyof T]: T[K] }` in two forms: Method 1 matches the
pre-evaluation constraint `keyof T` with template `T[K]`; Method 2 matches the
post-instantiation form where `keyof T` was eagerly evaluated to a literal union
and verifies `evaluate_keyof(obj) == constraint`. A homomorphic mapped type
*inherits* the source's optional/readonly modifiers; tsc treats any
`{ [K in keyof T]: … }` (even when the template is not literally `T[K]`) as
homomorphic for modifier inheritance, captured by `is_homomorphic`.

Modifier computation routes through `crate::type_queries::compute_mapped_modifiers`
(called from `get_mapped_modifiers`), which combines the mapped type's explicit
`+?`/`-?`/`+readonly`/`-readonly` directives with the inherited source modifier.
The source `readonly` for an *index signature* is read from the source object's
`string_index`/`number_index` slot (`source_index_signature_readonly`), not the
property list — a fix for homomorphic maps over `{ readonly [k: string]: V }`
silently producing a writable index signature.

`strip_removed_optional_undefined` mirrors tsc's `removeMissingOrUndefinedType`:
when `-?` removes optionality, the synthetic top-level `undefined` is stripped —
but only when `exactOptionalPropertyTypes` is off, since under exact-optional an
explicit `| undefined` is not the missing marker (the cache key carries
`exact_optional_property_types` precisely so a result computed under one mode is
never served under the other).

### Key remapping and shape preservation

`remap_key_type_for_mapped` substitutes a source key into the `as` clause
(`name_type`), evaluates it, and treats a `never` result as a filtered-out key
(`Omit`-style). Array and tuple sources preserve their shape:
`evaluate_mapped_array` and `evaluate_mapped_tuple_with_readonly_source` (gated by
`is_identity_name_mapping`) keep `Partial<[number, string]>` as `[number?,
string?]` rather than degrading to a plain object, and keep array methods on
`Partial<number[]>`. The mapped-key count is bounded by `max_mapped_keys`
(`DEFAULT_MAX_MAPPED_KEYS` = 500, or 250 on `wasm32`); exceeding it marks the
guard exceeded and returns `ERROR`.

---

## `keyof`

`evaluate_keyof` (in `evaluate_rules/keyof.rs`) computes the key space. The
critical ordering at the top is: `TemplateLiteral` before `Union` (template keyof
is the apparent `string` key space), and `Union` before general evaluation (to
avoid premature union simplification). The per-`TypeData` rules:

| Operand | `keyof` result |
| --- | --- |
| `Object` / `ObjectWithIndex` / `Callable` | union of public property keys (in `declaration_order`, matching tsc's alloc-order union sort), plus index-signature key types (`extend_keyof_with_index_signature_keys`) |
| `Union` | intersection of each member's key space (`keyof_union_intersection`); deferred-conditional members are the identity element and dropped |
| `Intersection` | union of each member's key space (`keyof_intersection`), with literal-discriminant narrowing |
| `Array` | `number \| "length" \| <array methods>` (`array_keyof_keys`, from the registered `Array<T>` base when available) |
| `Tuple` | numeric index literals (`append_tuple_indices`) plus array keys |
| `Mapped` | the `name_type` (remapped key space) or the evaluated constraint |
| `TypeParameter` / `Infer` | `keyof <constraint>` when the constraint is informative, else deferred `KeyOf(operand)` |
| `Conditional` | branch-shared key reduction (`try_keyof_from_conditional_branches`) or default-constraint key space (`try_keyof_from_conditional_default_constraint`), else deferred |
| primitives / literals | apparent-type keyof (`apparent_primitive_keyof`) |
| `any` / `never` | `string \| number \| symbol` |
| `unknown` | `never` |
| `Lazy` / `Application` / `Enum` / `ThisType` / `TypeQuery` | resolve, then recurse |

`index_access` of a generic index (`T[K]` where `K` is generic) keeps `keyof`
deferred (returning `KeyOf(operand)`): eagerly resolving `T[K]` through `K`'s
constraint expands to a union of value shapes whose disjoint key sets collapse to
`never` under `keyof (A | B) = keyof A & keyof B`, exposing user code to spurious
TS2345 (issue #8725). The set-intersection arithmetic for unions lives in
`intersect_keyof_sets` / `KeyofKeySet`, which tracks `has_string`/`has_number`/
`has_symbol`, string literals, and unique symbols separately so a broad `string`
key absorbs literals correctly.

### Walk-through: `keyof { a: 1; b: 2 }`

`evaluate_keyof(Object{a,b})` -> the `Object` arm collects public properties,
sorts by `declaration_order`, maps each through `property_name_to_key_type`
(`literal_key_for_property_name` yields `"a"`/`"b"`, or a numeric literal for
bare-numeric names like `{ 1: … }`), and returns
`interner.union(["a", "b"])` -> `"a" | "b"`.

---

## Indexed access

`evaluate_index_access` (in `evaluate_rules/index_access.rs`) computes `T[K]`. The
front of the function handles the cases that must run *before* evaluating the
object, because evaluation would destroy the structure they rely on:

- A numeric literal index into a `Tuple` (`evaluate_tuple_literal_index`),
  including the `no_unchecked_indexed_access` undefined-injection.
- `MappedType[K]` where `K` extends the mapped constraint
  (`try_mapped_type_param_substitution`): substitute `K` into the template
  directly so `{ [P in "one"|"two"]: F<P> }[K]` yields `F<K>`, not a union of
  functions. The constraint-param substitution lives in
  `instantiate_mapped_template_with_constraint_param`, which mirrors tsc's
  `substituteIndexedMappedType` for generic constraints.

Then it evaluates object and index and dispatches:

- `any` object or index -> `any`; `ERROR` operand -> `ERROR`.
- `T[never]` / `T[keyof T]` over an empty key set -> `never`.
- **Index-union distribution**: `T[A | B] -> T[A] | T[B]`, capped at
  `MAX_UNION_INDEX_SIZE` (= 500).
- Otherwise the `IndexAccessVisitor` dispatches per object `TypeData`
  (`visit_object`, `visit_object_with_index`, `visit_intrinsic` for apparent
  primitives, tuple/array, etc.).

The most subtle rule is **deferral of a lossy `O[K]` distribution**:
`keyof_constraint_distribution_is_lossy` keeps `O[K]` deferred when `K` is a bare
type parameter constrained by `keyof O` and distributing `K`'s constraint over
every key would produce a value-type *union* that diverges from the single
generic type — preventing a false TS2322 on homomorphic element functions
`(x: O[K]) => O[K]` and sidestepping the quadratic value-union expansion on large
interfaces like `JSX.IntrinsicElements`. The single-key / all-identical-value
case still resolves through the constraint (`is_generic_index`); a genuinely
missing key on a concrete index returns `UNDEFINED` (or `T | undefined` under
`no_unchecked_indexed_access`).

---

## Template literals and string intrinsics

`evaluate_template_literal` (in `evaluate_rules/template_literal.rs`) expands a
span list to a union of literal strings via a **Cartesian product**. All-text
spans concatenate to one `literal_string`. Otherwise each `Type` span is
evaluated and `template_span_expansion` reports its cardinality (e.g. `boolean`
contributes 2 combinations); the running `total_combinations` is checked against
`TEMPLATE_LITERAL_EXPANSION_LIMIT` and, on overflow, the engine calls
`mark_union_too_complex` (the checker's TS2590) and returns `TypeId::STRING`. A
span that contains a non-literal type sets `can_fully_expand = false`, so the
result stays a deferred `template_literal`. When every span is enumerable, the
product is materialized into a union of `literal_string`s.

`evaluate_string_intrinsic` (in `evaluate_rules/string_intrinsic.rs`) handles
`Uppercase`/`Lowercase`/`Capitalize`/`Uncapitalize`: it maps over union members
(`Uppercase<never> = never`), applies the case transform to string literals, and
preserves a deferred `StringIntrinsic` over `any` / non-literal operands so the
intrinsic constraint survives relation checks.

---

## Generic applications

`evaluate_application` (in `evaluate/application.rs`) expands `Base<Args>`. It is
the most heavily cached path because real codebases instantiate the same
utilities thousands of times. The phases:

1. **Callee normalization**: resolve the base to a `DefId` via
   `resolve_application_def_id` (handles `Lazy`, `TypeQuery`, `UnresolvedTypeName`,
   symbol-backed objects). A base without a resolvable body stays opaque
   (`evaluate_application_no_def_id`) for a later, richer resolver.
2. **Per-`DefId` recursion guard**: `increment_def_depth` bounds re-expansion at
   `MAX_DEF_DEPTH` (= 100); over the `REAL_INSTANTIATION_BAILOUT_THRESHOLD`
   (= 40) a structural depth bail escalates to a *real* TS2589 instead of a silent
   opaque bail.
3. **Divergence guard**: `detect_recursive_growth` (in
   `evaluation/recursive_growth.rs`) flags an alias whose per-step argument
   structural weight exceeds `MAX_RECURSIVE_GROWTH_STEP` (= 100_000) or sustains
   `MAX_DETECTION_GROWTH_STEPS` (= 1000) consecutive new maxima — the unbounded
   `BuildTuple`/template-literal accumulators tsc reports as TS2589.
4. **Raw-args cache**: `lookup_application_eval_cache(def_id, args, no_unchecked)`
   — only consulted by evaluators with an explicit `query_db`, so a
   limited/noop resolver cannot observe a result computed under stronger
   resolution.
5. **Body evaluation** under an `app_body_limit_epoch` snapshot (below).

---

## Caches and invariants

Evaluation maintains a layered cache hierarchy. The invariant binding them all:
*a result that depended on the call stack, on a recursion/fuel limit, or on an
incomplete resolver must never be persisted to a cache whose key does not capture
that context.* Three sticky flags and one epoch counter enforce it.

### Limit signals

`recursion_limit_hit()` is the single OR of three sticky booleans
(`guard.is_exceeded()`, `silent_depth_bailed`, `deep_recursion_seen`); once set,
the run produced at least one stack-context artifact. The problem with sticky
booleans is granularity: the *first* bail anywhere in a run would disable every
later cache write. So `limit_epoch` (a monotonic `u32` bumped by every limit
event via `note_limit_event`) gives per-node precision. `memo_insert` snapshots
`limit_epoch` at node entry; if it moved by the time the result is written, the
node is inserted into the `tainted` set and excluded from persistent caches —
this is the per-entry discrimination (issue #13241) that lets a clean subtree of
a partially-bailed run still be reused. Reading a tainted entry back from the
local cache records a fresh limit event so the taint propagates to in-flight
ancestors.

### The cache layers

| Cache | Scope | Key | Holds | Gate / invalidation |
| --- | --- | --- | --- | --- |
| `cache: FxHashMap<TypeId, TypeId>` | per-evaluator | `TypeId` | every node's result | dropped with the evaluator; cleared by `reset` and on `set_no_unchecked_indexed_access` change; tainted entries flagged for downstream filtering |
| `conditional_subtype_cache` | per-evaluator | `(check, extends)` | `bool` | memoizes the conditional subtype query |
| `contains_infer_cache` / `contains_type_by_id_cache` | per-evaluator | `TypeId` / `(root, target)` | `bool` | pure structural predicates |
| structural-inertness bit | interner-wide, shared | `TypeId` | "evaluates to itself under any resolver" | monotonic; populated by `is_structurally_eval_inert`; never invalidated (a closed structural type's inertness is permanent) |
| persistent eval memo (`lookup_eval_memo`/`insert_eval_memo`) | interner-wide | `(TypeId, no_unchecked)` | clean-window result | written only by plain (`NoopResolver`, default-mode, `persistent_memo_reads`) evaluators; read only by the same context; taint- and TS2590-gated (issue #13097) |
| `closed_eval_cache` | project-wide | `(TypeId, no_unchecked, exact_optional)` | substitution-independent result | input/write/limit gates (below) |
| `application_eval_cache` | cross-evaluator | `(DefId, args, no_unchecked)` | application result | `app_body_limit_epoch` gate; limited resolvers read but never write |
| cross-eval `QUERY_MEMO` | thread-local, per top-level query | `(TypeId, no_unchecked)` | fresh-sub-evaluator result | cleared at every top-level query start; only stable (non-bailed) results stored |

### `closed_eval_cache` gates

The substitution-independent cache (`evaluate/closed_eval.rs`) can only change
speed, never results, because of three gates documented at the module head:

- **Input gate**: a cached node holds no
  `TypeParameter`/`Infer`/`ThisType`/`BoundParameter`
  (`is_substitution_dependent_type` is false), so its value is a pure function of
  `(TypeId, options)` and the project's single fixed resolver. Eligible kinds are
  `IndexAccess`/`KeyOf`/`Application`/`Conditional`, subject to structural
  exclusions: no syntactic `Conditional` inside an `IndexAccess`/`KeyOf` body
  (`body_has_conditional`, because a conditional can bind `infer` against use-site
  inference/narrowing/contextual state the key does not capture), and no
  index-signature-bearing operand (`is_index_object_cacheable`, because the
  checker derives element-access diagnostics from the structural mapped form).
- **Write gate (kind split)**: the meta-operation kinds
  (`IndexAccess`/`KeyOf`/`Application`) commit only from the checker's
  authoritative, context-free pass (`closed_eval_writes_allowed` *and*
  `query_db.is_some()`); a resolver-backed mid-relation/inference/narrowing
  evaluator runs against a *partial* resolver and may compute an under-resolved
  head. A *closed* `Conditional` that resolved to a definite branch is exempt and
  may commit from any top-level evaluation — the deep-recursion win for
  ts-toolbelt `AutoPath`/`MetaPath`/`Join` families (issue #13250).
- **Limit gate**: a run that hit any recursion/complexity limit
  (`recursion_limit_hit`), evaluated an unresolved-base application
  (`unresolved_def_seen`), or newly tripped TS2590 caches nothing.

The read path additionally refuses to serve a materialized
`IndexAccess`/`KeyOf`/`Application` to a `limited_resolver` evaluator, because a
fully-materialized form fed into in-flight inference yields a different
(under-reduced) answer that would poison a later authoritative read
(`RequiredKeys` collapsing to `never`).

### The cross-evaluator runaway breaker

`infer`-pattern matching and the subtype checker construct *fresh*
`TypeEvaluator`s mid-relation, each with empty guards, so a recursive
conditional/`infer` utility (`Unbox<Box<2>>`, `Awaited<Promise<2>>`) re-enters
the same `TypeId` through new evaluators and no per-instance guard fires.
`evaluation/cross_eval_guard.rs` defends with a thread-local `ACTIVE` set
(`CrossEvalExpansionGuard`): re-entering an in-flight `TypeId` returns `None`, and
the caller treats the type as not-yet-resolved so the in-flight expansion
converges. The companion `QUERY_MEMO` memoizes stable fresh-evaluator results per
top-level query and is reset by `EvalQueryFrame` whenever a new top-level query
begins. The op budget itself (`query_budget.rs`) counts every `evaluate` across
all instances; exceeding `DEFAULT_MAX_EVAL_OPS_PER_QUERY` (= 2_000_000) marks the
evaluator bailed and leaves the type opaque — tsc's global `instantiationCount`
applied across the fresh-instance boundary.

---

## Edge cases and `tsc` parity

- **Error propagation vs. unresolved names.** `visit_conditional` bails to
  `ERROR` only on a *genuine* error check type (`is_genuine_error_type`), not on
  an `UnresolvedTypeName` (a display-preserving cross-arena reference), which the
  broad `is_error_type` would fold into "error" and fabricate `{ k: error }`
  through a homomorphic body.
- **`any` in a conditional check** returns the *union* of both branches, not just
  the true branch, with `infer` patterns bound to `any`
  (`any extends infer U ? U : never → any`).
- **`keyof (A | B)` over deferred conditionals.** A `keyof` member that does not
  reduce (a self-referential conditional whose true branch is itself) is the
  *universal* key space and is the identity element of the union intersection, so
  `R<T> | Concrete` lets the concrete branches own the key space rather than
  collapsing to an opaque `KeyOf` that triggers spurious TS2536
  (`keyof_member_is_universal`).
- **Readonly source variance.** A conditional strips a `ReadonlyType` wrapper from
  the source only when the target is itself a readonly-array shape
  (`target_accepts_readonly_source`); against a mutable array target the wrapper
  carries the TS4104 variance signal and the relation must fail (issue #9743).
- **Promise fulfillment cycles.** `visit_lazy` keeps `type T = string |
  Promise<T>` opaque at the outer lazy boundary
  (`is_self_recursive_promise_union`), because structural comparison of
  `Promise<T>`'s callbacks would chase `T -> Promise<T> -> T` forever — while
  general recursive unions (`Json`, recursive arrays) still expand.
- **Bare-numeric keyof.** `keyof { 1: … }` yields the numeric literal `1`, not
  `"1"` (`literal_key_for_property_name`), the same rule that drives mapped-type
  key substitution.
- **Symbol-keyed homomorphism.** Mapped/keyof evaluation round-trips well-known
  symbols (`Symbol.iterator`) through their canonical `[Symbol.xxx]` text key
  rather than the synthetic `__unique_N` placeholder
  (`symbol_named_atom_from_unique_symbol_ref`), so `{ [K in keyof T]: … }` over an
  `Iterable` does not silently drop the iterator method.
- **TS2589 detection pass.** `with_flag_depth_on_app_cycle` puts the evaluator in
  a mode that drives recursive branches (rather than deferring) so an
  unconditionally-recursive alias re-applies itself and the guard observes the
  depth — the `is_depth_detection_pass()` checks scattered through
  `evaluate_conditional` are the parity hooks for this.
- **Option-keyed results.** The `no_unchecked_indexed_access` and
  `exact_optional_property_types` flags are part of every persistent cache key
  (`EvaluationCacheKey`) because both change results (indexed access of
  optional/array members, homomorphic-modifier `undefined` stripping); a key that
  omitted either could serve a result computed under a different option set.

---

## Cross-references

- The structural subtype kernel conditionals call into:
  [solver-relations](solver-relations.md).
- `infer` candidate merging and contextual inference:
  [solver-inference](solver-inference.md).
- Generic substitution (`instantiate_generic_cached`, `instantiate_type`):
  [solver-instantiation](solver-instantiation.md).
- Narrowing reuses the same `IndexAccessOptions` newtype and shares
  option-keyed caches: [solver-narrowing](solver-narrowing.md).
- Arithmetic/string operations the checker asks for sit beside evaluation:
  [solver-operations](solver-operations.md).
- `TypeId`, `TypeData`, `DefId`, and the interner constructors:
  [solver-types-intern-def](solver-types-intern-def.md).
- Object/property collection and the contextual/compat caches:
  [solver-caches-objects-contextual-compat](solver-caches-objects-contextual-compat.md).
- How the checker invokes evaluation through `TypeEnvironment` and stabilizes
  `DefId`s: [checker-context-and-state](checker-context-and-state.md) and
  [checker-declarations-modules](checker-declarations-modules.md).
- Where evaluation results feed TS2322/TS2345/TS2416:
  [checker-assignability-gateway](checker-assignability-gateway.md).
- The full pipeline timeline:
  [end-to-end-timeline](end-to-end-timeline.md).
