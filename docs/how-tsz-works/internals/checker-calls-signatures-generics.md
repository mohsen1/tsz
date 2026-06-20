# Call Resolution, Overloads, and Generic Checking on the Checker Side

A call expression like `f(a, b)` looks deceptively simple, but it is one of the
most orchestration-heavy operations in the checker. The checker must resolve the
callee, expand spread and tuple arguments, set up contextual typing so callback
arguments get sensible parameter types, validate explicit type arguments,
collect argument types (sometimes twice, in two inference rounds), pick which
overload signature applies, drive solver inference for unbound type parameters,
and finally turn the solver's structured `CallResult` into either a return type
or a diagnostic (`TS2554`, `TS2345`, `TS2769`, `TS2344`, and friends). Crucially,
none of the *semantic* work — relation checks, inference, instantiation — runs in
the checker. The checker is the conductor; the solver is the orchestra.

This document traces the real code path that runs when a `CallExpression` or
`NewExpression` is checked, names the functions involved, and explains where
each responsibility lives. It is a sibling to
[checker-context-and-state](checker-context-and-state.md),
[checker-flow-and-narrowing](checker-flow-and-narrowing.md),
[checker-assignability-gateway](checker-assignability-gateway.md), and the
solver-side docs [solver-relations](solver-relations.md),
[solver-inference](solver-inference.md), and
[solver-instantiation](solver-instantiation.md).

## Owns / Must not own

The line between checker orchestration and solver semantics is the single most
important thing to keep straight when reading this subsystem.

**The checker owns:**

- Resolving the callee expression to a `TypeId` (identifier/property/element
  access, `super`, optional chains, IIFE wrappers, `require`/dynamic `import`).
- Deciding *which* `TypeId` to feed the solver — applying explicit type
  arguments, resolving `Lazy(DefId)`/`Application` callee forms to concrete
  callable shapes, splitting nullish members for optional-chain calls.
- Argument collection: spread/tuple expansion, contextual typing of each
  argument, two-pass (Round 1 / Round 2) inference scheduling for generic calls,
  literal preservation policy.
- Overload *selection orchestration*: looping over candidate signatures, running
  tsc's two-pass `chooseOverload` (subtype pass then assignable pass),
  speculative diagnostic snapshot/rollback, contextual retries.
- Explicit type-argument count and constraint validation (`TS2558`, `TS2344`,
  `TS2743`).
- Turning a `CallResult` into diagnostics and source spans.

**The checker must not own:**

- The relation kernel that decides whether an argument type is assignable to a
  parameter type. That is `is_assignable_to` / `is_assignable_to_strict` /
  `is_assignable_to_bivariant_callback`, implemented in the solver and reached
  only through the `CheckerCallAssignabilityAdapter`.
- Type-parameter inference (`constrain_types`, reverse mapped inference,
  union/intersection candidate fixing). That is `CallEvaluator` in the solver.
- Instantiation of signatures with inferred type arguments.
- Constructing raw `TypeKey`, pattern-matching solver internals, or reading
  printer output as a predicate.

The whole subsystem is therefore best read as: *the checker prepares inputs,
hands them across a query boundary, and interprets the structured result.*

## Module map

| Path | Role |
| --- | --- |
| `crates/tsz-checker/src/types/computation/call/mod.rs` | Call-computation facade; `get_type_of_call_expression`, predicate storage, the `CallFinalizationCtx` glue. |
| `crates/tsz-checker/src/types/computation/call/inner.rs` | `get_type_of_call_expression_inner` — the master orchestration function; callee resolution, early returns, classification, dispatch. |
| `crates/tsz-checker/src/types/computation/call/inner/argument_collection.rs` | `collect_call_arguments_for_dispatch` — the two-pass (Round 1/Round 2) generic argument collector. |
| `crates/tsz-checker/src/types/computation/call/callee_context.rs` | Callee-shape refresh and contextual setup helpers. |
| `crates/tsz-checker/src/types/computation/call/return_context.rs` | Return-context substitution wiring. |
| `crates/tsz-checker/src/checkers/call_checker/mod.rs` | `CallableContext`, `OverloadResolution`, and the `CheckerCallAssignabilityAdapter` that implements the solver's `AssignabilityChecker` trait. |
| `crates/tsz-checker/src/checkers/call_checker/applicability.rs` | Thin adapters: `resolve_call_with_checker_adapter`, `resolve_new_with_checker_adapter`, the subtype-pass variant, and spread normalization. |
| `crates/tsz-checker/src/checkers/call_checker/candidate_collection.rs` | `collect_call_argument_types_with_context` — single-pass argument collection with spread expansion and excess-property checks. |
| `crates/tsz-checker/src/checkers/call_checker/spread_arity.rs` | `spread_callee_infers_params_from_arguments` — `TS2556` suppression for open-ended tuple spreads. |
| `crates/tsz-checker/src/checkers/call_checker/overload_resolution.rs` | `resolve_overloaded_call_with_signatures` — the full overload loop. |
| `crates/tsz-checker/src/checkers/call_checker/overload_resolution/contextual_retry.rs` | Per-candidate contextual-refresh retry after an argument mismatch. |
| `crates/tsz-checker/src/checkers/call_checker/overload_resolution/return_context.rs` | Return-context refinement for argument inference. |
| `crates/tsz-checker/src/checkers/call_checker/diagnostics.rs` | Speculative diagnostic snapshot/rollback, callback-body error detection, overload-failure pruning. |
| `crates/tsz-checker/src/types/computation/call_result.rs` | `handle_call_result` — maps `CallResult` variants to diagnostics/return types. |
| `crates/tsz-checker/src/checkers/generic_checker/mod.rs` | `validate_call_type_arguments` and `CallTypeArgumentValidation` (`TS2558`/`TS2743`). |
| `crates/tsz-checker/src/checkers/generic_checker/constraint_validation.rs` | `validate_type_args_against_params` — `TS2344` constraint checking. |
| `crates/tsz-checker/src/query_boundaries/checkers/call.rs` | The query boundary: `resolve_call`, `resolve_new`, `get_overload_call_signatures`, `get_contextual_signature_for_arity`, etc. |

