# The Call-Evaluation Kernel: Arguments, Contextual Typing, and Reverse/Mapped Inference

A call expression like `f(a, b)` enters the checker as syntax and leaves as a
return `TypeId` (or a diagnostic). The checker resolves the callee, expands
spreads, sets up contextual typing, and decides *which* `TypeId` to hand the
solver — but the moment it asks "does this signature apply, and if generic, what
are the inferred type arguments?", control crosses into a single solver struct:
`CallEvaluator`. This document traces that struct's machinery. It is the
solver-side *driver* that sits between [checker-calls-signatures-generics](checker-calls-signatures-generics.md)
(the AST orchestration) and the raw [solver-inference](solver-inference.md)
engine (`InferenceContext`, candidate collection and resolution). The
`CallEvaluator` owns neither end: it does not parse AST, and it does not
re-implement the union-find / best-common-type kernel. It owns the *call
algorithm* — argument-count gating, the `this` check, overload looping, the
two-round (Round 1 / Round 2) inference schedule, the structural constraint
walker, reverse-mapped wiring, constructor (`new`) semantics, and the
construction of the final `CallResult`.

This is WAVE-2 depth: the sibling docs describe the boundaries. Here we go into
the kernel that the boundaries call. Everything below lives under
`crates/tsz-solver/src/operations` (`core/`, `call_args.rs`,
`call_contextual.rs`, `constructors.rs`, `spread_args.rs`, `generic_call/`,
`constraints/`) plus the inference glue in `crates/tsz-solver/src/inference`.
Where the engine internals (priority filtering, BCT, contra resolution, literal
widening) are owned by `InferenceContext`, this doc links to
[solver-inference](solver-inference.md) rather than re-explaining them.

## Owns / Must not own

| | Owns | Must not own |
|---|---|---|
| **`CallEvaluator` (`operations/core`, `operations/generic_call`, `operations/constraints`)** | Argument-count bounds, `this`-receiver gating, overload looping, two-round inference scheduling, the structural constraint walker (`constrain_types`), reverse-mapped/reverse-keyof wiring, constructor semantics, spread-marker recognition, building `CallResult` | AST traversal, source spans, diagnostic *text* (it returns structured `CallResult` variants, not strings); the relation kernel; the candidate-resolution math |
| **Checker adapter (`CheckerCallAssignabilityAdapter`)** | Supplying `is_assignable_to`, `evaluate_type`, `type_resolver`, placeholder-id allocation, alias expansion through the file's `TypeEnvironment` | Inference scheduling; constraint collection; choosing which round an arg belongs to |
| **`InferenceContext` (`inference/`)** | The union-find table of `InferenceVar`, per-variable `candidates`/`contra_candidates`/`upper_bounds`, `InferencePriority` filtering, BCT, fixing, strengthening | Call shape; what an "argument" is; overloads; `this`; spreads |

The hard rule the checker side obeys: it never pattern-matches `CallEvaluator`
internals or constructs a raw `TypeKey`. It hands the evaluator a callee
`TypeId` and the argument `TypeId`s, and reads back a `CallResult`. The
evaluator, in turn, never formats a diagnostic — it returns
`ArgumentTypeMismatch { index, expected, actual, fallback_return }` and lets the
[checker-error-reporter-diagnostics](checker-error-reporter-diagnostics.md) layer
turn that into `TS2345`.

## Module map

| Path | Role |
|---|---|
| `operations/core/call_evaluator.rs` | The `AssignabilityChecker` trait, `CallResult` enum, the `CallEvaluator<'a, C>` struct and all its per-call state/caches, `get_contextual_signature*` (the contextual-signature extractor `TypeVisitor`), union-signature combination |
| `operations/core/call_resolution.rs` | `resolve_call` (dispatch by `TypeData`), `resolve_function_call`, `resolve_callable_call` (overloads), `resolve_union_call`/`resolve_intersection_call`, and the free-function entry points `resolve_call_with_checker*` |
| `operations/core/call_inference_shape.rs` | `generic_function_shape_for_inference` — α-renames the callee's type parameters when an argument carries a same-named outer type parameter (collision guard) |
| `operations/call_args.rs` | `check_argument_types_with` (the per-argument assignability loop), `arg_count_bounds`, spread-marker and aggregate-rest handling |
| `operations/call_contextual.rs` | `is_contextually_sensitive` (+ memo), callback-arity precheck, contextual-signature-strict compatibility |
| `operations/spread_args.rs` | Recognizes the `__tsz_spread_argument__` and bare `[...T]` spread-marker tuples the checker synthesizes |
| `operations/constructors.rs` | `resolve_new` and friends: construct-signature dispatch, `TS2350`, mixin intersection, union-must-all-construct strictness |
| `operations/generic_call/resolve.rs` | `resolve_generic_call_inner` — the Round 1 / fix / Round 2 schedule |
| `operations/generic_call/resolve/finalize.rs` | `finish_generic_call_resolution` — final resolution, constraint check (`TS2344`), return-type instantiation, post-inference argument re-check |
| `operations/generic_call/return_context.rs` | `resolve_generic_call` / `resolve_with_request` entry, contextual-return seeding |
| `operations/constraints/walker.rs` | `constrain_types` / `constrain_types_impl` — the structural constraint collector |
| `operations/constraints/signatures.rs` | `constrain_parameter_types` (contravariant `in_contra_mode`), function-to-signature matching |
| `operations/constraints/reverse_mapped.rs` | `constrain_reverse_mapped_type`, `reverse_infer_through_template` — homomorphic mapped-type reversal |
| `operations/iterators.rs` | `get_iterator_info` / element-type extraction used when spreads are lowered |

---

## Where the checker enters: `AssignabilityChecker` and the entry points

