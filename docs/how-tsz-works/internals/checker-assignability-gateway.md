# The Assignability Gateway and Query Boundaries

This chapter documents the single most important checker-to-solver interface in
`tsz`: the assignability gateway. Every `TS2322` ("Type 'X' is not assignable to
type 'Y'"), `TS2345` ("Argument of type 'X' is not assignable to parameter of
type 'Y'"), `TS2416` ("Property 'p' in type 'X' is not assignable to the same
property in base type 'Y'"), and dozens of more specialized assignment-family
diagnostics flow through it. The gateway exists so that source-aware checker code
can ask *"is this source assignable to this target, and if not, why?"* without
ever owning a relation kernel, a `SubtypeFailureReason` walk, a relation cache,
or a raw `TypeKey`.

The pipeline the gateway implements is fixed: a checker source site builds a
`RelationRequest`, the boundary translates request policy into a solver
`RelationPolicy`, the solver decides the relation and (on failure) derives a
`SubtypeFailureReason` from the *same* configured `CompatChecker`, the boundary
re-wraps that into a checker-facing `RelationOutcome`/`RelationFailure`, and the
error reporter renders it with the right span, code, and elaboration chain. This
chapter traces the full surface of `crates/tsz-checker/src/query_boundaries`
and `crates/tsz-checker/src/assignability`, the two real directories that own
the request side and the diagnostic side respectively.

## Owns / Must not own

| Owns | Must not own |
| --- | --- |
| `RelationRequest` construction: `(source, target)` `TypeId` pair, `RelationKind`, excess/missing-property policy, fresh/spread markers, `overload_subtype_pass`, `decision_only` | The relation decision itself (lives in `tsz-solver`'s `CompatChecker` / `relation_queries`) |
| Translating packed checker `u16` flags into a typed solver `RelationPolicy` (`relation_policy::from_checker_flags_u16`) | The structural subtype algorithm, variance, weak-type classification (solver `relations`) |
| The checker-facing `RelationOutcome` and `RelationFailure`, derived from the solver's `AssignabilityFailureAnalysis` / `SubtypeFailureReason` | The authoritative shape of `SubtypeFailureReason` and `RelationResult` (solver `diagnostics::core`, `relation_queries`) |
| Choosing *which* diagnostic code and span to emit, suppression, dedup, elaboration ordering (`error_reporter`, `assignability_diagnostics`) | Constructing raw `TypeKey`, pattern-matching raw `TypeData`, reading printer output as a predicate |
| The checker-side assignability cache key namespace and the session-stamped failure memo (`assignability/cache_key`, `failure_memo`) | The solver's internal relation cache (the `RelationCacheKind::Assignable` slot is solver-owned) |
| Checker-only post-relation gates (iterator protocol, namespace property mismatch, alias-application arg rejection, `keyof` literal membership) | Re-deciding the structural relation; the gates only *downgrade* a `true` verdict |

For the algorithms that produce the answers the gateway returns, see
[solver-relations](solver-relations.md), [solver-narrowing](solver-narrowing.md),
[solver-evaluation](solver-evaluation.md),
[solver-caches-objects-contextual-compat](solver-caches-objects-contextual-compat.md),
and [solver-types-intern-def](solver-types-intern-def.md). For how the diagnostic
is finally formatted and pushed, see
[checker-error-reporter-diagnostics](checker-error-reporter-diagnostics.md). For
the context plumbing (`pack_relation_flags`, the two `TypeEnvironment`s, the
`CheckerOverrideProvider`), see
[checker-context-and-state](checker-context-and-state.md).

## Where the code lives

The gateway spans two directories. `query_boundaries` is the *request and
relation-execution* side; `assignability` is the *checker source-site and
diagnostic* side.

| Path | Role |
| --- | --- |
| `crates/tsz-checker/src/query_boundaries/relation_request.rs` | `RelationRequest`, `RelationKind`, `ExcessPropertyMode`, `MissingPropertyMode`; per-kind constructors; `solver_relation_policy`, `failure_memo_key` |
| `crates/tsz-checker/src/query_boundaries/relation_types.rs` | `RelationFailure` (checker-facing), `PropertyClassification`; `RelationFailure::from_solver_reason` |
| `crates/tsz-checker/src/query_boundaries/assignability.rs` | The boundary core: `RelationOutcome`, `execute_relation`, `check_assignable_gate_with_overrides`, `is_assignable_with_overrides`, `cached_assignability_with_overrides`, `classify_object_properties` |
| `crates/tsz-checker/src/query_boundaries/assignability/final_relation.rs` | `cached_final_assignability` — the checker-final verdict funnel under `RelationCacheKind::CheckerAssignable` |
| `crates/tsz-checker/src/query_boundaries/assignability/cache_key.rs` | `assignability_cache_key`, `subtype_cache_key`, boundary-safe `RelationFlags` `u16` constants |
| `crates/tsz-checker/src/query_boundaries/assignability/relation_kind_variants.rs` | Non-default kinds: bivariant-callback, subtype, redeclaration-identity relation queries |
| `crates/tsz-checker/src/query_boundaries/relation_policy.rs` | `from_checker_flags_u16` — the single packed-`u16` → `RelationPolicy` edge |
| `crates/tsz-checker/src/assignability/assignability_relation.rs` | `execute_relation_request` (the checker-side wrapper with the failure memo), `is_assignable_to`, `is_assignable_to_with_env`, `prepare_assignability_inputs` |
| `crates/tsz-checker/src/assignability/relation_outcome_helpers.rs` | The ~80 per-`RelationKind` `*_relation_outcome` methods on `CheckerState` |
| `crates/tsz-checker/src/assignability/failure_memo.rs` | `failure_memo_lookup` / `failure_memo_store` — the stamp-guarded reason memo (issue #13243) |
| `crates/tsz-checker/src/assignability/assignability_diagnostics.rs` | `analyze_assignability_failure` — the error reporter's structured-reason entry |
| `crates/tsz-checker/src/assignability/assignability_diagnostics/argument_reports.rs` | `check_argument_assignable_or_report` (TS2345), `check_assignable_or_report_generic_at` |
| `crates/tsz-checker/src/error_reporter/assignability.rs` | TS2322 emission, `render_failure_reason` dispatch, EPC fallback, code selection |
| `crates/tsz-solver/src/relations/relation_queries.rs` | `query_assignability_with_failure_analysis`, `query_assignability_decision_only`, `RelationPolicy`, `RelationContext`, `RelationResult`, `AssignabilityFailureAnalysis` |
| `crates/tsz-solver/src/diagnostics/core.rs` | `SubtypeFailureReason` — the authoritative solver reason vocabulary |
| `crates/tsz-solver/src/relations/compat.rs` | `AssignabilityOverrideProvider` trait, `CompatChecker` (Lawyer layer) |
| `crates/tsz-solver/src/types.rs` | `RelationCacheKey`, `RelationCacheKind`, `RelationFlags` bitflags |

## The end-to-end data flow

```text
 checker source site (e.g. variable initializer, call arg, return)
   |
   |  prepare_assignability_inputs:
   |    ensure_relation_inputs_ready (force Lazy DefIds)
   |    substitute_this_type_if_needed
   |    evaluate_type_for_assignability  (both sides)
   v
 RelationRequest { source, target, kind, excess/missing modes, flags ... }
   |
   |  CheckerState::execute_relation_request
   |    failure_memo_key -> failure_memo_lookup (replay or miss)
   |    CheckerOverrideProvider::new(self, None)
   v
 query_boundaries::assignability::execute_relation
   |    request.solver_relation_policy(packed_u16)  -> (SolverKind, solver_flags)
   |    assignability_policy_and_context            -> (RelationPolicy, RelationContext)
   v
 tsz_solver::relations::relation_queries
   |    query_assignability_with_failure_analysis   (decision + reason, one CompatChecker)
   |      or query_assignability_decision_only       (decision_only requests)
   v
 AssignabilityQueryOutcome { RelationResult, Option<AssignabilityFailureAnalysis> }
   |
   |  back in execute_relation:
   |    RelationFailure::from_solver_reason(reason)
   |    classify_object_properties (when fresh/EPC requested)
   |    suppress_excess_property_failure_if_needed
   v
 RelationOutcome { related, depth_exceeded, iteration_exceeded,
                   failure, weak_union_violation, property_classification }
   |
   |  back in execute_relation_request:
   |    failure_memo_store(capture)
   |    propagate_overflow_flags
   |    apply_checker_side_downgrade (iterator/namespace/keyof gates)
   v
 checker source site reads outcome.related / .failure / .weak_union_violation
   |
   v
 error_reporter::assignability -> analyze_assignability_failure (memo replay)
                               -> render_failure_reason -> push_diagnostic (TS2322/2345/2416)
```

Note that the solver is asked at most twice for a failing assignment: once by the
`RelationRequest` gateway (to pick the diagnostic shape) and once by
`analyze_assignability_failure` (to build the elaboration chain). The
session-stamped failure memo collapses the second call into a replay of the first
(issue #13243). Both call sites are keyed on the *same* prepared
`(source, target, flags, sound_mode)` tuple, so the memo can never produce a
reason inconsistent with the decision.

## The request side: `RelationRequest`

`RelationRequest` (`query_boundaries/relation_request.rs`) is the checker's
vocabulary for "what kind of relation am I asking about, and under what
diagnostic policy". It is a small `Copy`-friendly value, not a callback:

```rust
pub(crate) struct RelationRequest {
    pub source: TypeId,
    pub target: TypeId,
    pub kind: RelationKind,
    pub excess_property_mode: ExcessPropertyMode,
    pub missing_property_mode: MissingPropertyMode,
    pub source_is_fresh: bool,
    pub allow_erased_generic_signature_retry: bool,
    pub overload_subtype_pass: bool,
    pub decision_only: bool,
}
```

The crucial design point is that `source` and `target` are **prepared**
`TypeId`s: lazy refs forced, `ThisType` substituted, and evaluated. The checker
does this preparation *before* building the request, in
`prepare_assignability_inputs` (`assignability/assignability_relation.rs`):

```rust
pub(crate) fn prepare_assignability_inputs(&mut self, source, target) -> (TypeId, TypeId) {
    self.ensure_relation_inputs_ready(&[source, target]);
    let raw_source = self.substitute_this_type_if_needed(source);
    let raw_target = self.substitute_this_type_if_needed(target);
    let source = self.evaluate_type_for_assignability(raw_source);
    let target = self.evaluate_type_for_assignability(raw_target);
    (source, target)
}
```

`ensure_relation_inputs_ready` forces every referenced `TypeData::Lazy(DefId)`
to be resolved before the relation runs. The doc comment on the source warns that
skipping this "silently drop[s] TS2322/TS2345 diagnostics" — an unresolved lazy
ref compares as an error type and masks the real mismatch. This is the checker
honoring the `DefId -> TypeId` resolution boundary owned by `TypeEnvironment`
(see [solver-types-intern-def](solver-types-intern-def.md)) rather than reaching
into solver internals.

### `RelationKind`: one variant per diagnostic context

`RelationKind` enumerates roughly a hundred distinct contexts. It is **not** the
solver's relation kind — it is a *checker-side* policy/diagnostics selector. The
common ones map directly to a TypeScript diagnostic family:

| `RelationKind` | Source site | Typical diagnostic |
| --- | --- | --- |
| `Assign` | `const x: T = expr` | TS2322 |
| `AssignabilityReason` | TS2322 reason-entrypoint relation | TS2322 |
| `CallArg` | `fn(expr)` argument | TS2345 |
| `Return` | `return expr` | TS2322 (return position) |
| `Satisfies` | `expr satisfies T` | TS1360 family |
| `Destructuring` | `const { a } = expr` | TS2322/TS2459 |
| `RestParameter` | `function f(...args: T)` | TS2345 |
| `JsxProps` / `JsxChildren` / `JsxElementType` | JSX attributes/children | TS2322/TS2746 |
| `TypeComparability` | `===`/overlap probes | TS2367/TS2678 |
| `IndexSignature` | index value compat | TS2411/TS2413 |

The remaining variants are narrow probes used during a single elaboration —
e.g. `IndexAccessConstraintKey`, `ConditionalTrueBranchConstraint`,
`InterfaceHeritagePropertyIndex`, `PolymorphicThisReceiver`,
`MappedObjectLiteralExcessValue`. Each exists so the boundary and the diagnostic
layer can branch on *why the checker is asking*, without the checker re-deriving
that from raw type shapes. Every variant has a `const fn` constructor (e.g.
`RelationRequest::call_arg`, `RelationRequest::return_stmt`,
`RelationRequest::satisfies`), so a source site never builds the struct literal
directly.

### Excess and missing property policy

Two enums describe how the boundary should classify object properties:

- `ExcessPropertyMode`: `Skip` (default, non-fresh sources),
  `Check` (fresh object literals — full EPC), `CheckExplicitOnly` (spread
  expressions; only written properties are checked).
- `MissingPropertyMode`: `Report` (default) or `Suppress` (e.g. `Partial<T>`
  patterns).

The builder methods encode the tsc-faithful defaults: `with_fresh_source()` sets
both `source_is_fresh = true` and `excess_property_mode = Check`;
`with_spread_source()` sets `CheckExplicitOnly`. `requires_property_classification`
is the predicate the boundary uses to decide whether to run
`classify_object_properties` at all — it returns `true` only when the source is
fresh, excess checking is on, or missing properties are reported.

### Translating to solver policy

`solver_relation_policy(base_flags)` is the one method that bridges the checker's
request kind into the solver's relation kind and flags:

```rust
pub(crate) fn solver_relation_policy(&self, base_flags: u16)
    -> (relation_queries::RelationKind, u16) {
    let mut flags = base_flags;
    if self.allow_erased_generic_signature_retry {
        flags |= RelationFlags::ALLOW_ERASED_GENERIC_SIGNATURE_RETRY.bits() as u16;
    }
    if self.kind == RelationKind::BivariantCallbacks {
        (RelationKind::AssignableBivariantCallbacks,
         flags & !(RelationFlags::STRICT_FUNCTION_TYPES.bits() as u16))
    } else {
        (RelationKind::Assignable, flags)
    }
}
```

Almost every `RelationKind` maps to the solver's plain `Assignable` relation —
the diagnostic distinctions live entirely on the checker side. The two genuine
*relation-policy* differences are `BivariantCallbacks` (strips
`STRICT_FUNCTION_TYPES` so callback parameters are compared bivariantly, tsc's
method-parameter rule) and `overload_subtype_pass` (handled separately because it
rides a typed `any`-propagation mode, not a packed flag — see below).

## The packed-flags edge

The checker never hands the solver a typed `RelationPolicy` directly from a
source site. Instead it packs the effective compiler options into a `u16` via
`CheckerContext::pack_relation_flags` (`context/compiler_options.rs`):

```rust
pub const fn pack_relation_flags(&self) -> u16 {
    let mut flags: u16 = RelationFlags::ALLOW_BIVARIANT_REST;
    if self.strict_null_checks()          { flags |= RelationFlags::STRICT_NULL_CHECKS; }
    if self.strict_function_types()       { flags |= RelationFlags::STRICT_FUNCTION_TYPES; }
    if self.exact_optional_property_types(){ flags |= RelationFlags::EXACT_OPTIONAL_PROPERTY_TYPES; }
    if self.no_unchecked_indexed_access() { flags |= RelationFlags::NO_UNCHECKED_INDEXED_ACCESS; }
    flags
}
```

The `RelationFlags` referenced here are the boundary-safe `u16` constants in
`query_boundaries/assignability/cache_key.rs` — they mirror the solver's typed
`RelationFlags` bit surface but keep the packed-`u16` protocol *quarantined* to
the boundary. The actual decode lives in one place,
`relation_policy::from_checker_flags_u16`, which calls `RelationPolicy::from_flags`.
Sound Mode adds two more bits at the boundary edge, in
`assignability_policy_and_context`:

```rust
let policy = relation_policy::from_checker_flags_u16(flags)
    .with_strict_subtype_checking(sound_mode)
    .with_strict_any_propagation(sound_mode);
```

`with_strict_any_propagation` is what makes `any` *not* silence structural
mismatches in Sound Mode — the Lawyer layer's central unsoundness toggle (see the
Compatibility Model in `.claude/CLAUDE.md` and
[solver-relations](solver-relations.md)).

## The boundary core: `execute_relation`

`execute_relation` (`query_boundaries/assignability.rs`) is the single
authoritative entry point for relation queries that need structured failure
information. Its return type, `RelationOutcome`, is the canonical packed answer:

```rust
pub(crate) struct RelationOutcome {
    pub related: bool,
    pub depth_exceeded: bool,        // -> TS2321 "Excessive stack depth"
    pub iteration_exceeded: bool,    // -> TS2859 "Excessive complexity"
    pub failure: Option<RelationFailure>,
    pub weak_union_violation: bool,  // -> TS2559 (emit EPC instead)
    pub property_classification: Option<PropertyClassification>,
}
```

`execute_relation` does the following, in order:

1. Computes `(solver_kind, solver_flags)` via `request.solver_relation_policy`.
2. If a `precomputed` `CachedAssignabilityAnalysis` was passed (a memo replay),
   it reconstructs the analysis without touching the solver and records a
   `record_relation_failure_memo_hit` perf counter.
3. Otherwise builds `(policy, context)` via `assignability_policy_and_context`.
   For `overload_subtype_pass` requests it additionally applies
   `policy.with_any_propagation_mode(AnyPropagationMode::AnySourceNotRelated)` —
   the typed mode that makes an `any` source not relate to non-`any`/non-`unknown`
   targets at every nesting level (tsc's `chooseOverload` with `subtypeRelation`).
   This mode participates in `RelationPolicy::cache_config`, so subtype-pass
   results never share a relation cache slot with the default assignable relation.
4. Runs the solver: `query_assignability_decision_only` for `decision_only`
   requests, `query_assignability_with_failure_analysis` otherwise.
5. On `related == true`, returns immediately with an all-clear outcome.
6. On failure, converts the solver `SubtypeFailureReason` into a
   `RelationFailure` via `from_solver_reason`, runs
   `classify_object_properties` when the request requires it, and applies
   `suppress_excess_property_failure_if_needed`.

The second return value is a `CachedAssignabilityAnalysis` — the raw captured
analysis a non-`decision_only` pass produced, for the caller to memoize. It is
`None` on the decision-only path and on memo replays.

### The single-pass decision-plus-reason invariant

The most important correctness property of `execute_relation` (and of the solver
entry `query_assignability_with_failure_analysis`) is that the pass/fail decision
and the failure reason are produced by **one** configured `CompatChecker`. The
solver doc comment is explicit about the bug class this prevents:

> The query boundary previously decided pass/fail with one configured checker and
> then computed the failure reason with a second, independently configured
> checker. Those two traversals could disagree — producing a failure reason that
> contradicts the decision, or no reason at all when a checker override (enum /
> abstract-constructor / accessibility / private brand) forced the failure
> before the structural walk ran.

So the decision and the explanation share the same relation cache, the same
overrides, and the same policy. A `decision_only` request runs the *identical*
decision pass but skips `explain_failure` and the weak-union probe, which on
large type-level programs would re-traverse the failing relation graph per probe
(issue #13213).

## The checker wrapper: `execute_relation_request`

Source sites do not call `execute_relation` directly. They call
`CheckerState::execute_relation_request` (`assignability/assignability_relation.rs`),
which adds the checker-only concerns the boundary cannot own:

```rust
pub(crate) fn execute_relation_request(&mut self, request: &RelationRequest) -> RelationOutcome {
    let flags = self.ctx.pack_relation_flags();

    // checker-only acceptance fast paths
    if self.homomorphic_mapped_display_source_assignable_to_target(request.source, request.target)
        || self.callable_source_satisfies_union_callable_arm(request.source, request.target) {
        return RelationOutcome { related: true, .. };
    }

    let memo_key = request.failure_memo_key(flags, self.ctx.sound_mode());
    let precomputed = memo_key.and_then(|key| self.failure_memo_lookup(key));
    let overrides = CheckerOverrideProvider::new(self, None);

    let lazy_failures_at_entry = lazy_resolve_failure_count();
    let (mut outcome, capture) = execute_relation(
        request, self.ctx.types, &self.ctx, flags,
        &self.ctx.inheritance_graph, &overrides, self.ctx.sound_mode(),
        precomputed.as_ref(),
    );

    if let (Some(key), Some(capture)) = (memo_key, capture) {
        self.failure_memo_store(key, capture, lazy_failures_at_entry);
    }
    self.propagate_overflow_flags(outcome.depth_exceeded, outcome.iteration_exceeded);
    self.apply_checker_side_downgrade(&mut outcome, request.source, request.target);
    outcome
}
```

Each per-`RelationKind` helper in `relation_outcome_helpers.rs` is a thin wrapper
that constructs the right request and calls this method. For example:

```rust
pub(crate) fn return_relation_outcome(&mut self, source, target) -> RelationOutcome {
    let request = RelationRequest::return_stmt(source, target);
    self.execute_relation_request(&request)
}
```

The reason-entrypoint helper `assignability_reason_relation_outcome` is the one
TS2322 source sites use; it layers several checker-only acceptance/rejection fast
paths (deferred index-access constraints, variance-accepted applications,
empty-object keyof index access) around the request before and after the boundary
call.

### `propagate_overflow_flags` and the budget diagnostics

`RelationResult` carries `depth_exceeded` and `iteration_exceeded`. When set, the
checker propagates them to the context (`propagate_overflow_flags`) so the
compiler can emit TS2321 ("Excessive stack depth comparing types") and TS2859
("Excessive complexity") respectively, instead of a misleading concrete mismatch.
These flags are *not* a failure reason — they are a fuel signal from the solver's
recursion/iteration guards (see [solver-relations](solver-relations.md) for the
relation fuel model).

## The reason vocabulary and its translation

The solver's reason type is `SubtypeFailureReason`
(`tsz-solver/src/diagnostics/core.rs`). It is rich and tsc-shape-aware — e.g.
`TupleArityMismatch(TupleArity)` pre-classifies into tsc's TS2618–TS2621 family,
`TupleElementTypeMismatch` carries a `multi_element` bool so the renderer knows
whether to emit the positional "Type at position N in source..." line, and
`ParameterTypeMismatch` carries an `inner_reason` so a callback's inner failure
(return vs parameter) can be distinguished.

The boundary translates this into the smaller, checker-facing `RelationFailure`
(`query_boundaries/relation_types.rs`) via `RelationFailure::from_solver_reason`.
This is deliberately **not** 1:1: it groups solver details into the categories the
checker's diagnostic renderer branches on.

| `SubtypeFailureReason` (solver) | `RelationFailure` (checker) |
| --- | --- |
| `MissingProperty` / `MissingProperties` | same |
| `ExcessProperty` | `ExcessProperty` |
| `PropertyTypeMismatch { nested_reason }` | `IncompatiblePropertyValue { nested }` |
| `ReturnTypeMismatch` / `ParameterTypeMismatch` | same names |
| `TupleArityMismatch` / `TupleElementMismatch` | `TupleArityMismatch { source_count, target_count }` |
| `NoCommonProperties` | `WeakUnionViolation` |
| `TypeMismatch` / `IntrinsicTypeMismatch` / `LiteralTypeMismatch` / `ErrorType` / `ReadonlyToMutableAssignment` / `UnionSourceMismatch` / `UnionTargetMismatch` / `ConditionalBranchMismatch` / `TypeParameterConstraintMismatch` / `IntersectionTargetMismatch` | all collapse to `TypeMismatch { source_type, target_type }` |
| `OptionalPropertyRequired` / `ReadonlyPropertyMismatch` / `PropertyNominalMismatch` / `PropertyVisibilityMismatch` | `PropertyModifierMismatch { property_name }` |
| `IndexAccessTypeParameterMismatch` | same (drives the TS5075 instantiation note) |

The many-to-one collapses are intentional: the *live* elaboration chain is
rendered straight from the structured solver reason (via `render_failure_reason`
and its nested `nested_reason` chain), so `RelationFailure` only needs to carry
enough to let the checker pick the diagnostic code and anchor. The collapsed
variants keep the relevant `(source, target)` pair for that decision while the
detailed sub-chain is recovered from the solver reason. This is why a union-target
failure keeps the `source/union` pair in `RelationFailure::TypeMismatch` while the
best-matching member and its missing-property reason are rendered from the solver
reason's structured chain.

### `PropertyClassification`

`PropertyClassification` is the canonical boundary output for object-level
analysis, populated by `classify_object_properties` when the request is fresh /
EPC-bearing. It lists `excess_properties`, `missing_properties`,
`incompatible_properties`, and a set of target-shape flags
(`target_has_index_signature`, `target_is_type_parameter`,
`target_is_empty_object`, `target_has_number_index`, ...). The `all_matching_compatible`
and `trimmed_source_assignable` booleans let `should_skip_weak_union_error` decide
whether a failure is caused *only* by excess properties without re-enumerating
and re-checking the properties — a hot-path optimization.

## The two assignability cache namespaces

There are two completely disjoint relation-cache namespaces, partitioned by
`RelationCacheKind` (`tsz-solver/src/types.rs`):

| Cache kind | Key builder | Written/read by | Honesty contract |
| --- | --- | --- | --- |
| `RelationCacheKind::Assignable` | `assignability_cache_key` (`for_assignability`) | Raw Lawyer relation (`is_assignable_with_overrides` → `cached_assignability_with_overrides`) | A cached `bool` is the *raw* relation verdict, before checker post-gates |
| `RelationCacheKind::CheckerAssignable` | `checker_final_assignability_cache_key` (`for_checker_assignability`) | The checker-final funnel `cached_final_assignability` only | A cached `bool` is *authoritative* — callers return it with no post-processing |

Both keys are derived from the same typed `RelationCacheConfig`
(`RelationPolicy::cache_config()`), so a checker write lands in the same slot as
the solver's internal write path *within its kind*. Because the kinds are
disjoint, the raw relation cache and the checker-final verdict can never poison
each other: the checker-final entry already folds in the post-relation gates, so
storing it under a separate kind keeps the raw entry honest for solver-internal
recursive queries.

`is_relation_cacheable` gates whether either cache is consulted at all (types with
free infer types or other non-stable shapes are not cached). `cached_final_assignability`
demonstrates the funnel discipline precisely:

```rust
// 1. lookup CheckerAssignable
if is_cacheable && let Some(cached) = lookup_assignability_cache(cache_key) { return cached; }
// 2. run the raw Lawyer relation (is_assignable_with_overrides)
let raw_related = relation_result.is_related();
// 3. insert the RAW verdict provisionally (so gate-issued recursive queries see it)
if is_cacheable { insert_assignability_cache(cache_key, raw_related); }
// 4. apply post-relation true-override gates
let result = raw_related && !self.assignability_true_override_rejects(source, target);
// 5. downgrade the stored entry if a gate rejected
if is_cacheable && result != raw_related { insert_assignability_cache(cache_key, result); }
```

The provisional insert before the gates is deliberate: the gates may issue their
own recursive relation queries, and those must observe the raw relation verdict
(the pre-#13243 post-pass semantics), not an in-progress sentinel.

## The session-stamped failure memo

A failing TS2322/TS2345 assignment historically executed the reason-collecting
relation more than once: once through the `RelationRequest` gateway (to choose the
diagnostic shape) and again inside `analyze_assignability_failure` when the error
reporter builds the elaboration chain. The failure memo (`assignability/failure_memo.rs`,
issue #13243) makes the second a pure replay.

- Key: `AssignabilityFailureKey = (TypeId, TypeId, u16, bool)` — prepared source,
  prepared target, packed solver flags, sound-mode (`context/caches.rs`).
- Storage: `assignability_failure_memo` under the
  `AssignabilityEvalStamp` validity model. Entries are dropped *wholesale*
  whenever the session stamp moves, so a hit replays exactly what a fresh pass in
  the current type environment would produce.
- `RelationRequest::failure_memo_key` returns `None` for memo-ineligible requests:
  `decision_only` (those stay reason-free), `overload_subtype_pass` (its
  `any`-propagation mode changes the relation outside the packed flags), and any
  request whose solver kind is not plain `Assignable`.

`failure_memo_store` mirrors the `evaluate_type_for_assignability` memo's
cleanliness guards: it refuses to persist an analysis that was depth/iteration
degraded, ran under exhausted resolution fuel, or compared against a
not-yet-registered `Lazy(DefId)` body. The last guard uses a snapshot of
`lazy_resolve_failure_count` taken before the relation; if the count advanced, the
analysis is a function of the registration window it ran in (not of the prepared
key alone) and must not be cached — the relation-layer analog of the env-eval
`unresolved_def_seen` backstop (issue #12101).

## Walk-through 1: `const x: { a: number } = { a: 1, b: 2 }`

A fresh object literal with an excess property, the canonical TS2322 EPC case.

1. The variable-declaration checker computes the initializer type
   `{ a: number; b: number }` (fresh) and the annotation target `{ a: number }`.
2. It builds a fresh request via `RelationRequest::variable_initializer(...).with_fresh_source()`,
   so `source_is_fresh = true` and `excess_property_mode = Check`.
3. `execute_relation_request` packs flags, computes `failure_memo_key` (eligible —
   plain `Assignable`, not decision-only), looks up the memo (miss on first
   encounter), and calls `execute_relation`.
4. `execute_relation` runs `query_assignability_with_failure_analysis`. The
   structural relation actually *holds* for the shared property `a`, but the fresh
   source carries an excess `b`. The solver returns `related = false` with
   `SubtypeFailureReason::ExcessProperty { property_name: b, target_type }`.
5. Because `requires_property_classification()` is true, `execute_relation` runs
   `classify_object_properties`, populating `excess_properties = [b]` and
   `all_matching_compatible = true`.
6. `suppress_excess_property_failure_if_needed` checks whether the target has a
   structure (index signature, deferred conditional, primitive intersection
   member) that makes EPC inapplicable. It does not, so the `ExcessProperty`
   failure survives.
7. The capture is stored in the failure memo under the prepared key.
8. The error reporter's TS2322 path (`error_reporter/assignability.rs`) calls
   `analyze_assignability_failure`, which *replays* the memo, sees the
   `ExcessProperty` reason, and routes to `check_object_literal_excess_properties`
   on each RHS object literal — emitting TS2353 ("Object literal may only specify
   known properties") anchored at the `b` property, not a generic TS2322.

## Walk-through 2: `function f(x: string) {}; f(123)` (TS2345)

1. The call checker resolves the parameter type `string` (target) and the argument
   type `123` / `number` (source).
2. `check_argument_assignable_or_report` (`assignability_diagnostics/argument_reports.rs`)
   narrows `this` from any enclosing `typeof` guard, runs the suppression and
   parse-recovery gates, then calls `self.call_arg_relation_outcome(source, target)`.
3. `call_arg_relation_outcome` builds `RelationRequest::call_arg(source, target)`
   and runs it through `execute_relation_request`.
4. The solver decides `number` is not assignable to `string` and returns
   `SubtypeFailureReason::TypeMismatch { source_type, target_type }`, which
   `from_solver_reason` maps to `RelationFailure::TypeMismatch`.
5. `outcome.related == false` and `outcome.weak_union_violation == false`, so the
   argument reporter falls through to `error_type_not_assignable_*`, which emits
   TS2345 ("Argument of type 'number' is not assignable to parameter of type
   'string'") anchored at the argument node `123`.

The only structural difference from walk-through 1 is the *diagnostic code* and
*anchor* — both checker-side decisions. The relation kernel that ran was the same
`Assignable` relation; the request's `RelationKind::CallArg` is what steered the
checker to TS2345 wording and the argument span.

## Walk-through 3: `class D extends B { x: string }` where `B.x: number` (TS2416)

1. The class checker (`classes/class_checker.rs`, "error 2416") iterates the
   derived class's members against the base class's same-named members.
2. For property `x` it asks the gateway whether the derived type `string` is
   assignable to the base type `number`.
3. The relation fails with `TypeMismatch`; the class checker emits TS2416
   ("Property 'x' in type 'D' is not assignable to the same property in base type
   'B'") anchored at the derived property. The relation machinery is identical to
   the previous two cases — only the *site* (member-override comparison) and the
   *code/anchor* differ.

## Checker-only acceptance and rejection gates

The solver decides the *structural* relation; a handful of TypeScript rules the
solver cannot observe live as checker-side gates that wrap the boundary result.

Acceptance gates run *before* the boundary call (in `execute_relation_request`
and `assignability_reason_relation_outcome`) and short-circuit to `related = true`:
`homomorphic_mapped_display_source_assignable_to_target`,
`callable_source_satisfies_union_callable_arm`, deferred index-access constraint
widening, and variance-accepted applications.

Rejection gates run *after* a solver `true` verdict and downgrade it, grouped in
`assignability_true_override_rejects` (`assignability/application_keyof_helpers.rs`):

1. `same_type_alias_application_args_reject` — `Alias<A>` vs `Alias<B>` whose
   unwitnessed arguments differ.
2. `checker_only_assignability_failure_reason` — iterator-protocol display
   mismatches the solver relation cannot see.
3. `namespace_source_has_matching_property_mismatch` — namespace-module source
   property mismatches.
4. `string_literal_source_outside_keyof_target` — a string-literal source outside
   a concretely resolvable `keyof` target's key set.

`apply_checker_side_downgrade` is the path that downgrades `outcome.related` to
`false` for the iterator-result and similar protocol mismatches. Its source
comment is precise about *not* populating `outcome.failure` from a downgrade — the
structured reason is recovered later by `analyze_assignability_failure`, and
populating `outcome.failure` here had unrelated semantic side effects on
`outcome.failure`-reading predicates (a #12239 conformance regression on
`coAndContraVariantInferences2.ts` and `correlatedUnions.ts`).

These gates are the legitimate place for the "Lawyer" exceptions that need
binder/symbol knowledge. The three callbacks that need binder data even *inside*
the relation — enum, abstract-constructor, and constructor-accessibility overrides
— are delivered to the solver through the `AssignabilityOverrideProvider` trait
(`tsz-solver/src/relations/compat.rs`), implemented by `CheckerOverrideProvider`
(`state/state.rs`). The provider is passed into every relation query so the
`CompatChecker` can call back for those special cases without the checker
re-running the structural walk.

## Caches and invariants

- **Two disjoint relation caches.** `RelationCacheKind::Assignable` stores raw
  Lawyer verdicts; `RelationCacheKind::CheckerAssignable` stores checker-final
  verdicts that fold in the post-relation gates. They never share a slot.
  Cacheability is gated by `is_relation_cacheable`.
- **One key derivation.** Both `assignability_cache_key` and
  `checker_final_assignability_cache_key` derive their config from the same
  `RelationPolicy::cache_config()`, so checker writes coincide with solver-internal
  writes within a kind.
- **Flags are part of the key.** The packed `u16` (strict-null, strict-function,
  exactOptional, noUncheckedIndexedAccess, allow-bivariant-rest) plus Sound Mode's
  `STRICT_SUBTYPE_CHECKING`/`STRICT_ANY_PROPAGATION` and the
  erased-generic-retry bit all participate in the cache config. Results computed
  under one configuration never leak into another.
- **The `any`-propagation mode partitions the cache.** The overload subtype pass's
  `AnySourceNotRelated` mode is encoded in `cache_config`, so subtype-pass results
  cannot share slots with the default assignable relation even though the packed
  `u16` flags are identical.
- **Failure memo is stamp-scoped.** `assignability_failure_memo` entries are
  invalidated wholesale on `AssignabilityEvalStamp` movement; a hit replays a
  reason-collecting pass valid for the current environment. Degraded/under-resolved
  analyses are never stored (`failure_memo_store` guards).
- **Decision and reason cannot diverge.** Both come from one configured
  `CompatChecker` in `query_assignability_with_failure_analysis`, sharing its
  relation cache.
- **Prepared inputs only.** Requests carry evaluated, lazy-forced, this-substituted
  `TypeId`s. The failure memo key is on the *prepared* pair, matching the key used
  by `analyze_assignability_failure` so the two sites memo-share.

## Edge cases and tsc parity

- **Weak-type / weak-union (TS2559).** When `outcome.weak_union_violation` is set
  (solver `NoCommonProperties`), the checker emits the excess-property / no-common-
  properties diagnostic instead of a plain TS2322. `is_assignable_no_weak_checks`
  exists because tsc's `isTypeAssignableTo` does *not* include the weak-type check;
  the flow narrowing guard uses that variant so narrowing matches tsc.
- **Excess-property suppression.** `suppress_excess_property_failure_if_needed`
  drops an `ExcessProperty` reason when the target contains a deferred conditional
  (structural mismatch, not EPC) or an intersection with primitive/type-parameter
  members. This is boundary-level policy, replacing what used to be checker-local
  re-analysis.
- **Intersection-target elaboration.** tsc relates a source to each constituent of
  a target intersection in written order and elaborates the first failing
  constituent. tsz evaluates the intersection into a merged object before the
  reason is built, so `wrap_intersection_target_failure` re-relates against each
  constituent and re-nests the first failure under an `IntersectionTargetMismatch`
  frame — a display-only restructure; the relation decision is unchanged.
- **Index-access type-parameter mismatch (TS5075).** Two distinct type parameters
  used as keys of structurally-identical indexed-access types (e.g.
  `JSX.IntrinsicElements[T1]` vs `[T2]`) produce
  `IndexAccessTypeParameterMismatch`, which drives the exact tsc chain plus the
  "could be instantiated with a different subtype of constraint" TS5075 note.
- **Tuple arity precision.** A variadic source like `[boolean, ...number[]]`
  reports its *required* length (`1`) via `TupleArityMismatch(TupleArity)`, not its
  slot count (`2`), matching tsc's TS2618–TS2621 wording.
- **Array-extending interfaces and false TS2559.** `analyze_assignability_failure`
  suppresses `NoCommonProperties` when the target extends Array/tuple, because such
  interfaces inherit non-optional members from `Array.prototype` that are absent
  from the `ObjectShape` property list and would otherwise look like weak types.
- **`exactOptionalPropertyTypes`.** When the flag is set, a `string | undefined`
  source assigned to an exact-optional target emits TS2375/TS2379 ("...with
  'exactOptionalPropertyTypes: true'. Consider adding 'undefined'..."), handled by
  the dedicated exact-optional reporter rather than plain TS2322.
- **Budget exhaustion.** `depth_exceeded`/`iteration_exceeded` surface as TS2321 /
  TS2859 rather than a spurious concrete mismatch, so deep recursive types report a
  complexity error exactly like tsc.

## Anti-hardcoding posture

Every decision in this gateway is structural. `RelationKind` distinguishes
*diagnostic contexts*, never user identifiers, alias names, type-parameter names,
property names (except true builtins via stable builtin identity), file names, or
rendered type strings. The boundary translates request policy into a typed
`RelationPolicy` and consumes a structured `SubtypeFailureReason`; it never
inspects formatted diagnostic text as a predicate. The checker-only gates use
binder/global builtin identity (`namespace_module_names`, `keyof` key-set
resolution) and solver structural helpers, not name matching. The printer reads
types for display; types are never read back from printer output to make a
relation decision.

For the relation algorithm, weak-type classification, and `explain_failure`, see
[solver-relations](solver-relations.md). For how `evaluate_type_for_assignability`
prepares inputs, see [solver-evaluation](solver-evaluation.md) and
[solver-caches-objects-contextual-compat](solver-caches-objects-contextual-compat.md).
For where the rendered diagnostic is finally pushed, deduplicated, and ordered,
see [checker-error-reporter-diagnostics](checker-error-reporter-diagnostics.md).
For the call-argument and overload machinery that drives most TS2345s, see
[checker-calls-signatures-generics](checker-calls-signatures-generics.md). For the
class-member override comparison behind TS2416, see
[checker-classes](checker-classes.md).