The two heavyweight files are `overload_resolution.rs` and
`candidate_collection.rs`. Both sit near the repo's 2000-line cap and host the
densest logic.

## The query boundary

Everything semantic flows through one trait. The solver defines
`AssignabilityChecker` (see
`crates/tsz-solver/src/operations/core/call_evaluator.rs`, `pub trait
AssignabilityChecker`) with methods such as `is_assignable_to`,
`is_assignable_to_strict`, `is_assignable_to_bivariant_callback`,
`evaluate_type`, `are_types_identical`, and `normalize_inferred_type`. The
solver's `CallEvaluator` calls these as it walks signatures and constrains type
parameters, but it does not know how the checker answers them.

The checker supplies the implementation in `CheckerCallAssignabilityAdapter`
(`call_checker/mod.rs`). Each relation method routes to a `CheckerState`
relation-outcome query — for example `is_assignable_to` calls
`call_adapter_compatibility_relation_outcome`, and the strict variant calls
`strict_relation_outcome`. The adapter carries one extra flag,
`overload_subtype_pass`, which when set routes through the *subtype-pass*
relation entries instead (more on that in the overload section).

The checker reaches the solver through small wrappers in `applicability.rs`:

```
collect_call_arguments_for_dispatch / overload loop
        |
        v
resolve_call_with_checker_adapter(func_type, arg_types, ...)   <- applicability.rs
        |  builds CheckerCallAssignabilityAdapter { state, overload_subtype_pass: false }
        v
query_boundaries::checkers::call::resolve_call(...)            <- call.rs
        |
        v
tsz_solver::operations::resolve_call_with_checker(...)        <- solver owns inference
        |
        v
returns CallWithCheckerResult = (CallResult, Option<predicate>, Option<instantiated params>)
```

`CallWithCheckerResult` is a triple
(`crates/tsz-solver/src/operations/core/call_evaluator.rs`): the `CallResult`
itself, an optional instantiated `TypePredicate` (for narrowing — the checker
stores this so `obj is Foo`-style guards work after the call), and the optional
*instantiated* parameter types, which the checker uses for post-inference excess
property checks against concrete (not pre-inference) parameter types.

Before any solver call the checker calls `ensure_callee_relation_inputs_ready`
and `ensure_relation_inputs_ready`
(`crates/tsz-checker/src/assignability/assignability_checker.rs`) to make sure
every `Lazy(DefId)` reference reachable from the callee/argument types is
resolved in the `TypeEnvironment` — the solver's relation kernel cannot resolve
`DefId`s itself, so stale lazy refs must be primed on the checker side first.

## CallResult: the structured answer

The solver never emits a diagnostic. It returns a `CallResult`
(`call_evaluator.rs`, `pub enum CallResult`) and the checker decides what to do:

| `CallResult` variant | Meaning | Checker reaction (`handle_call_result`) |
| --- | --- | --- |
| `Success(TypeId)` | Call resolved; payload is the return type. | Finalize and return it. |
| `NotCallable { type_id }` | Callee has no call/construct signatures. | `TS2349` (or `TS2348` "did you mean `new`?" via `error_class_constructor_without_new_at`, `TS2346` for `super()`). |
| `ThisTypeMismatch { .. }` | The receiver does not satisfy the signature's `this` type. | `TS2684`. |
| `ArgumentCountMismatch { expected_min, expected_max, actual }` | Wrong arity. | `TS2554`/`TS2555` (and `TS2556` for indeterminate spreads). |
| `OverloadArgumentCountMismatch { actual, expected_low, expected_high }` | Arity falls in a gap between overload arities. | `TS2575` (`NO_OVERLOAD_EXPECTS_ARGUMENTS...`). |
| `ArgumentTypeMismatch { index, expected, actual, fallback_return }` | Argument `index` is not assignable. | `TS2345` via the assignability gateway; `fallback_return` keeps downstream checking alive. |
| `NonVoidFunctionCalledWithNew` / `VoidFunctionCalledWithNew` | `new` on a non-constructor. | Routed through `error_non_void_function_called_with_new_at` (a no-op in tsz — the old `TS2350` was removed in tsc 6.0); `new` on an `any`-returning target still feeds the `TS7009` `noImplicitAny` path. |
| `NoOverloadMatch { func_type, arg_types, failures, fallback_return }` | No overload accepted these arguments. | `TS2769` with the per-overload `failures` as related information. |

Because `ArgumentTypeMismatch` carries a `fallback_return`, even a rejected call
yields a usable downstream type, so `f(badArg).someMethod` still reports a
sensible secondary diagnostic instead of cascading `any`/`error` noise. That is
deliberate tsc-parity behavior baked into the variant shape.

## Walk-through: `get_type_of_call_expression_inner`

The master function is
`CheckerState::get_type_of_call_expression_inner(idx, request)`
(`call/inner.rs`). Dispatch reaches it through
`get_type_of_call_expression_with_request`, which `dispatch/mod.rs` routes to on
`CALL_EXPRESSION` (and `get_type_of_new_expression_with_request` for
`NEW_EXPRESSION`, in `types/computation/complex.rs`). The flow, in order:

**1. Special callees first.** `require("...")` resolved against a CommonJS module
returns the module's value type directly (`commonjs_module_value_type`), and a
dynamic `import(...)` short-circuits through `check_and_resolve_dynamic_import`
(emitting `TS2307` on a missing module). `super(...)` is flagged via
`is_super_expression` and later resolved against *construct* signatures.

**2. Callee typing with contextual setup.** If the call is in a contextual
position (e.g. it is an IIFE, or a higher-order callee), the checker wraps the
contextual type into a synthetic callable so the callee resolver can read the
expected return type — `setup_iife_contextual_type` and
`setup_higher_order_callee_contextual_type`. The callee type is then computed,
with a fast path for plain local function-declaration identifiers
(`is_fast_path_function_decl`) that skips identifier-side diagnostic probes.

Note that `error_non_void_function_called_with_new_at`
(`error_reporter/call_errors/error_emission.rs`) is intentionally a no-op: the
historical `TS2350` ("Only a void function can be called with the 'new' keyword")
was removed in tsc 6.0, so tsz keeps the `CallResult` variant for control flow
but emits nothing there, preserving parity.

**3. Early returns for degenerate callees.** A series of guards handle
`callee_type == TypeId::ANY` (untyped call — arguments still checked for `TS7006`
on callbacks and `TS2454` definite-assignment, but type arguments on an `any`
callee trigger `TS2347`), `callee_type == TypeId::ERROR` (cascading; arguments
still walked), and `unknown`/`never` callees via
`check_callee_unknown_or_never`. Even on these failure paths the checker calls
`collect_call_argument_types_with_context` so argument-side diagnostics are not
lost.

**4. Optional-chain nullish splitting.** When the callee is an optional chain
(`?.()`, or a continuation like `o?.a.b()`), the checker evaluates the callee
type and calls `split_nullish_type`; if the whole thing was nullish the call
returns `undefined`, otherwise the non-nullish part becomes the callee and the
final result is widened with `undefined`.

**5. Explicit type-argument validation.** If the call has explicit type
arguments (`f<number>(x)`), `validate_call_type_arguments` runs *before*
argument checking and returns a `CallTypeArgumentValidation { count_mismatch,
constraint_violation }`. If either flag is set the checker stops type-checking
arguments against the wrongly-instantiated signature (it still walks them for
side-effect diagnostics) and returns a recovered return type. This is the parity
rule that tsc reports the type-argument problem and *suppresses* cascading
`TS2345` for that call.

**6. Apply explicit type arguments.** When the type arguments are valid,
`apply_type_arguments_to_callable_type` substitutes them into the callee
(`fn<T>(x: T)` called as `fn<number>("s")` becomes `(x: number) => ...`, so the
string argument can be checked against `number`).

**7. Classification.** The callee is resolved through `evaluate_application_type`
and `resolve_lazy_type`, then classified by
`query::classify_for_call_signatures` into the solver enum `CallSignaturesKind`
(`crates/tsz-solver/src/type_queries/extended.rs`):