`CallEvaluator` is generic over `C: AssignabilityChecker`
(`operations/core/call_evaluator.rs`). That trait is the *only* hole through
which the evaluator reaches semantic answers it does not own. The default-method
shape tells you exactly what the kernel delegates:

```rust
pub trait AssignabilityChecker {
    fn is_assignable_to(&mut self, source: TypeId, target: TypeId) -> bool;
    fn is_assignable_to_strict(&mut self, ...) -> bool { ... }            // subtype pass
    fn is_assignable_to_bivariant_callback(&mut self, ...) -> bool { ... }
    fn evaluate_type(&mut self, type_id: TypeId) -> TypeId { type_id }     // resolver-backed
    fn expand_type_alias_application(&mut self, _: TypeId) -> Option<TypeId> { None }
    fn promise_like_type_argument(&mut self, _: TypeId) -> Option<TypeId> { None }
    fn type_resolver(&self) -> Option<&dyn TypeResolver> { None }
    fn are_types_identical(&mut self, a, b) -> bool { ... }
    fn normalize_inferred_type(&mut self, type_id: TypeId) -> TypeId { type_id }
    fn next_inference_placeholder_id(&mut self) -> u64 { ...global counter... }
}
```

The default implementations make the evaluator *usable from pure solver unit
tests* (where there is no file, no `TypeEnvironment`, and placeholder names never
reach a diagnostic). In the real pipeline the implementor is
`CheckerCallAssignabilityAdapter` (`crates/tsz-checker/src/checkers/call_checker/mod.rs`),
which routes `is_assignable_to` through the file's compatibility relation,
`evaluate_type` through the checker's `TypeEnvironment` resolver, and
`next_inference_placeholder_id` through a *deterministic per-file counter* so any
placeholder name that surfaces in a diagnostic is stable across parallel file
checks. The adapter's `is_assignable_to` also short-circuits to `false` when the
checker-only assignability layer (`checker_only_assignability_failure_reason`)
has a reason — this is how DOM/checker-only quirks stay out of the solver.

The free-function entry points (`call_resolution.rs`) are the public surface the
[checker-calls-signatures-generics](checker-calls-signatures-generics.md) layer
calls:

```text
resolve_call_with_checker_and_arg_sources(interner, checker, func_type, arg_types, opts)
  └─ CallEvaluator::new(interner, checker)
     ├─ set_force_bivariant_callbacks / set_contextual_type / set_actual_this_type
     ├─ set_arg_source_is_type_annotation / set_arg_source_is_readonly_annotation
     └─ evaluator.resolve_call(func_type, arg_types)
        → (CallResult, last_instantiated_predicate, last_instantiated_params)
```

The two side channels — `last_instantiated_predicate` and
`last_instantiated_params` — are how a generic call hands back data the checker
needs *after* the call succeeds: the instantiated type predicate (for flow
narrowing) and the instantiated parameter types (so the checker can run excess
property checking on the post-inference shapes rather than the raw, type-variable
shapes). `resolve_new_with_checker` is the parallel entry for `new` expressions.

---

## `resolve_call`: dispatch by callee shape

`resolve_call` (`call_resolution.rs`) is a `match` on `interner.lookup(func_type)`:

| Callee `TypeData` | Handler |
|---|---|
| `Function(_)` | `resolve_function_call` (the core; handles generics) |
| `Callable(_)` | `resolve_callable_call` (overload set) |
| `Union(_)` | `resolve_union_call` — combine compatible signatures across members |
| `Intersection(_)` | `resolve_intersection_call` — first callable member wins |
| `Application(_)` | `checker.evaluate_type`, retry; else `expand_type_alias_application`, retry; else `NotCallable` |
| `TypeParameter` with callable constraint | recurse into the constraint |
| `Conditional(_)` | evaluate; if deferred, both branches must be callable |
| otherwise | `NotCallable { type_id }` |

Note the `Application` arm: the evaluator does **not** itself know how to resolve
a cross-file generic alias to its body. It asks the adapter
(`evaluate_type`, then `expand_type_alias_application`), and only after expansion
decides callability *structurally*. This keeps the "what does `Lazy(DefId)`
resolve to" question in the checker's `TypeEnvironment`, per the architecture
contract that semantic refs are `TypeData::Lazy(DefId)`.

The first two lines of `resolve_call` clear the side channels
(`last_instantiated_predicate = None; last_instantiated_params = None;`) so a
non-generic call never leaks a previous generic call's predicate.

---

## The non-generic path: `resolve_function_call`

For a `FunctionShape` with `type_params.is_empty()`, `resolve_function_call`
(`call_resolution.rs`) runs a fixed sequence:

```text
1. (generic fast paths skipped — type_params empty)
2. Compute deferred_this_error (do NOT return yet):
     receiver_constraining_this_type(func.this_type)  // this: void opts out (TS2684)
     if actual_this not assignable to expected_this → ThisTypeMismatch (deferred)
3. arg_count_bounds(&func.params) → (min_args, max_args)
     arg_types.len() < min_args → ArgumentCountMismatch  (or TS2345 for variadic
                                   tuple rest / `...args: never`)
     arg_types.len() > max      → ArgumentCountMismatch
4. check_argument_types(&func.params, arg_types, is_method)  → maybe ArgumentTypeMismatch
5. `...args: never` post-check (args-as-tuple <: never → TS2345)
6. NOW return deferred_this_error if any
7. CallResult::Success(func.return_type)
```

The ordering of step 2 vs step 6 is a deliberate `tsc` parity point: `tsc`
reports argument errors (`TS2345`) **before** `this`-context errors (`TS2684`),
so the `this` check is computed early (to know what to report) but its result is
withheld until arguments pass. The comment in the code spells this out.

`receiver_constraining_this_type` filters out `this: void`: a callable declared
`this: void` opts out of the receiver check entirely and accepts any receiver,
mirroring `tsc`'s `checkApplicableSignature` gate.