- `Callable(CallableShapeId)` — a callable with one or more signatures.
- `MultipleSignatures(Vec<CallSignature>)` — e.g. a single `Function` shape, or
  signatures gathered from a union/intersection of callables.
- `NoSignatures` — not callable.

If classification yields `NoSignatures` but the callee identifier has an explicit
annotation or a direct function type, the checker retries classification against
that type (`explicit_identifier_callee_annotation_type`,
`direct_function_call_type_for_type_argument_validation`).

**8. Overload detection.** Overloads are then collected — but **not** for union
callees. A `(F1 | F2)("a")` call has *union call* semantics (valid only if the
call works for *all* members, handled inside the solver's union-call path),
whereas overload resolution accepts a call if *any one* signature matches.
Mixing the two would silently lose `TS2554`. So when the callee is a union
(`common::is_union_type`), `overload_signatures` is forced to `None`; otherwise
`call_checker::get_overload_call_signatures` (Callable case) or the multi-signature
list (when `len() > 1`) supplies the candidate list.

**9. The branch.** If there are overload signatures, the checker calls
`resolve_overloaded_call_with_signatures` (the overload loop, below) and, on a
result, stores any selected type predicate and calls `handle_call_result`.
Otherwise it proceeds to single-signature contextual setup and the two-pass
argument collector.

## Argument collection and contextual typing

Before solver dispatch the checker must know each argument's `TypeId`. Two
collectors exist.

### Single-pass: `collect_call_argument_types_with_context`

`candidate_collection.rs`'s
`collect_call_argument_types_with_context(args, expected_for_index,
check_excess_properties, skip_sensitive_indices, callable_ctx)` is the workhorse.
It is generic over a closure `expected_for_index(i, arg_count) -> Option<TypeId>`
that supplies the contextual (expected) type for argument position `i`. Its
responsibilities:

- **Spread/tuple expansion.** A `...t` argument where `t` is a tuple expands to
  multiple positional arguments. The collector first counts the *expanded* arity
  (`expanded_count`) — `const`-asserted array literals, fixed-length tuple
  constraints, and array-literal spreads all expand; a type-parameter spread
  whose constraint is a *variadic* tuple (`...v` with `v: A extends readonly [L,
  ...L[]]`) is **kept whole** as a single `[...A]` marker via
  `type_param_variadic_tuple_spread`, because destructuring through the
  constraint would lose `A`'s identity and break downstream rest-parameter
  inference. Spread element types are normalized through
  `normalized_spread_argument_type` (`applicability.rs`).
- **Iterability and depth guards.** A non-iterable spread emits `TS2488`
  (`check_spread_iterability`). A recursive mapped tuple spread that may exceed
  the instantiation depth emits `TS2589` once
  (`recursive_mapped_tuple_spread_may_exceed_depth_in_types`,
  `emit_ts2589_spread_instantiation_depth`) and then recovers the remaining
  arguments as `any`.
- **Sensitive-argument placeholders.** When `skip_sensitive_indices` is set
  (Round 1 of generic inference, below), each skipped slot pushes a synthetic
  one-parameter `Function` placeholder. The single parameter matters: a
  zero-parameter placeholder would make the solver's `is_contextually_sensitive`
  return `false` and wrongly include the slot in inference.
- **Excess-property checks** on object-literal arguments when
  `check_excess_properties` is true and the slot is not generic-skip-protected.
- **Indeterminate-spread arity** errors: `TS2556` is reported on the first
  non-tuple spread only (`emitted_ts2556` flag), with the suppression logic from
  `spread_callee_infers_params_from_arguments` for inline callees that
  contextually type the rest-position parameter from the spread.

The `CallableContext` (`call_checker/mod.rs`) threads the callable type through
the collector explicitly rather than via ambient mutable state, so rest-position
(`TS2556`) and generic excess-skip decisions can query the callee shape.

### Two-pass: `collect_call_arguments_for_dispatch`

Generic calls need a different strategy because a callback argument's parameter
types depend on type parameters that are inferred *from* the arguments. The
checker resolves this chicken-and-egg with a two-round scheme in
`call/inner/argument_collection.rs`,
`collect_call_arguments_for_dispatch(inputs)`, returning a
`CollectedCallArguments` bundle. The decision to use two passes is gated by
`is_generic_call` (the callee shape has non-empty `type_params` and there are no
explicit type arguments) **and** `is_contextually_sensitive` returning `true` for
at least one argument.

`is_contextually_sensitive` (`types/computation/contextual.rs`) is the deferral
predicate: an arrow/function with any un-annotated parameter is sensitive; a
`function` expression that references `this` is sensitive; object/array literals
are sensitive if any element is; parentheses and conditionals pass through.

```
needs_two_pass = any(is_contextually_sensitive(arg))
        |
   no   |   yes
        |
        v
  Round 1: collect non-sensitive args (sensitive slots get a
           Function placeholder via skip_sensitive_indices).
           Run inference against an env-evaluated FunctionShape:
           compute_contextual_types_with_context -> TypeSubstitution.
           Seed extra bindings from non-sensitive object-literal
           properties; iterate a small fixpoint (capped at
           shape.type_params.len()) to refine partial-object inference.
        |
        v
  Push ThisType<T> (instantiated with the Round 1 substitution) onto
  this_type_stack so object-literal methods resolve `this` correctly.
        |
        v
  Round 2: recollect ALL args, now with parameter types instantiated
           from the Round 1 substitution, so callback parameters get
           concrete contextual types instead of bare type parameters.
```

Two subtle pieces of state cross from Round 2 back to the caller:
`checker_round2_substitution` and `checker_round2_shape`. When the checker's
intra-expression Round 2 pins type parameters that the solver's *single-pass*
`resolve_call` cannot recover (the canonical case is a homomorphic mapped +
`infer` return position whose reverse inference fails), the caller uses these to
refine the solver's `instantiated_params` so the post-call assignability recheck
sees the tighter expected types
(`refine_instantiated_params_with_checker_substitution`). There is also
`direct_literal_conflict_substitution` for resolving bare-type-parameter literal
conflicts.

### Contextual parameter types

Per-argument contextual types come from
`contextual_parameter_type_for_call_with_env_from_expected`, falling back to
`ContextualTypeContext::get_parameter_type_for_call`
(`query_boundaries::common`). There is one important parity filter: a bare *free*
type parameter from an *enclosing* generic signature must **not** contextually
type an argument. For `set((p) => ...)` where `set: (p: T) => void` and `T` is
free in an enclosing `<T>(...)`, tsc does not flow `T` into the callback, so the
callback's own parameters stay implicitly `any` (`TS7006`) and the argument is
checked against `T` directly (`TS2345`). The filter `is_generic_call ||
!is_type_parameter_type(...)` enforces this — a genuine generic callee keeps its
seed (because its own type parameters are what we are inferring), but a
non-generic callee does not.

## Generic inference dispatch

Once arguments are collected, the checker resolves the callee to a concrete
callable (`evaluate_application_type` → `resolve_lazy_type` →
`resolve_lazy_members_in_union`, plus `replace_function_type_for_call` so the
global `Function` type stays callable), then dispatches:

- For generic calls, the argument types are first run through
  `sanitize_generic_inference_arg_types`
  (`types/computation/call_inference.rs`), which scrubs provisional inference
  artifacts that should not seed type parameters.
- Per-argument source markers are computed:
  `call_arg_source_type_annotation_markers` flags arguments whose type came from
  an annotation/assertion (`as`, `satisfies`, `as const`, a typed identifier),
  and `call_arg_source_readonly_annotation_markers` flags readonly-array
  annotations. These tell inference *not* to re-widen those literals (e.g.
  `Object.fromEntries`'s `T` must not widen `1 → number` just because the call
  is generic). When any marker is set the checker uses
  `resolve_call_with_checker_adapter_and_arg_sources`; otherwise the plain
  `resolve_call_with_checker_adapter`. `super(...)` uses
  `resolve_new_with_checker_adapter` against construct signatures.

The solver's `CallEvaluator` does the actual inference: it walks the signature,
calls back into the adapter's relation methods, runs `constrain_types` (capped by
`MAX_CONSTRAINT_RECURSION_DEPTH = 100` and `MAX_CONSTRAINT_STEPS = 20_000` in
`call_evaluator.rs`), fixes union/intersection candidates, and on success
publishes `last_instantiated_predicate` and `last_instantiated_params`. See
[solver-inference](solver-inference.md) for that kernel.