### Overloads: `resolve_callable_call`

A `Callable` with more than one call signature loops every signature through
`resolve_function_call`, collecting the failure shapes. The result-selection
logic mirrors `tsc`'s overload error precedence:

- First `Success` wins immediately.
- If **all** signatures fail on argument **count**, emit one
  `ArgumentCountMismatch` (or `OverloadArgumentCountMismatch` = `TS2575` when no
  overload matches the exact arity but two surrounding fixed arities exist).
- If **all** type mismatches are *identical* (same index/expected/actual) and
  nothing else failed, collapse to a single `ArgumentTypeMismatch` (`TS2345`)
  rather than `NoOverloadMatch` (`TS2769`). The same collapse applies to
  identical `this`-type mismatches.
- Otherwise `NoOverloadMatch { failures, fallback_return }`. The
  `fallback_return` comes from `overload_failure_return_type`, which intersects
  the candidate return types (`getIntersectionType(candidates.map(returnType))`)
  so disjoint primitives collapse to `never` (suppressing cascades) while
  object returns merge (member access still resolves). Generic overloads keep
  the simpler last-signature recovery type.

---

## Contextual sensitivity: deciding what waits for Round 2

Before any inference runs, the kernel classifies each argument as
*contextually sensitive* or not. This is the hinge of the two-round schedule. The
predicate is `is_contextually_sensitive` (`call_contextual.rs`), memoized per
`TypeId` in `contextual_sensitivity_cache` (a `RefCell<FxHashMap<TypeId, bool>>`
on the evaluator) to avoid exponential re-traversal on deep `Application` chains.

The rule approximates `tsc`'s AST-level `isContextSensitive`: a value is
contextually sensitive only when it still *needs* contextual typing to be pinned
down. Concretely:

- `Function(shape)` → sensitive iff `function_signature_is_contextually_sensitive(&params)`
  (a parameter is `any`-typed or carries an inference placeholder). A fully
  annotated function — including a generic function reference like
  `id<T>(x: T) => T` — is **not** sensitive and participates in Round 1.
- `Object`/`ObjectWithIndex` → sensitive only for a **fresh literal**
  (`ObjectFlags::FRESH_LITERAL`) whose properties are themselves sensitive.
  Class instances and evaluated generic shapes are never sensitive — their types
  are already determined.
- Union/Intersection/Array/Tuple/Application/Conditional/Mapped/etc. → recurse
  into members; `Callable` (a class-constructor value), intrinsics, literals,
  `Lazy`, `Recursive`, etc. are never sensitive.

The sibling `type_uses_inference_placeholders` is the related probe used to
decide whether a *target* type still has open placeholders.

---

## The generic path: `resolve_generic_call_inner` step by step

This is the heart of the kernel. When `func.type_params` is non-empty,
`resolve_function_call` routes to `resolve_generic_call` →
`resolve_with_request` → `resolve_generic_call_inner`
(`generic_call/resolve.rs`), after two fast guards:

1. `resolve_trivial_single_type_param_call` (`generic_call/normalization.rs`):
   the closed-form `identity<T>(x: T): T`-shaped call — one type param, one
   non-rest non-optional param that *is* `T` (or a union containing bare `T`),
   return mentions `T`, no `this`/predicate, no contextual override, arg not
   contextually sensitive. Resolves without spinning up the full inference
   context.