The contextual return type is forwarded to the solver
(`call_resolution_contextual_type`) for *return-context seeding*: higher-order
patterns like `map(xs, identity)`, `compose(list, box)`, or
`consumeClass(createClass(x => ...))` need the contextual result to instantiate
parameter and callback types in the final solve step. `any`/`unknown` contextual
return types are filtered out first because they carry no information and would
poison inference (`T & ((arg: string) => any)` collapsing to `any`).

## Overload resolution

`resolve_overloaded_call_with_signatures(args, signatures,
force_bivariant_callbacks, contextual_type, actual_this_type)`
(`overload_resolution.rs`) is the largest single function in the subsystem. It
mirrors tsc's `chooseOverload` and runs roughly as follows.

**Union-contextual baseline.** First it synthesizes a *union* of all overload
signatures' function types (`union_or_single`) and uses it as the contextual type
for one shared argument-collection pass. Literal preservation is forced on
(`preserve_literal_types = true`) so a `"canvas"` argument stays the literal
`"canvas"` instead of widening to `string` — otherwise the union contextual type
(which collapses `"canvas" | string → string`) would make every literal overload
fail to match.

**Speculative state snapshot.** Overload resolution is *speculative*: trying a
candidate may emit callback-body diagnostics that must vanish if the candidate
loses. The checker snapshots full state with `snapshot_overload_retry_state`
(`ctx.snapshot_full()`) and restores with `rollback_overload_retry_state` on
failure. `node_types` is handled with an *overlay* so speculative collection can
still read previously cached expression types (flow narrowing of an argument like
`obj[k]` after `obj[k] = rhs` must see the cached `rhs`), while its own writes
stay isolated in the overlay layer.