2. `generic_function_shape_for_inference` (`core/call_inference_shape.rs`): the
   **collision guard**. If any argument structurally references a
   `TypeParameter` whose *name* collides with one of the callee's own type
   params (but is not one of the callee's own param ids), the callee's type
   params are α-renamed to fresh `__infer_*` placeholders before inference, so an
   outer `T` cannot be confused with the callee's `T`.

### Phase 0 — set up inference variables

```text
infer_ctx = InferenceContext::with_query_db(interner)   // resolver + query-db wired in
for each tp in func.type_params:
    var = infer_ctx.fresh_var()
    placeholder_id = checker.next_inference_placeholder_id()   // deterministic, per-file
    placeholder_atom = intern("__infer_<id>")
    infer_ctx.register_type_param(placeholder_atom, var, tp.is_const)
    placeholder = intern(TypeParameter { name: placeholder_atom,
                                         constraint: tp.constraint,
                                         origin: InferPlaceholder { id } })
    substitution.insert(tp.name, placeholder)   // T → __infer_N
    var_map.insert(placeholder, var)            // __infer_N → InferenceVar
```

Each type parameter gets a *unique* placeholder type whose name is the synthetic
`__infer_*` atom, not the user's `T`. Registering the placeholder name (not `T`)
with the inference context is what keeps occurs-checks from being confused by an
identically named outer `T`. Two important side effects happen here:

- If `tp.constraint` is concrete (contains no placeholder), it is added as an
  `add_upper_bound` and recorded via `set_declared_constraint`. If the
  constraint is a primitive family,
  `mark_declared_constraint_preserves_literals(var)` is set so `T extends string`
  preserves a fresh `"z"` candidate.
- If the type parameter occurs at top level of the return type *and* is inferred
  only from top-level positions (`type_param_preserves_inferred_literal`),
  `mark_top_level_in_return_type_unfixed(var)` is set. This mirrors `tsc`'s
  `getCovariantInference` gate: such a variable's fresh literal candidates are
  **not** widened, so `'a' extends 'a' ? never : 'a'` reduces correctly and a
  forbidden argument reaches a `never` parameter (`TS2345`).

The per-call placeholder bookkeeping (`current_call_inference_placeholders`,
`shared_inference_placeholders`) is computed *only when an argument is itself a
generic function* — the sole trigger for TS-3.4 higher-order re-generalization —
because the per-parameter `collect_referenced_types` walk is expensive for large
signatures. Both sets are always cleared first so a later read can never observe
a previous call's state.

The evaluator also resets its constraint guards at this point:
`constraint_pairs.clear()`, `constraint_fixed_union_members.clear()`,
`constraint_recursion_depth.set(0)`, `constraint_step_count.set(0)`.

### Phase 1 — Round 1: non-contextual arguments

```text
for (i, arg_type) in arg_types:                       // skip rest-tuple positions
    target_type = param_type_for_arg_index(instantiated_params, i, len)
    if arg is contextually sensitive AND a later generic-function-like arg
       depends on the same type param → defer (saw_deferred_arg = true; continue)
    (contextual_arg_type, contextual_target_type) = contextual_round1_arg_types(...)
       └─ None  → this arg is fully deferred to Round 2; continue
    ... eager concrete checks (constraint satisfaction, function-union compat) ...
    source_for_inference = widen object-literal props if target is a bare
                           placeholder without a contextual seed
    source_for_inference = substitute_caller_type_params(...) if arg is sensitive
    source_for_inference = instantiate_generic_function_argument_against_target(...)
    constrain_types_for_arg_source(i, infer_ctx, var_map,
                                   source_for_inference, contextual_target_type, priority)
    if either side is function-like and target has no placeholders:
        constrain_return_context_structure(...)        // return-context candidates
    if same-base Application on both sides: directly constrain matching args
```

Round 1 processes everything that does **not** need contextual typing —
primitives, arrays, plain objects, variable references, and the non-contextual
parts of mixed objects. The point is to pin down the type parameters that
contextually sensitive arguments (lambdas) will later consume. A handful of
hard-won parity behaviors live in this loop:

- **First-wins for repeated naked type params.** For `g<T>(a: T, b: T)`, `tsc`
  keeps the first primitive-family candidate and reports the later conflicting
  argument. The evaluator tracks `first_direct_primitive_candidate` per var and
  records `first_direct_primitive_mismatch` rather than merging `""` and `3` into
  a union. The exception: a *nullable* later argument
  (`("a", "b" | undefined)`) is not skipped, because `tsc`'s
  `getCommonSupertype` strips nullable before tournament reduction and adds it
  back — skipping would lose the `"b"` candidate.

- **Direct-placeholder tracking.** `direct_param_vars` collects the inference
  vars that appear as a *naked* (top-level, not inside a union/intersection)
  parameter type. These get first-wins treatment; vars inside `T | string` come
  from union decomposition and should merge into a union (`getCommonSupertype`)
  instead.

- **Leaked-caller-type-param deferral.** When the checker contextually typed an
  inline arrow using the union of overload signatures, the arrow's parameter
  types may still carry the caller's *pre-substitution* `T`. Inferring from those
  would poison Round 1, so the arg is deferred to Round 2 where it is re-typed
  with the resolved overload's contextual type.

The actual constraint recording is `constrain_types_for_arg_source`, which wraps
`constrain_types` and flips `infer_ctx.source_is_type_annotation` when the
argument came from an explicit annotation/assertion (so a literal from `x as 'B'`
is recorded non-fresh and not widened).

### Phase 2 — fixing between rounds

After Round 1:

```text
if return type is a bare type param with no candidates and no deferred arg
   covers it → add the contextual type as a ReturnType candidate
infer_ctx.fix_current_variables_with(Some(|s,t| checker.is_assignable_to(s,t)))
build fixed_subst:  for each fixed var → { placeholder_atom → resolved, tp.name → resolved }
re-seed inference from `this` (variadic `[...T, ...U]` split) if vars remain unfixed
```

`fix_current_variables_with` (in `inference/infer_resolve.rs`, owned by
[solver-inference](solver-inference.md)) is the bridge: it resolves every var
that already has candidates and *freezes* it by writing the `resolved` field, so
a lower-priority Round 2 constraint can't overwrite it. The closure it receives
is the checker's `is_assignable_to`, used for co/contra resolution so `Lazy`
types compare through their `extends` chains. The result is `fixed_subst`,
mapping both the synthetic placeholder atom and the original `T` name to the
resolved type. Crucially, *unfixed* placeholders stay intact so Round 2 can still
infer them.

There is a deliberate `tsc`-parity carve-out in the fixer: a variable whose only
covariant candidate is `never` (with no contra-candidates) is **not** fixed —
`never` is a useless contextual type for Round 2, and a deferred argument may
provide a better candidate. If `never` really is correct, final resolution
re-derives it.

### Phase 3 — Round 2: contextual arguments

```text
if saw_deferred_arg:
    round2_params = instantiated_params re-instantiated with fixed_subst (if non-empty)
    for (i, arg_type) in arg_types:
        if !is_contextually_sensitive(arg) AND not a deferred generic-fn arg → skip
        conflict precheck: conflicting_contextual_signature_instantiation_type(...)
        r2_target = re-instantiated target (resolved placeholders filled in,
                    unresolved ones preserved; callback-param placeholders kept)
        r2_priority = NakedTypeVariable if r2_target is a bare placeholder else ReturnType
        constrain_types(infer_ctx, var_map, r2_arg, r2_target, r2_priority)
        if function-like: constrain_return_context_structure(...)
        special: function arg → tuple inference into a `...rest: T` type param
```

Round 2 only processes the deferred (contextually sensitive) arguments, now that
Round 1 has fixed the type parameters those lambdas depend on. The lambda
`(a) => a.length` finally gets `a: string` because `r2_target` was re-instantiated
with `fixed_subst`. The priority choice matters: a bare-placeholder target uses
`NakedTypeVariable` so direct argument inference outranks contextual-return
substitution; otherwise `ReturnType` priority is used so the return position
yields only when nothing better exists.

### Phase 4 — `finish_generic_call_resolution`

`finalize.rs` performs the closing sequence:

```text
1. strengthen_constraints()          // SCC cycle unification + fixed-point propagation
2. resolve source vars (__infer_src_*) from generic-function arguments, substitute
   their concrete results back into outer-var candidates (multi-pass)
3. for each tp:  resolve_with_constraints_by(var, |s,t| checker.is_assignable_to)
                 → final_subst[tp.name] = ty   (or tp.default / constraint fallback)
4. constraint check (TS2344): instantiate tp.constraint with final_subst, evaluate,
   arg_satisfies_type_parameter_constraint(ty, constraint_ty)?
      └─ on failure, try un-widened literal candidates; else fall back to the
         constraint type so the *argument* check reports TS2345, not TS2344
5. raw_return_type = instantiate_call_type(func.return_type, final_subst)
   + hoist source placeholders, store display_alias (e.g. `D<unknown>`)
6. final post-inference argument re-check:
   check_argument_types_with(instantiated_params, final_args, strict=true, is_method)
7. record last_instantiated_params / last_instantiated_predicate
8. CallResult::Success(return_type)
```

Two subtleties worth flagging:

- **Default vs constraint fallback.** When inference produces no candidate, a type
  parameter falls back to its `default`. The evaluator records
  `default_fallback_tp_names`; a later argument mismatch against a default-using
  parameter is *suppressed* (`CallResult::Success`) because a default is a
  fallback, not a requirement. A constraint fallback (step 4) is the opposite: the
  variable is set to its constraint *precisely so* the argument re-check emits
  `TS2345`.

- **Const type parameters.** A bare `const`-modified `x: T` always type-checks by
  construction (the argument *is* `T`), so an argument mismatch at a bare const
  param position is skipped — the mismatch is an artifact of the checker computing
  the arg type with `in_const_assertion` while the solver applied
  `apply_const_assertion` separately.

```text
                resolve_generic_call_inner
  ┌──────────────────────────────────────────────────────────────┐
  │ Phase 0  alloc InferenceVar + __infer_N placeholder per tp    │
  │          add concrete constraints as upper bounds             │
  └───────────────────────────┬──────────────────────────────────┘
                              │
  ┌───────────────────────────▼──────────────────────────────────┐
  │ Phase 1  ROUND 1  non-contextual args                         │
  │          constrain_types(source, target, priority)            │
  └───────────────────────────┬──────────────────────────────────┘
                              │
  ┌───────────────────────────▼──────────────────────────────────┐
  │ Phase 2  fix_current_variables_with(checker.is_assignable_to) │
  │          build fixed_subst   (freezes Round-1 winners)        │
  └───────────────────────────┬──────────────────────────────────┘
                              │
  ┌───────────────────────────▼──────────────────────────────────┐
  │ Phase 3  ROUND 2  contextual args (lambdas), re-instantiated  │
  │          targets from fixed_subst                             │
  └───────────────────────────┬──────────────────────────────────┘
                              │
  ┌───────────────────────────▼──────────────────────────────────┐
  │ Phase 4  finalize: strengthen → resolve → TS2344 check →      │
  │          instantiate return → re-check args → CallResult      │
  └──────────────────────────────────────────────────────────────┘
```

---

## The constraint walker: `constrain_types`

Every `constrain_types(ctx, var_map, source, target, priority)` call
(`constraints/walker.rs`) records evidence into `InferenceContext`. It is the
mechanical heart that the two rounds call repeatedly. The contract:
**collect `source <: target` constraints**, where `var_map` maps placeholder
`TypeId`s to `InferenceVar`s.

The public `constrain_types` is the guarded wrapper; the real work is
`constrain_types_impl`. The guards (described under *Caches and invariants*
below) run first, then dispatch:

```text
source == target                       → return (nothing to learn)
target is a placeholder (var_map hit)  → ctx.add_candidate(var, source, priority)   // lower bound
source is a placeholder (var_map hit)  → in_contra_mode ? add_contra_candidate(var, target)
                                                          : add_upper_bound(var, target)
source == ANY                          → propagate_type_to_placeholders(...)         // tsc propagationType
source == UNKNOWN || target == ANY     → stop
source/target is Lazy(DefId)           → checker.evaluate_type, recurse
array-like ↔ array-like                → recurse element-wise (sets in_array_element_context,
                                          in_readonly_source_context)
ReadonlyType / NoInfer wrappers        → look through (NoInfer blocks inference into wrapped type)
IndexAccess ↔ IndexAccess              → recurse obj/idx
KeyOf ↔ KeyOf                          → recurse *reversed* (contravariant)
Mapped source                          → evaluate, recurse
Mapped target                          → REVERSE-MAPPED path (see below)
Function ↔ Function / Callable         → constrain_parameter_types + return
```

The placeholder arms are the base case. Adding `source` as a *candidate*
(lower bound) for a target placeholder is the normal covariant inference. The
`source`-is-placeholder arm has the variance switch: in `in_contra_mode` (set
while descending function parameters) the target becomes a *contra-candidate*
resolved by intersection, not a hard upper bound — matching `tsc`, where
contravariant inferences go to `contraCandidates`.

### Function-to-function: variance and `constrain_parameter_types`

When both sides are functions, parameters are constrained contravariantly and
returns covariantly. `constrain_parameter_types` (`constraints/signatures.rs`) is
where `in_contra_mode` is toggled:

```text
constrain_parameter_types(source_param, target_param):
  if target_param is a bare placeholder:
      add_contra_candidate(var, source_param, priority)
      if source_param is NOT itself a type param:
          in_contra_mode = true; constrain_types(target_param, source_param); restore
  else if target_param structurally contains a placeholder:
      in_contra_mode = true
      constrain_types(source_param, target_param)   // forward, routed to contra
      constrain_types(target_param, source_param)   // reverse
      restore
  else:
      constrain_types(target_param, source_param)   // plain contravariant
```

The double-direction-in-contra-mode case fixes a real parity bug: decomposing a
union target like `{kind:T}` against `{kind:'a'}|{kind:'b'}` would otherwise add
separate hard upper bounds `'a'` and `'b'`, producing a false `TS2345` when the
covariant result `'a'` fails to satisfy upper bound `'b'`. Routing both
directions through contra-candidates resolves it by intersection instead. The
covariant return is constrained at `priority.max(ReturnType)` so return-position
inferences never outrank a direct argument.

---

## Reverse-mapped inference

The `Mapped` target arm of `constrain_types_impl` is where homomorphic
mapped-type reversal happens. Given a target `{ [K in keyof T]: Box<T[K]> }` and
a source object, the evaluator reconstructs `T` from the source's properties.
The entry is `find_keyof_inference_target(mapped.constraint, var_map)` (locating
the `keyof T` whose `T` is an inference placeholder) followed by
`constrain_reverse_mapped_type` (`constraints/reverse_mapped.rs`).

For each source property `p: V`:

```text
key_literal = literal_key_for_property_name(p.name)        // "p" or Number(1)
instantiated_template = instantiate(template, { K → key_literal })   // e.g. Box<T["p"]>
reversed = reverse_infer_through_template(V, instantiated_template, target_placeholder)
accumulate (p.name, reversed)  → object candidate for T
```

`reverse_infer_through_template` walks the template structure backwards: for a
`T[K]` position it yields `V` directly; for `Box<T[K]>` it descends into the
`Box` argument. The reconstructed object is added as a candidate at
`HomomorphicMappedType` priority — strictly worse than a direct `NakedTypeVariable`
inference, so a bare `obj: T` argument always wins over a synthetic mapped shape.
When reverse-mapping succeeds for the homomorphic param, that param is *excluded*
from the var map for the subsequent property-by-property template inference of
*other* type params, so an `any`-typed source property cannot propagate `any`
into `T` via `T[K]` and override the structural candidate.

Reverse-keyof inference (`source <: keyof T`) is the cousin: passing a string
literal `'a'` against a `keyof T` parameter synthesizes `{ a: any }` as a
contra-candidate for `T` at `LiteralKeyof` priority (the lowest "real" priority,
matching `tsc`'s `InferencePriority.LiteralKeyof`).

---

## Inference priorities

The constraint walker stamps every candidate with an `InferencePriority`
(`crates/tsz-solver/src/types.rs`). Lower numeric value = higher priority:

| Priority | Bit | Meaning |
|---|---|---|
| `NakedTypeVariable` | `1<<0` | `T` appears directly as a parameter type |
| `HomomorphicMappedType` | `1<<1` | structure-preserving mapped reversal |
| `PartialHomomorphicMappedType` | `1<<2` | partially homomorphic |
| `MappedType` | `1<<3` | generic `{ [K in keyof T]: U }` |
| `ContravariantConditional` | `1<<4` | conditional in contravariant position |
| `ReturnType` | `1<<5` | contextual type from return position |
| `LowPriority` | `1<<6` | fallback |
| `LiteralKeyof` | `1<<7` | reverse-keyof synthetic shape |
| `Circular` | `1<<8` | cycle break |

The resolution math (which priority wins, how candidates of equal priority
combine into a union vs supertype) is owned by `InferenceContext` and documented
in [solver-inference](solver-inference.md). The kernel's job is only to *stamp*
the right priority on each constraint — e.g. a generic-function argument matched
against a function-typed parameter gets `ReturnType` priority
(`arg_inference_priority` in Round 1), while a bare argument gets
`NakedTypeVariable`.

---

## Contextual signature extraction

`get_contextual_signature_for_arity_inner` (`call_evaluator.rs`) extracts a
`FunctionShape` from a contextual type so the kernel (and the checker, via
`get_contextual_signature_*_with_compat_checker`) can answer "given a lambda is
assigned to *this* type, what parameter types should its parameters have?". It is
a `TypeVisitor` (`ContextualSignatureVisitor`) with a cycle guard
(`visiting: FxHashSet<TypeId>`) so a `Lazy`-to-`Application`-to-`Lazy` loop bails
with `None` instead of spinning:

| Visited `TypeData` | Behavior |
|---|---|
| `Function` | return the shape directly |
| `Callable` | prefer call signatures; fall back to construct signatures for `new`/`super`; at a known arg count, prefer fixed-arity overloads over catch-all rest signatures |
| `Application(Base<Args>)` | resolve `Base`'s shape, build a substitution `TypeParam → Arg` via `TypeSubstitution::from_args` (auto-fills defaults), instantiate params/return/`this`/predicate |
| `Union` | drop nullable/`void`/`never` members; require all callable members' signatures to match; combine (intersect params, union returns) |
| `Intersection` | combine member shapes (`combine_function_shapes`) |
| `Lazy(DefId)` | evaluate and recurse |

`combine_contextual_signatures` is careful about overload sets: mixed-arity
overloads cannot be flattened (it would widen shorter overloads through trailing
optionals), and if **any** signature in a multi-signature set has type
parameters, it returns `None` — exactly `tsc`'s `getIntersectedSignatures`
behavior, which refuses to contextually type an arrow assigned to an overloaded
type containing both generic and non-generic signatures.

When combining param types across signatures, `any`-typed positions are dropped
if any non-`any` alternative exists (`param_types.retain(|ty| *ty != ANY)`), then
the survivors are `union_literal_reduce`'d. This is what makes
`(a: number) => string | (a: boolean) => Date` contextually present a
`(a: number & boolean) => string | Date` shape.

---

## Constructors: `resolve_new`

`resolve_new` (`constructors.rs`) is the `new`-expression analogue of
`resolve_call`, dispatching on the callee `TypeData` but using
`construct_signatures`:

| Callee | Behavior |
|---|---|
| `Function` with `is_constructor` | `resolve_function_call` |
| `Function` (plain, non-arrow) | call it; if it returns `void` → `VoidFunctionCalledWithNew` (result `any`); else `NonVoidFunctionCalledWithNew` (`TS2350`) |
| `Callable` | `resolve_callable_new` — overloaded construct signatures |
| `Union` | `resolve_union_new` — **all** members must be constructable (stricter than calls) |
| `Intersection` | `resolve_intersection_new` — intersection of instance types (mixin pattern) |
| `Application` | `evaluate_type`, retry; else recurse into the base |
| `Lazy`/`Conditional`/`IndexAccess`/`Mapped`/`TemplateLiteral`/`TypeQuery` | `checker.evaluate_type`, retry |

The `VoidFunctionCalledWithNew` vs `Success(ANY)` distinction exists so the
checker can emit `TS7009` (`noImplicitAny`) only for functions *lacking* construct
signatures, not for types that legitimately return `any` from a construct
signature. Generic constructors flow through the same
`resolve_generic_call`/`finalize` pipeline as calls; `finalize.rs` stores a
display alias (`D<unknown>`) so the formatter shows the nominal generic instead
of the expanded structural type.

---

## Spreads

The checker lowers spread/iterable arguments before handing the kernel a flat
`arg_types` slice, but it leaves *markers* the kernel must recognize.
`spread_args.rs` provides two:

- `spread_argument_marker_inner` — a single-element rest tuple named
  `__tsz_spread_argument__`, standing for an indeterminate run of `inner`-typed
  arguments (e.g. the `...boolean[]` tail of `[string, ...boolean[]]` expanded
  into a rest parameter).
- `generic_spread_argument_marker_inner` — a bare unnamed `[...T]` rest tuple
  whose inner type is a `TypeParameter`.

`check_argument_types_with` (`call_args.rs`) special-cases both: rather than
checking the marker tuple `[...boolean[]]` against the single rest *element*
type (which would wrongly reject it), it computes `remaining_rest_type_after_offset`
and compares the spread's array form against the remaining rest's array form
(`...boolean[]` as `boolean[] <: boolean[]`). When the remaining rest still
mentions a type parameter, the marker is deferred to inference rather than checked
concretely. Element-type extraction for iterable spreads is owned by
`operations/iterators.rs` (`get_iterator_info`, `extract_iterator_result_value_types`).

---

## A worked example

Consider:

```typescript
declare function map<T, U>(xs: T[], f: (x: T) => U): U[];
map([1, 2, 3], x => x.toFixed());
```

1. The checker resolves `map` to its `FunctionShape` and hands
   `resolve_call_with_checker_and_arg_sources` the callee `TypeId` and
   `arg_types = [number[], <fresh arrow>]`. `resolve_call` →
   `resolve_function_call`; `type_params` non-empty → `resolve_generic_call` →
   `resolve_generic_call_inner`.

2. **Phase 0.** Two vars: `T → __infer_0`, `U → __infer_1`. `instantiated_params`
   becomes `(xs: __infer_0[], f: (x: __infer_0) => __infer_1)`.

3. **Round 1.** `is_contextually_sensitive(number[])` is `false` → processed.
   `constrain_types(number[], __infer_0[], NakedTypeVariable)` takes the
   array-like arm, recurses element-wise to `constrain_types(number, __infer_0)`,
   and since `__infer_0` is in `var_map`, `add_candidate(T_var, number,
   NakedTypeVariable)`. The arrow is contextually sensitive (its `x` is
   unannotated) → deferred (`saw_deferred_arg = true`).

4. **Phase 2.** `fix_current_variables_with(checker.is_assignable_to)` resolves
   `T = number` and freezes it. `fixed_subst = { __infer_0 → number, T → number }`.
   `round2_params` re-instantiates `f` to `(x: number) => __infer_1`.

5. **Round 2.** The arrow is re-typed with `x: number`; the checker now types its
   body `x.toFixed()` as `string`, giving the arrow type
   `(x: number) => string`. `constrain_types((x:number)=>string,
   (x:number)=>__infer_1)` → function arm → `constrain_parameter_types(number,
   number)` (no learning) and the covariant return
   `constrain_types(string, __infer_1, ReturnType)` → `add_candidate(U_var,
   string)`.

6. **Phase 4.** `resolve_with_constraints` gives `U = string`. Neither `T` nor
   `U` has a constraint → no `TS2344`. `return_type = instantiate(U[],
   {U→string}) = string[]`. The post-inference re-check passes. Result:
   `CallResult::Success(string[])`.

The checker turns that into the call expression's type; no diagnostic.

---

## Caches and invariants

The kernel keeps several pieces of state on the `CallEvaluator` and
`InferenceContext`. Their lifetime and invalidation matter for both correctness
and the recursion guards.

| State | Owner | Scope / invalidation |
|---|---|---|
| `contextual_sensitivity_cache: RefCell<FxHashMap<TypeId, bool>>` | `CallEvaluator` | Lives for one evaluator (one call request); dropped with it. Pure function of the type graph, so never stale within a request |
| `constraint_pairs: RefCell<FxHashSet<(TypeId, TypeId)>>` | `CallEvaluator` | Cleared at the start of each `resolve_generic_call_inner`. Visited-pair set: a repeated `(source, target)` in `constrain_types` short-circuits, breaking structural cycles |
| `constraint_recursion_depth: Cell<usize>` | `CallEvaluator` | Per `constrain_types` nesting; capped at `MAX_CONSTRAINT_RECURSION_DEPTH = 100` |
| `constraint_step_count: Cell<usize>` | `CallEvaluator` | Total `constrain_types` calls per inference pass; capped at `MAX_CONSTRAINT_STEPS = 20_000` to bound pathological recursive explosions |
| `constraint_fixed_union_members: RefCell<FxHashMap<TypeId, FxHashSet<TypeId>>>` | `CallEvaluator` | Per inference pass; memoizes fixed members for target union types |
| `reverse_mapped_visited: RefCell<FxHashSet<(TypeId, TypeId)>>` | `CallEvaluator` | `(template, source)` pairs in flight; re-entry converges to the source (coinductive fixed point) |
| `reverse_alias_expansion_visited` | `CallEvaluator` | `(alias_base, source)` pairs; cycle key for recursive alias expansions where the template `TypeId` changes each level |
| `reverse_mapped_depth: Cell<u32>` | `CallEvaluator` | Hard cap `REVERSE_MAPPED_DEPTH_CAP = 64` as a safety net for pathological inputs |
| `current_call_inference_placeholders` / `shared_inference_placeholders` | `CallEvaluator` | Cleared each generic call; populated only when an argument is a generic function (TS-3.4 re-generalization gate) |
| `subtype_cache` / `active_subtype_checks` | `InferenceContext` | BCT and bound-validation memo + coinductive cycle break; per context |
| `infer_visited` / `infer_depth` | `InferenceContext` | `(source, target)` cycle break + `MAX_INFER_DEPTH = 20` for `infer_from_types` |
| `app_expansion_depth` | `InferenceContext` | `MAX_APP_EXPANSION_DEPTH = 5` for recursive type-alias targets |

Invariants:

- **Side channels reset per call.** `last_instantiated_predicate` and
  `last_instantiated_params` are cleared at the top of `resolve_call`, so a
  non-generic call after a generic one cannot read a stale predicate.
- **Placeholder names are deterministic in the checker.** Through
  `next_inference_placeholder_id`, the adapter uses a per-file counter so any
  `__infer_*` name that reaches a diagnostic is stable across parallel checks and
  repeated runs.
- **The evaluator never holds AST.** `CallEvaluator` takes `TypeId`s in and
  returns a structured `CallResult` out — no spans, no node ids, no formatted
  text. The mapping from `ArgumentTypeMismatch`/`NoOverloadMatch` to `TS2345`/
  `TS2769` is the checker's job
  ([checker-error-reporter-diagnostics](checker-error-reporter-diagnostics.md)).

---

## Edge cases and `tsc` parity

These are the behaviors most likely to bite if you change the kernel; each is a
deliberate match to `tsc`.

- **`this` errors come after argument errors.** `resolve_function_call` computes
  the `this` mismatch early but withholds it until arguments validate, so `TS2345`
  precedes `TS2684`. A `this: void` declaration opts out of the receiver check
  entirely.

- **Identical overload failures collapse.** When every count-compatible overload
  fails on the *same* argument index/expected/actual, `resolve_callable_call`
  reports `TS2345`, not `TS2769`. The same applies to identical `this`-type
  mismatches.

- **First-wins for repeated naked type params.** `g<T>(a: T, b: T)` with
  `("", 3)` reports the second argument; a later context-sensitive callback cannot
  merge `""` and `3` into a union. The nullable exception preserves
  `getCommonSupertype` semantics for `("a", "b" | undefined)`.

- **Literal preservation across the round boundary.** A type parameter at top
  level of the return type and inferred only from top-level positions does not
  widen its fresh literal candidates (`mark_top_level_in_return_type_unfixed`),
  so conditional/`Exclude` parameters reduce correctly and forbidden arguments
  hit a `never` parameter.

- **Defaults are fallbacks, not constraints.** An argument mismatch against a
  parameter whose type uses a *defaulted* type parameter is suppressed
  (`CallResult::Success`); a *constraint*-fallback variable is set to its
  constraint precisely to make the argument re-check emit `TS2345`.

- **`NoInfer<T>` blocks inference.** The walker looks through `NoInfer` wrappers
  on the source but refuses to recurse into a `NoInfer` *target*, so the source
  contributes no candidates for the wrapped type parameter.

- **Union construct strictness.** `new` on a union requires *every* member to be
  constructable; `new` on a non-arrow function is allowed but yields `any` only
  when the function returns `void` (`TS2350` otherwise).

- **Contextual signature refusal for mixed overloads.** A multi-signature
  contextual type with any generic signature yields no contextual signature,
  matching `tsc`'s `getIntersectedSignatures`, so an arrow assigned to such a
  type is not contextually typed.

- **Reverse-mapped priority ordering.** Homomorphic reversal is
  `HomomorphicMappedType` priority and reverse-keyof is `LiteralKeyof` — both
  strictly worse than `NakedTypeVariable`, so a direct `obj: T` argument always
  beats a synthetic key/template-derived shape.

---

## Where to go next

- The candidate-resolution math (priority filtering, BCT, contra resolution,
  widening, fixing, strengthening): [solver-inference](solver-inference.md).
- Applying the inferred substitution to produce the return type:
  [solver-instantiation](solver-instantiation.md).
- The subtype kernel behind the adapter's `is_assignable_to`:
  [solver-relations](solver-relations.md).
- Conditional-type `infer T` matching (a *different* mechanism from call
  inference): [solver-evaluation](solver-evaluation.md).
- The checker orchestration that picks the callee `TypeId`, expands spreads, and
  selects overloads: [checker-calls-signatures-generics](checker-calls-signatures-generics.md).
- The contextual-typing and reverse-inference companion on the boundary:
  [solver-contextual-typing-and-reverse-inference](solver-contextual-typing-and-reverse-inference.md).
- Turning a `CallResult` into a diagnostic:
  [checker-error-reporter-diagnostics](checker-error-reporter-diagnostics.md).
- The big-picture pass order: [end-to-end-timeline](end-to-end-timeline.md).