**Two-pass `chooseOverload` (tsc parity).** For multi-signature lists
(`overload_two_pass = signatures.len() > 1`), each candidate is tried first under
the **subtype relation** (`resolve_call_with_checker_adapter_subtype_pass`, which
sets `overload_subtype_pass = true` on the adapter). In the subtype pass an `any`
argument is *not* related to concrete parameter types at any nesting level. If a
candidate succeeds only under the looser **assignable relation**, it is *not*
accepted immediately — it is stashed in `assignable_pass_fallback`, because a
later candidate may still pass the strict subtype pass (e.g. a generic overload
instantiated with `U = any`). This preserves declaration order exactly as tsc
does.

```
for each candidate signature (in declaration order):
    subtype pass (any-not-related)?  --success--> select
            |
          fail (rollback speculative state)
            |
            v
    assignable pass --success--> stash as assignable_pass_fallback, keep going
            |
          fail
            v
    record best ArgumentTypeMismatch for diagnostics, continue
---- after loop ----
no subtype winner? -> accept the first assignable_pass_fallback
still nothing?      -> NoOverloadMatch -> TS2769 with per-candidate failures
```

**Per-candidate refinement.** Each candidate may be retried. After a first-pass
`ArgumentTypeMismatch`, `retry_overload_after_contextual_refresh_mismatch`
(`overload_resolution/contextual_retry.rs`) recomputes a return-context
substitution and re-runs the candidate so callback bodies are evaluated with
per-overload parameter types. Generic overloads with contextual refresh
arguments do an *instantiated retry*: the checker rolls back to the overload
snapshot, refreshes all argument nodes (`refresh_all_args`), composes Round-1's
argument-driven substitution with the return-context substitution
(`extract_arg_inference_substitution` + `merge_return_context_substitution`), and
re-collects arguments under the instantiated parameter types. This is what lets
e.g. a Vue Options-API `ThisType<Data & Readonly<Props> & Instance>` resolve
`this` to concrete members instead of leaving `Data`/`Props` unresolved and
emitting false `TS2339`.

**Callback-body deferral.** A union-context success can still leave inline
callbacks typed under a lossy union, hiding per-signature body errors. The
checker speculatively re-types callbacks against the *selected* signature
(`overload_candidate_has_callback_body_errors`,
`selected_overload_callback_body_has_errors`,
`current_block_body_callback_return_mismatch_arg`) and, if the selected signature
has body errors, *defers* (prunes those diagnostics and `continue`s to the next
candidate). `no_rcs_fallback` defers a non-generic overload whose return type is
`any`-tainted so a later generic overload with return-context inference can win
(e.g. preferring a generic `reduce<U>` over a non-generic overload that returns
`(a: any) => any`).

**Failure path.** If no candidate matches, the solver-side `NoOverloadMatch`
carries `failures: Vec<PendingDiagnostic>`; `handle_call_result` renders `TS2769`
("No overload matches this call") with each candidate's failure as related
information. `diagnostics.rs` owns the filtering that decides which speculative
failures survive (`callback_body_no_overload_diagnostics_since`,
`prune_callback_body_diagnostics`,
`diagnostics_for_overload_mismatch_argument_between`).

## `this`-arguments

For a method call `o.m(...)`, the receiver `o` becomes the `this` argument. In
`inner.rs` the checker unwraps the callee
(`skip_parenthesized_and_assertions`) and, if the unwrapped form is a property or
element access, computes `actual_this_type` from `access.expression` (splitting
nullish members when the call is an optional chain). That `actual_this_type` is
threaded into `resolve_call_with_checker_adapter` and the overload loop, and the
solver compares it against each signature's `this_type`, returning
`ThisTypeMismatch` (→ `TS2684`) on failure.

`force_bivariant_callbacks` is also derived here: it is set when the unwrapped
callee is a property/element access, matching tsc's method-position bivariance
for callback parameters. The `this` *keyword expression* itself (not the
receiver) is typed elsewhere — `dispatch/this.rs`'s `dispatch_this_keyword`
handles `this` lookups and its own diagnostics (`TS2465`, `TS2332`).

## Explicit type-argument validation

`f<A, B>(x)` validation lives in the generic checker, separate from inference.
`validate_call_type_arguments(callee_type, type_args_list, call_idx)`
(`generic_checker/mod.rs`) runs before argument checking:

- It resolves the callee through `evaluate_application_type`/`resolve_lazy_type`
  so the classifier can see signatures, then extracts the matching signature's
  type parameters via `query::extract_type_params_for_call` (which handles
  overload arity matching).
- **Count checks.** `min_required` excludes type parameters with defaults;
  `max_expected` is the total. A count outside `[min_required, max_expected]`
  emits `TS2558` ("Expected N type arguments, but got M"). When the callee has
  overloads expecting two different type-parameter counts, it instead emits
  `TS2743` ("No overload expects N type arguments, but overloads do exist that
  expect either A or B") via `query::overload_type_param_counts`.
- **Untyped callee.** Type arguments on a callee that resolves to `any` emit
  `TS2347` ("Untyped function calls may not accept type arguments"), but the
  arguments are still resolved so identifiers stay referenced for
  `noUnusedLocals`.
- **Constraint checks.** When counts are fine, `validate_type_args_against_params`
  (`generic_checker/constraint_validation.rs`) checks each argument against its
  type parameter's constraint and emits `TS2344` ("Type X does not satisfy the
  constraint Y"). This is the largest file in the generic checker; it handles
  deferred constraints, indexed-access constraints, `infer`-position constraints,
  recursive heritage constraints, and the many parity carve-outs tsc applies to
  avoid eager false positives.

The validation result, `CallTypeArgumentValidation { count_mismatch,
constraint_violation }`, gates whether the call proceeds (step 5 of the
walk-through). The constraint *display* and error-anchor helpers live in
`constraint_display_helpers.rs` and `type_arg_error_helpers.rs`.

## A small concrete trace

Consider:

```ts
function pick<T, K extends keyof T>(obj: T, key: K): T[K] { ... }
declare const o: { a: number; b: string };
const x = pick(o, "a");
```

1. `dispatch/mod.rs` routes the `CALL_EXPRESSION` to
   `get_type_of_call_expression_with_request` →
   `get_type_of_call_expression_inner`.
2. The callee `pick` is an identifier resolving to a generic `Function` shape via
   the fast path (`is_fast_path_function_decl` → `get_type_of_symbol`).
3. No explicit type arguments, so step 5/6 are skipped.
   `classify_for_call_signatures` returns `MultipleSignatures` with one element →
   no overload list (`len() <= 1`).
4. `get_contextual_signature_for_arity` yields the callee shape; `type_params`
   is `[T, K]` and there are no explicit type args, so `is_generic_call = true`.
5. Neither `o` nor `"a"` is contextually sensitive (`is_contextually_sensitive`
   is `false` for an identifier and a string literal), so `needs_two_pass =
   false`; arguments collect in one pass. `preserve_literal_types` is forced on
   because there is a literal argument, so `"a"` stays the literal `"a"`.
6. `arg_source_type_annotation_markers` flags `o` (typed identifier) so its
   members are not re-widened. The checker calls
   `resolve_call_with_checker_adapter_and_arg_sources` with
   `arg_types = [typeof o, "a"]`.
7. The solver's `CallEvaluator` infers `T = { a: number; b: string }` from `obj:
   T` and `K = "a"` from `key: K` (with `K extends keyof T` satisfied), then
   computes the return `T[K] = number`. It returns `CallResult::Success(number)`
   plus `last_instantiated_params`.
8. `handle_call_result` finalizes and `x` is `number`.

If the call were `pick(o, "c")`, the solver would infer `K` against `keyof T =
"a" | "b"`, find `"c"` not assignable, and return
`ArgumentTypeMismatch { index: 1, expected: "a" | "b", actual: "c",
fallback_return: ... }`, which the assignability gateway renders as `TS2345`. The
`fallback_return` still lets `x` have a type so later uses do not cascade.

## Caches and invariants

- **Speculative diagnostics.** Overload and contextual-retry attempts run under
  `snapshot_full` / `rollback_full` (the `FullSnapshot` machinery in
  `context::speculation`). The snapshot captures diagnostics, the
  emitted-diagnostic dedup set, `TS2454` definite-assignment dedup state, the
  `TS2307` module dedup, and the implicit-any-checked-closures set. The
  invariant is that *no* diagnostic from a losing candidate survives, while
  diagnostics from the winning candidate are kept via the transaction API.
- **`node_types` overlay.** During overload resolution the caller's
  `node_types` cache is *overlaid* (`node_types.overlay()`), never replaced with
  an empty map. The pristine `original_node_types` is held so each restore site
  rebuilds as "restore the caller's entries, then layer the winning signature's
  entries on top." Replacing it with an empty map (an old bug) silently dropped
  every cached expression type the caller and sibling statements had computed.
  The clone is `Arc`-cheap (copy-on-write).
- **Contextual resolution cache.** Cleared once before the overload loop
  (`clear_contextual_resolution_cache`) rather than per-argument; it is empty
  after the first iteration so per-argument clearing was redundant.
- **`generic_excess_skip`.** Set during argument collection to mark parameter
  positions whose original type was a bare type parameter (so excess-property
  checks are skipped — `T` captures the full object shape). It is intentionally
  *not* restored at the end of argument collection; it must survive through the
  recovery paths and `handle_call_result`, and is restored only right before the
  final call-result handling.
- **`this_type_stack`.** A `ThisType<T>` marker pushed during Round 1 (or from
  the callee shape) must stay on the stack through `handle_call_result`; popping
  it early makes post-inference rechecks fall back to the wrong contextual type
  and emit false `TS2339`. The push is tracked by `pushed_this_type_from_shape`.
- **Relation-input readiness.** `ensure_callee_relation_inputs_ready` and
  `ensure_relation_inputs_ready` must run before each solver dispatch so every
  reachable `Lazy(DefId)` is resolved in the `TypeEnvironment`; the solver's
  relation kernel cannot resolve `DefId`s on its own.
- **Inference fuel.** Solver-side, the `CallEvaluator` enforces
  `MAX_CONSTRAINT_RECURSION_DEPTH` (100) and `MAX_CONSTRAINT_STEPS` (20 000),
  plus a `reverse_mapped_depth` cap and a `reverse_mapped_visited` set for
  reverse mapped-type inference. The checker never raises these; pathological
  recursion is bounded by the solver.

## Edge cases and tsc parity

- **Union callees are not overloads.** `(F1 | F2)("a")` must be valid for *all*
  members (union-call semantics) rather than *any* member (overload semantics);
  the explicit `callee_is_union` guard forces `overload_signatures = None` so the
  solver's union-call path runs and `TS2554` is not lost.
- **Subtype pass before assignable pass.** An `any` argument is not related to
  concrete parameter types in the first overload pass, so a candidate that only
  matches because of `any` does not win over a later candidate that genuinely
  matches under the subtype relation — exactly tsc's `chooseOverload`.
- **Literal preservation in overloads.** Forcing `preserve_literal_types` keeps
  string/number literal arguments from widening, which is mandatory for literal
  overloads (`document.createElement("canvas")` matching the `"canvas"`
  overload).
- **Argument-source markers.** Literals from annotations/assertions/`as const`
  are not re-widened during inference, so `Object.fromEntries`-style `T` stays
  `1` instead of `number` only because a call is generic.
- **Bare enclosing type parameters do not contextually type callbacks.** They
  flow to `any` (`TS7006`) and the argument is checked against the parameter
  directly (`TS2345`).
- **Invalid explicit type arguments suppress argument diagnostics.** A
  `count_mismatch` or `constraint_violation` stops argument type-checking against
  the wrongly-instantiated signature, matching tsc's `errorType` propagation.
- **Variadic tuple-constraint spreads stay whole.** A type-parameter spread whose
  constraint is `readonly [L, ...L[]]` is kept as a single `[...A]` marker so the
  type parameter's identity survives inference.
- **`super<T>(...)` is always `TS2754`.** The parser strips the type arguments;
  the checker does not re-check argument arity against the (now-mismatched)
  constructor, avoiding a false `TS2554`.
- **`TS2575` overload arity gaps.** When an arity sits between two surrounding
  overload arities, the solver returns `OverloadArgumentCountMismatch` and the
  checker emits `TS2575` instead of a plain `TS2554`.
- **Fallback returns keep downstream checking alive.** `ArgumentTypeMismatch` and
  `NoOverloadMatch` both carry a `fallback_return`, so a rejected call still
  yields a usable type and secondary diagnostics (`TS2339` on the result) remain
  meaningful instead of cascading `any`/`error`.

## Where to read next

- The relation methods the adapter calls: [solver-relations](solver-relations.md).
- The inference kernel (`CallEvaluator`, `constrain_types`, reverse mapped
  inference): [solver-inference](solver-inference.md).
- Signature instantiation with inferred type arguments:
  [solver-instantiation](solver-instantiation.md).
- How `TS2345`/`TS2322` mismatches become structured reasons and messages:
  [checker-assignability-gateway](checker-assignability-gateway.md) and
  [checker-error-reporter-diagnostics](checker-error-reporter-diagnostics.md).
- Contextual typing and the contextual/object caches:
  [solver-caches-objects-contextual-compat](solver-caches-objects-contextual-compat.md).
- Constructor/`new` resolution shares the same machinery via
  `resolve_new_with_checker_adapter`; class-side checking is in
  [checker-classes](checker-classes.md).
- The whole call as a stage in the pipeline:
  [end-to-end-timeline](end-to-end-timeline.md).
