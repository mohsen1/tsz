# The Relation Engine: Subtype, Assignability, Identity, Comparable, Variance

The relation engine answers one family of questions for the rest of the
compiler: *given two `TypeId` handles, are they related?* "Related" is
overloaded — `is source a structural subtype of target?`, `is source
assignable to target under TypeScript's legacy rules?`, `are these two types
identical for redeclaration?`, `do these two types overlap?` — and each of
those is a distinct **relation kind** with its own quirks. The engine lives
entirely under `crates/tsz-solver/src/relations` and is the single owner of
relation decisions in the pipeline. The checker never runs the structural walk
itself; it phrases a `RelationPolicy` and a `RelationKind` and calls a query
function (`relation_queries.rs`), which builds the right checker, runs it, and
returns a boolean plus, on failure, a structured `SubtypeFailureReason`.

The module is organized around the **Judge / Lawyer** split documented in the
crate's own module headers. The **Judge** (`SubtypeChecker`, in
`relations/subtype/`) implements pure, sound, set-theoretic structural
subtyping — it knows nothing about TypeScript's unsound legacy quirks. The
**Lawyer** (`CompatChecker` + `AnyPropagationRules`, in `relations/compat.rs`
and `relations/lawyer.rs`) wraps the Judge and layers on the TypeScript-specific
business rules: `any` propagation, weak-type detection (TS2559), excess-property
freshness (TS2353), enum/private-brand nominality, and the empty-object/Function
interface escape hatches. A guiding invariant, stated verbatim in `lawyer.rs`:
*the Lawyer never makes types MORE compatible — it only adds restrictions on top
of the Judge's structural analysis.* (The one true exception is `any`, which the
Lawyer makes *more* permissive by short-circuiting.)

This document traces how those layers interact, how the structural walk
dispatches by type kind, how variance is computed and applied, how recursion and
fuel guards bound the walk, how the relation caches are keyed and invalidated,
and how a failing relation is turned into a diagnostic reason. Sibling internals
docs cover the consumers and neighbors: the assignability gateway in the checker
([checker-assignability-gateway](checker-assignability-gateway.md)), evaluation
([solver-evaluation](solver-evaluation.md)), inference
([solver-inference](solver-inference.md)), instantiation
([solver-instantiation](solver-instantiation.md)), narrowing
([solver-narrowing](solver-narrowing.md)), type construction and interning
([solver-types-intern-def](solver-types-intern-def.md)), and the cache
landscape ([solver-caches-objects-contextual-compat](solver-caches-objects-contextual-compat.md)).

## Owns / Must not own

**Owns:**

- The definition of every relation kind (`RelationKind` in
  `relation_queries.rs`) and the policy bundle (`RelationPolicy`) that
  configures one.
- The structural comparison kernel: `SubtypeChecker::check_subtype` and its
  per-kind dispatch (`check_subtype_inner_impl` in `subtype/core_dispatch.rs`,
  the `SubtypeVisitor` in `subtype/visitor.rs`, and the category rules under
  `subtype/rules/`).
- Coinductive cycle detection, depth/iteration limits, and the cross-operation
  fuel guard.
- The TypeScript compatibility rules layered by `CompatChecker`: `any`,
  weak types, excess properties, void-return, bivariant-rest, nominal brands.
- Variance computation (`relations/variance.rs`) and the per-argument variance
  walk used by the `Application`-vs-`Application` fast paths.
- The structured failure-reason production (`subtype/explain*.rs` plus
  `CompatChecker::explain_failure`), and the relation caches (the shared
  `QueryCache` and the instance-local memos).

**Must not own:**

- Source locations, AST node identity, or diagnostic *rendering*. The engine
  produces a `SubtypeFailureReason`; the checker's error reporter renders it
  (see [checker-error-reporter-diagnostics](checker-error-reporter-diagnostics.md)).
- Meta-type *evaluation* — `keyof`, conditional, mapped, indexed-access, and
  template-literal reduction belong to the `TypeEvaluator`
  ([solver-evaluation](solver-evaluation.md)); the relation engine *calls* it
  (`evaluate_type`) but does not implement it.
- Generic *inference* — that is `relations`-adjacent but distinct
  ([solver-inference](solver-inference.md)). The relation engine consumes
  already-instantiated types.
- Type *construction/interning*. The engine reads `TypeData` via the
  `TypeDatabase` trait and never constructs raw `TypeKey`s for policy.

## Module map

| Path | Role |
| --- | --- |
| `relations/mod.rs` | Re-exports the public submodules (`compat`, `freshness`, `judge`, `lawyer`, `relation_queries`, `subtype`, `variance`). |
| `relations/relation_queries.rs` | The checker-facing query boundary: `RelationKind`, `RelationPolicy`, `query_relation*`, `query_assignability_with_failure_analysis`. |
| `relations/judge.rs` | `DefaultJudge` — high-level *classifier* queries (`classify_iterable`, `classify_callable`, `get_property`, `is_subtype`) over a `SubtypeChecker`. |
| `relations/lawyer.rs` | `AnyPropagationRules` — the `any` propagation mode selector (`All` / `TopLevelOnly` / `AnySourceNotRelated`). |
| `relations/compat.rs` | `CompatChecker` — the Lawyer. `is_assignable`, `is_assignable_strict`, `explain_failure`, excess-property + weak-type + Object-interface logic. |
| `relations/compat_weak.rs` | Weak-type detection helpers (`is_weak_type`, `violates_weak_type`, `violates_weak_union`). |
| `relations/compat_mapped.rs` | Mapped-type-specific assignability shortcuts used by the Lawyer. |
| `relations/compat_overrides.rs` | Enum/abstract-constructor/private-brand nominality overrides. |
| `relations/freshness.rs` | `is_fresh_object_type`, `widen_freshness` — `ObjectFlags::FRESH_LITERAL` tracking for excess-property checking. |
| `relations/variance.rs` | `compute_variance`, `VarianceComputer`, `run_application_variance_arg_loop`. |
| `relations/subtype/core.rs` | The `SubtypeChecker` struct, its flags, construction, `reset`, and lazy-resolution helpers. |
| `relations/subtype/cache.rs` | `check_subtype` — fast paths, cross-checker memoization, cycle/`DefId`/`SymbolId` detection, fuel guard, the `maybe_keys` protocol. |
| `relations/subtype/core_dispatch.rs` | `check_subtype_inner_impl` — the big structural dispatch by type kind. |
| `relations/subtype/visitor.rs` | `SubtypeVisitor` — visitor-based traversal for the target side. |
| `relations/subtype/rules/` | Category rules: `objects.rs`, `functions/`, `unions.rs`, `tuples.rs`, `literals.rs`, `generics.rs`, `conditionals.rs`, `intrinsics.rs`, `mapped_*`. |
| `relations/subtype/overlap.rs` | `are_types_overlapping` — the Overlap relation (TS2367). |
| `relations/subtype/explain.rs`, `explain_function.rs`, `explain_tuple.rs` | The failure-explanation "slow path" that re-runs the walk to build `SubtypeFailureReason`. |

## Relation kinds

`RelationKind` (`relation_queries.rs`) enumerates the relations the boundary
serves:

| Kind | Layer | Meaning |
| --- | --- | --- |
| `Assignable` | Lawyer | TypeScript assignability — `CompatChecker::is_assignable_with_overrides`. The default for `TS2322`/`TS2345`/`TS2416`. |
| `AssignableBivariantCallbacks` | Lawyer | Assignability with `strict_function_types` forced off so callback params compare bivariantly. |
| `Subtype` | Judge | Strict structural subtyping — `SubtypeChecker::is_subtype_of`. Used by union/overload reduction and for soundness-sensitive checks. |
| `Overlap` | Judge | Non-empty-intersection check — `SubtypeChecker::are_types_overlapping`. Backs TS2367 ("no overlap"). |
| `RedeclarationIdentical` | Lawyer | Bidirectional structural identity for `var`/parameter redeclaration (TS2403) — `are_types_identical_for_redeclaration`. |

`query_relation_with_resolver` dispatches on the kind and builds the matching
checker via `configured_compat_checker` / `configured_subtype_checker`, applying
the `RelationPolicy`. The **Comparable** relation that TypeScript uses for
`==`/`switch` (and assertion narrowing) is not a `RelationKind`; it lives
separately as `types_are_comparable` in `type_queries/flow.rs`, built on top of
the overlap and assignability primitives — see
[solver-narrowing](solver-narrowing.md).

### `RelationPolicy` and cache honesty

`RelationPolicy` is the checker-visible bundle describing one query. Its
critical contract: *every field that affects whether the relation holds must
also be encoded in the `RelationCacheConfig` returned by
`RelationPolicy::cache_config`.* Fields that only change the error message (not
the boolean) are deliberately kept out of the cache key. The behavior-affecting
booleans live in a typed `RelationFlags` bitset (`strict_null_checks`,
`strict_function_types`, `exact_optional_property_types`,
`no_unchecked_indexed_access`, `disable_method_bivariance`, `erase_generics`
via `NO_ERASE_GENERICS`, `skip_weak_type_checks`, `strict_subtype_checking`,
`strict_any_propagation`). A documented regression note in `from_flags` warns
against re-coupling independent options: an earlier version mistakenly inferred
`strict_any_propagation` from `STRICT_FUNCTION_TYPES`, and a regression test
(`strict_function_types_does_not_imply_strict_any`) now pins them apart.

```text
checker call site
      │  RelationPolicy + RelationKind
      ▼
relation_queries::query_relation_with_resolver
      │
      ├── Assignable / AssignableBivariantCallbacks / RedeclarationIdentical
      │        └── CompatChecker  (Lawyer)
      │               └── SubtypeChecker  (Judge)   ← structural kernel
      └── Subtype / Overlap
               └── SubtypeChecker  (Judge)
```

## The Lawyer: `CompatChecker::is_assignable`

The assignability entry is `CompatChecker::is_assignable(source, target)`
(`compat.rs`). It runs a fast identity check, an unstrict-null shortcut
(`!strict_null_checks && target.is_nullish() => true`), an operation-local cache
lookup keyed by `(source, target)`, then delegates to `is_assignable_impl`. The
cache here is a plain `FxHashMap<(TypeId, TypeId), bool>` cleared whenever a
policy-affecting setter runs (`set_strict_function_types`,
`set_strict_null_checks`, `set_exact_optional_property_types`, etc.), so it can
never serve a stale verdict across a configuration change.

`is_assignable_impl` is where the Lawyer's quirks live, in order:

1. **Normalize operands** — `normalize_assignability_operands` runs a bounded
   (≤8 iteration) resolve/evaluate loop over `Lazy(DefId)`, `Mapped`,
   `Application`, and `KeyOf` so both sides are in a comparable shape.
2. **Fast path** — `check_assignable_fast_path` handles `source == target`,
   direct union containment (`S <: S | U`), `any`/`unknown`/`never`/error tops
   and bottoms (with the tsc rule `any` is NOT assignable to `never`), and
   non-strict null/undefined. The `any` handling consults
   `lawyer.any_source_not_related` and `lawyer.allow_any_suppression` to decide
   whether to short-circuit or fall through to structural checking.
3. **Enum nominality** — `enum_assignability_override` distinguishes
   `EnumA.Member` from `EnumB.Member` even when both are `0` (see
   `compat_overrides.rs`).
4. **Weak types** — `violates_weak_union` / `violates_weak_type` (TS2559),
   skipped when `skip_weak_type_checks` is set.
5. **Excess properties** — `check_excess_properties` (TS2353) on fresh object
   literals.
6. **Empty-object / Object-interface / Function-interface** escape hatches.
7. **Homomorphic-mapped shortcuts** — `S` already satisfies
   `{ [K in keyof S]+?: S[K] }` etc.
8. **TS2859 complexity guard** — if the evaluated constituent-count cross-product
   exceeds `1_000_000` and the pair is not trivially related, it marks the guard
   exceeded and returns `false` (mirrors tsc's overflow on huge union/intersection
   comparisons).
9. **Fall through to the Judge** — `configure_subtype(strict_function_types)`
   then `self.subtype.is_subtype_of(source, target)`.

### The `any` propagation modes

`any` is TypeScript's "black hole": both top and bottom. The Lawyer models the
policy with `AnyPropagationRules` (`lawyer.rs`), which projects to an
`AnyPropagationMode` (`subtype/core.rs`):

| Mode | `any` source matches | `any` target accepts | Selected when |
| --- | --- | --- | --- |
| `All` | every depth | every depth | default (legacy TS) |
| `TopLevelOnly` | only at depth 0 | only at depth 0 | Sound Mode / identity checks |
| `AnySourceNotRelated` | never | every depth | overload subtype pass (tsc `chooseOverload` with `subtypeRelation`) |

`AnyPropagationMode::allows_any_source_at_depth(depth)` /
`allows_any_target_at_depth(depth)` are consulted at the top of
`check_subtype`. When an `any` source is *not* allowed at the current depth, the
checker demotes it to the reserved `TypeId::STRICT_ANY`, which only matches the
top types — this is how the identity relation (`with_identity_check_mode`)
correctly rejects `number <: any` at nested positions while still treating
top-level `any` permissively.

## The Judge: structural subtype kernel

The Judge is `SubtypeChecker<'a, R: TypeResolver>` (`subtype/core.rs`). It is a
large struct because TypeScript's relation has many context-dependent knobs; the
notable flags:

- `strict_function_types` — contravariant function params when true.
- `allow_void_return` — `() => T` matches `() => void`.
- `allow_bivariant_rest` — `any`/`unknown` rest params are bivariant
  (TypeScript issue 20007).
- `disable_method_bivariance` — methods normally bivariant; this forces
  contravariance for soundness paths.
- `erase_generics` — non-generic functions may match generic targets by erasing
  the target's type params to their constraints (tsc `eraseGenerics`).
- `exact_optional_property_types`, `no_unchecked_indexed_access`,
  `strict_null_checks` — compiler-option mirrors.
- `any_propagation` — the mode above.
- `assume_related_on_cycle` — whether cycles/overflow are assumed-related
  (`true`, tsc `Ternary.Maybe`) or definitive `false`.
- `enforce_weak_types` / `in_property_check` / `in_intersection_member_check` —
  weak-type propagation state threaded from the Lawyer.
- `in_callback_param_check`, `in_bivariant_callback_return_check`,
  `force_strict_callback_param_variance` — tsc `SignatureCheckMode.Callback`
  parity bits for nested callback comparisons.

The public entry is `is_subtype_of` (`subtype/helpers.rs`), a thin wrapper over
`check_subtype(source, target).is_true()`. `is_assignable_to` on the
`SubtypeChecker` is an alias for `is_subtype_of` — the *strict structural*
check, distinct from the Lawyer's `CompatChecker::is_assignable`.

### `SubtypeResult` and coinduction

The walk returns a four-valued `SubtypeResult` (`subtype/core.rs`):

| Variant | Meaning | `is_true()` |
| --- | --- | --- |
| `True` | Definitely related. | yes |
| `False` | Definitely not related. | no |
| `CycleDetected` | A valid coinductive cycle (e.g. `interface List { next: List }`). | yes |
| `DepthExceeded` | Expansive recursion / fuel overflow (`type T<X> = T<Box<X>>`). | yes |

Both `CycleDetected` and `DepthExceeded` count as `true`. This is the crux of
tsc parity: when the relation checker cannot decide within its limits, it
assumes the types *are* related (`Ternary.Maybe`), which prevents spurious
`TS2344`/`TS2322` errors on recursive generic constraints. `cycle_result()` and
`depth_result()` flip these to `False` only when `assume_related_on_cycle` is off.

### `check_subtype` — the cache + cycle gate

`check_subtype` (`subtype/cache.rs`) is the outer wrapper around the structural
dispatch. It runs, in order:

1. **`any`/depth demotion** — compute `allow_any_source`/`allow_any_target`
   from the propagation mode; demote disallowed `any` to `STRICT_ANY`.
2. **Trivial fast paths** — `source == target` ⇒ `True`; type-parameter
   equivalences (alpha-renaming, below); intrinsic disjointness (two concrete
   primitives in id range 8..=13 ⇒ `False`); `any`/`unknown`/`never`/error
   tops and bottoms; `unknown == {} | null | undefined` decomposition; disjoint
   unit types (distinct literals/unique symbols ⇒ `False`).
3. **Cross-checker memoization** — `QueryDatabase::lookup_subtype_cache_value`
   keyed by a `RelationCacheKey`. Three values: `True`, `False`, and the
   budget-conditional `LimitTrue { fuel_band }` (honest only when the current
   `remaining_global_subtype_fuel() <= fuel_band`). Skipped under
   `identity_cycle_check` (the key cannot encode identity mode) and for
   context-dependent pairs (polymorphic `this`, class-check context), which use
   the instance-local `local_relation_cache` instead.
4. **Structural-identity fast path** — `QueryDatabase::canonical_id(source) ==
   canonical_id(target)` ⇒ `True` (graph-isomorphism canonicalization, after
   the cheap cache check).
5. **Nominal heritage fast path (#13935)** — for `Lazy(DefId) <: Lazy(DefId)`,
   `nominal_heritage_subtype` decides by registered heritage edges *before*
   evaluation materializes the base's members (the lever for relation-saturated
   DOM/lib interfaces like `Worker <: EventTarget`).
6. **Global fuel guard** — `crate::limits::enter_subtype_frame()` consumes from
   `MAX_GLOBAL_SUBTYPE_FUEL` (10,000) and snapshots two cache-poisoning sentinel
   counters (unresolved-`Lazy` failures and weak-type sensitivity).
7. **Cycle detection** — three layers, all *before* evaluation (see below).
8. **Pre-evaluation intrinsic checks** — Object-interface target (any
   non-nullable source is assignable, via `check_object_contract`) and
   Function-interface target (any callable source), checked before
   `evaluate_type` loses the `DefId` identity.
9. **Dispatch** — finally `check_subtype_inner` → `check_subtype_inner_impl`.

Every exit after the recursion guard is entered routes through the
`finish_frame!` macro (the `maybe_keys` protocol) and `leave_global!`.

### Three layers of cycle detection

Because expansive recursive types mint fresh `TypeId`s on every evaluation,
cycle detection must run *before* evaluation and at multiple identity levels
(`subtype/cache.rs`):

1. **`TypeId`-pair guard** — `self.guard` (a
   `RecursionGuard<(TypeId, TypeId)>`, profile `SubtypeCheck`). It checks the
   reversed pair too (`(target, source)`) for bivariant cross-recursion. On
   `RecursionResult::Cycle` it returns `result_on_cycle`; on `DepthExceeded` /
   `IterationExceeded` it returns `depth_result`.
2. **`DefId`-pair guard** — `self.def_guard`
   (`RecursionGuard<(DefId, DefId)>`) catches cycles in `Lazy(DefId)` /
   `Enum(DefId)` / `Application` base `DefId`s before they expand. It skips when
   both sides are `Application`s of the *same* base `DefId` (e.g.
   `Box<number>` vs `Box<string>` are a legitimate comparison, not a cycle) —
   unless they are a conditional-alias self-comparison with identical args.
3. **`SymbolId`-pair detection** — the same interface (e.g. `Promise`) can get
   different `DefId`s in lib vs user files. The engine resolves each `DefId` to
   its underlying `SymbolId` and treats a `(SymbolId, SymbolId)` pair already in
   flight under a *different* `DefId` pair as a cycle, in both forward and
   reversed orientation. This is what prevents `Promise` vs `PromiseLike`
   recursion from exploding.

The `RecursionGuard` profile `SubtypeCheck` (`recursion.rs`) bounds depth at
`MAX_SUBTYPE_DEPTH` = 100 and iterations at 100,000. Distinct from the global
fuel: depth/iterations are per-checker-instance; fuel is a cross-instance TLS
budget that bounds the combined `evaluate → subtype → instantiate → evaluate`
cycle no per-instance guard can see (issue 7574). `check_subtype_inner` also
wraps the dispatch in `crate::recursion::with_solver_frame` (and
`stacker::maybe_grow`) so deep but legitimate walks grow the stack instead of
overflowing.

## Structural dispatch by type kind

`check_subtype_inner_impl` (`subtype/core_dispatch.rs`) is the big switch over
the *source* type kind, with the source-union / source-intersection ordering
constraints handled inline and the rest delegated to the `SubtypeVisitor`
(target side) and the category rules. The ordering is load-bearing:

```text
check_subtype_inner_impl(source, target)
  1. readonly-application / display-alias peel  (Readonly<X> <: T  →  X <: T)
  2. non-strict-null source shortcut
  3. nominal object-shape DefId identity
  4. primitive → boxed wrapper  (string <: String, "x" <: String)
  5. apparent primitive shape   (string structurally <: { length: number, ... })
  6. conditional source / conditional target  (branch decomposition)
  7. SOURCE union   →  ALL members must be <: target   (must precede target union!)
  8. TARGET union   →  source <: SOME member
       (with: keyof-subset, numeric-enum widening, identity prescan,
        array-element fast path, type-param constraint, string-intrinsic
        constraint, template projection, distributive intersection factoring,
        discriminated-union matching, enum decomposition)
  9. → SubtypeVisitor (objects, functions, tuples, arrays, literals, generics …)
```

The source-union rule (step 7) **must** run before the target-union rule
(step 8): `(A | B) <: (C | D)` means *every* source member is `<:` the whole
target union, whereas `S <: (C | D)` means `S <:` *some* member. Getting the
order wrong inverts union semantics.

The target-union path is unusually dense because TypeScript accepts many
non-obvious union assignments: a `keyof` source against
`string | number | symbol`; an open numeric enum's `number` against a union
containing that enum; the *distributive intersection factoring* rule
(`S <: (A & S) | (B & S)` reduces to `S <: A | B`); and tsc's
`typeRelatedToDiscriminatedType` for discriminated unions, where a source with
discriminant properties is matched against the union member whose discriminant
agrees, with a narrowed source.

### The `SubtypeVisitor`

`SubtypeVisitor` (`subtype/visitor.rs`) implements the `TypeVisitor` trait and
dispatches on the *target* `TypeData` kind (`visit_object`, `visit_function`,
`visit_tuple`, `visit_intersection`, `visit_type_parameter`, etc.). Each
`visit_*` hands off to the matching rule module:

| Target kind | Rule module | Key function(s) |
| --- | --- | --- |
| Object / ObjectWithIndex | `rules/objects.rs` | `check_object_subtype`, `check_property_compatibility` |
| Function / Callable | `rules/functions/` | `check_function_subtype_impl`, `are_parameters_compatible_impl` |
| Tuple | `rules/tuples.rs` | element/rest alignment |
| Union / Intersection | `rules/unions.rs` | member iteration, intersection merge |
| Literal | `rules/literals.rs` | literal-vs-primitive widening |
| Generic `Application` | `rules/generics.rs`, `generics_application_helpers.rs` | variance fast path, instantiation fallback |
| Conditional | `rules/conditionals.rs` | branch decomposition |
| Mapped | `rules/mapped_*.rs` | homomorphic mapped comparison |
| Intrinsic / boxed | `rules/intrinsics.rs`, `rules/intrinsic_object.rs` | `boxable_intrinsic_kind` |

Object property compatibility (`check_object_subtype` /
`check_property_compatibility`) is the structural workhorse: it walks the
target's properties, finds each in the source (`lookup_property`), and checks
optionality, readonly, visibility, and the recursive property-type relation. A
missing required target property yields `MissingProperty`/`MissingProperties`;
an incompatible present property yields `PropertyTypeMismatch`.

## Variance

Generic assignability does not always require fully relating the instantiated
bodies. If `C<T>` uses `T` only covariantly, `C<Dog> <: C<Animal>` reduces to
`Dog <: Animal`. The `Variance` bitflag (`types.rs`) and `relations/variance.rs`
implement this.

`Variance` is a `u8` bitset, not a four-value enum, so it can encode both the
variance position and several reliability caveats:

| Flag | Meaning |
| --- | --- |
| `COVARIANT` | covariant position (return types, array elements) |
| `CONTRAVARIANT` | contravariant position (parameters) |
| `NEEDS_STRUCTURAL_FALLBACK` | mapped-type modifiers (`+?`/`-?`/`readonly`) can break the shortcut; fall through to structural |
| `REJECTION_UNRELIABLE` | a variance *failure* may be wrong (indexed-access / intersection normalization can make differing args equivalent) |
| `DIRECT_USAGE` | param found in a direct (non-mapped) position; makes a `NEEDS_STRUCTURAL_FALLBACK` rejection trustworthy |

The classifiers `is_covariant`, `is_contravariant`, `is_invariant`
(both bits set), and `is_independent` (neither bit set) derive the four logical
cases. `compute_variance` / `VarianceComputer` traverse a generic body tracking
*polarity* — positive (covariant) for returns and array elements, negative
(contravariant) for parameters, *both* (invariant) for mutable props with
divergent read/write types — using `(TypeId, Polarity)` cycle keys so recursive
types like `type List<T> = { head: T; tail: List<T> }` compute correctly.

The actual per-argument walk is centralized in `run_application_variance_arg_loop`
(`variance.rs`), the *single source of truth* shared by both Application fast
paths (`relation_queries::check_application_variance` at the boundary and
`SubtypeChecker::try_variance_fast_path` in the engine), so they cannot drift on
argument orientation:

```text
for each declared variance[i]:
   invariant     →  arg_related(s_arg, t_arg)  AND  arg_related(t_arg, s_arg)
   covariant     →  arg_related(s_arg, t_arg)
   contravariant →  arg_related(t_arg, s_arg)         (reverse only)
   independent   →  skipped
```

Each caller still owns the *relation* used to relate two argument types — the
boundary uses the Lawyer `CompatChecker::is_assignable`, the engine uses the raw
Judge `check_subtype` — and its own accept/reject/fall-through verdict; only the
variance *walk* is shared. Declared variance per `DefId` is memoized in a
universe-shared interner store (`shared_def_variance`) and a per-checker session
cache, because variance is a pure function of the (lazy-ref-heavy) generic body
and is otherwise recomputed on every type reference that validates its args.

### Function variance in detail

Function comparison (`rules/functions/mod.rs`,
`are_parameters_compatible_impl`) is where variance gets subtle:

- **Strict mode** (`strict_function_types` true, non-method): parameters are
  **contravariant** — `target_param <: source_param`
  (`check_subtype_from_parameter_compare(target, source)`).
- **Legacy mode** (`strict_function_types` false): parameters are **bivariant** —
  contravariant *or* covariant succeeds.
- **Methods**: bivariant *regardless* of `strict_function_types`, unless
  `disable_method_bivariance` is set. `method_should_be_bivariant = is_method &&
  !disable_method_bivariance`.
- **Callback parameters**: when both param types are themselves callable and the
  outer slot is method-bivariant, tsc enters `SignatureCheckMode.Callback`:
  the callback's *own* parameters become strict (no method-bivariance
  loosening). This is the `in_callback_param_check` /
  `force_strict_callback_param_variance` machinery. Crucially, tsc strips
  `null`/`undefined` before deciding callback-ness
  (`getSingleCallSignature(getNonNullableType(t))`), so a parameter typed
  `((value: T) => R) | undefined` — exactly `Promise.then`'s `onfulfilled` — is
  still treated as a strict callback; the engine mirrors this with
  `remove_nullish` and a nullability-agreement gate.
- **Return types**: covariant, with the **void exception** — when
  `allow_void_return` is on and the target returns `void`, any source return is
  accepted (`check_return_type_compatibility`), because the caller promises to
  ignore the value.

## Weak types, freshness, and nominal brands (Lawyer-only)

These three TypeScript quirks are owned by the Lawyer and make types *less*
compatible than pure structure would.

**Weak types (TS2559).** A *weak type* (`compat_weak.rs::is_weak_type`) is an
object type with at least one property, all properties optional, and no
call/construct/index signatures (for an intersection, *all* members must be
weak). Assigning to a weak type requires at least one **common property** with
the source; otherwise `violates_weak_type` returns true and the assignment is
rejected. `violates_weak_union` extends this to weak-type union members. This
guards against typo'd config objects. The weak check is skipped when
`skip_weak_type_checks` is set, matching tsc's `isTypeAssignableTo` (which does
*not* run the weak check — tsc only applies it at specific diagnostic sites like
variable declarations and argument passing), and the engine threads
`enforce_weak_types` into nested structural comparisons so the policy reaches
deep object-to-object checks.

**Freshness / excess properties (TS2353).** Object *literals* are interned with
`ObjectFlags::FRESH_LITERAL`; `freshness.rs::is_fresh_object_type` reads that
flag. While fresh, a literal may not carry properties absent from the target
(unless the target has a matching index signature or is empty) —
`find_excess_property_in` (`compat.rs`) implements the search. Once a literal is
assigned to a variable it loses freshness via `widen_freshness` /
`widen_freshness_deep`, which removes the flag recursively through property types
(mirroring tsc's `getRegularTypeOfObjectLiteral`) and carries forward display
properties/aliases so the widened `TypeId` still renders nicely.

**Nominal brands.** `compat_overrides.rs` enforces the few places TypeScript is
nominal: enum members (`EnumA.A` ≠ `EnumB.A` even at value `0`), classes with
private/protected members (the "private brand" — same shape, separate
declarations ⇒ incompatible, but subclasses inherit the brand), and constructor
accessibility (TS2673/TS2674). These can require binder/symbol context the
solver lacks, so the checker injects them through the
`AssignabilityOverrideProvider` trait (`enum_assignability_override`,
`abstract_constructor_assignability_override`,
`constructor_accessibility_override`), defaulting to `NoopOverrideProvider`.

## Caches and invariants

| Cache | Scope / owner | Key | Invalidation |
| --- | --- | --- | --- |
| Cross-checker relation cache | shared `QueryDatabase` | `RelationCacheKey` (source, target, behavior flags, `this_context`) | dropped with the query DB; never written for undetermined or context-dependent results |
| `local_relation_cache` | one `SubtypeChecker` instance | `RelationCacheKey` | cleared by `reset`; for polymorphic-`this` / class-check pairs only |
| `eval_cache` | one `SubtypeChecker` instance | `(TypeId, no_unchecked_indexed_access)` | cleared by `reset`; memoizes `evaluate_type` results + stability |
| `CompatChecker::cache` | one `CompatChecker` | `(TypeId, TypeId)` | cleared by every policy-affecting setter |
| `DefaultJudge::subtype_cache` / `eval_cache` | one `DefaultJudge` | `(TypeId, TypeId)` / `TypeId` | `clear_caches` |
| `shared_def_variance` + session variance cache | interner / `QueryDatabase` | `DefId` | universe-stable; variance is a pure function of the def body |

Several **cache-honesty invariants** are essential to tsc parity:

- **Undetermined `False` is never cached.** When `resolve_lazy_type` finds a
  `Lazy` whose body is not yet registered (re-entrant lib resolution), it calls
  `note_lazy_resolve_failure`; `check_subtype` snapshots the failure counter on
  frame entry and refuses to cache a `False` whose computation observed an
  unresolved `Lazy`.
- **Weak-sensitivity is never cached across enforcement states.** The
  flag-agnostic `RelationCacheKey` cannot encode weak-type enforcement, so a
  result whose computation read `note_weak_type_sensitivity` is excluded.
- **Context-dependent pairs stay local.** A pair carrying polymorphic `this`, or
  checked inside a class-check context, depends on the resolver's current `this`
  binding and the `is_class_symbol` closure — neither is in the key — so it uses
  the instance-local memo, valid only for the checker's lifetime (one top-level
  query). When a concrete `this` binding *is* available, `make_cache_key`
  discriminates the shared key by it (`RelationCacheKey::this_context`,
  issue 13828) so the pair can safely re-enter the shared cache.
- **`maybe_keys` (tsc `maybeKeys` parity, issue 13241).** A
  `CycleDetected`/`DepthExceeded` verdict is a *Maybe*: it is recorded in the
  `maybe_keys` stack and only **promoted** to a definitive cache entry when the
  *outermost* frame of the checker instance completes successfully. On a
  definitive `False`, `finish_relation_frame` truncates the stack to the frame's
  entry watermark, discarding Maybe entries that depended on the now-invalidated
  assumption. A fuel-limit Maybe is promoted to `LimitTrue { fuel_band }` only
  when the whole budget chain was *pristine* at entry (full global fuel, fresh
  per-instance iteration budget, no enclosing solver frames), so any later query
  holds an equal-or-smaller budget and reusing the assumed-related verdict is
  monotonically safe ("fuel-band honesty").

## Failure reasons and the assignability gateway

A boolean `false` is not enough to produce a TypeScript diagnostic. The engine
produces a structured `SubtypeFailureReason` (`diagnostics/core.rs`), a deeply
nested enum whose variants map directly to tsc message families:

`MissingProperty`/`MissingProperties` (TS2741/TS2739), `PropertyTypeMismatch`
(with a `nested_reason` box for the recursive chain), `ReturnTypeMismatch`,
`ParameterTypeMismatch` (with `inner_reason`), `TupleArityMismatch` /
`TupleElementTypeMismatch` / `TupleVariadicPositionMismatch`
(TS2618–TS2627), `ArrayElementMismatch`, `IndexSignatureMismatch`,
`NoUnionMemberMatches`, `NoCommonProperties` (the weak-type reason),
`ExcessProperty` (TS2353), `ReadonlyToMutableAssignment` (TS4104),
`IndexAccessTypeParameterMismatch` (TS5075), `AbstractConstructorAssignment`
(TS2517), and the generic fallbacks `TypeMismatch`/`IntrinsicTypeMismatch`/
`LiteralTypeMismatch`.

These are built by the **explain "slow path"** — `subtype/explain.rs`,
`explain_function.rs`, `explain_tuple.rs`, and `CompatChecker::explain_failure`.
The boolean walk runs first and decides `false`; the explain pass then *re-runs*
the structural logic to find *why*. Because it re-runs the relation, it can be
unboundedly expensive on pathological generics, so it carries its own work
budget: `EXPLAIN_EVAL_BUDGET` = 16,000 units (`explain.rs`), loaded into
`explain_eval_fuel` only at the outermost `explain_failure` entry (issue 13243).
The boolean relation walk is never budgeted by this fuel — `Some(_)` doubles as
the "in explain" marker — so the elaboration can collapse to a coarse
`TypeMismatch` on overflow without ever altering the diagnostic produced on
terminating inputs.

The single-pass entry `analyze_weak_and_explain`
(used by `query_assignability_with_failure_analysis` in `relation_queries.rs`)
computes the weak-type classification *once* and derives both the
`weak_union_violation` boolean (which routes TS2559 vs TS2322/TS2741 at the
checker) and the failure reason from the same probe, instead of running the two
weak probes twice. The reason walk runs on the *same* `CompatChecker` whose
relation cache is already warm with the decision's sub-results, so the
explanation can never contradict the verdict.

This whole production feeds the shared assignability gateway: the checker phrases
a relation, the gateway runs **relation → structured reason → diagnostic** for
`TS2322`/`TS2345`/`TS2416`. The relation engine owns the first two steps; the
gateway and error reporter own the last. See
[checker-assignability-gateway](checker-assignability-gateway.md) and
[checker-error-reporter-diagnostics](checker-error-reporter-diagnostics.md).

## Worked example 1: `let x: { a: number } = { a: 1, b: 2 }`

The checker phrases an `Assignable` relation, `source = { a: 1, b: 2 }` (a fresh
object literal), `target = { a: number }`.

```text
CompatChecker::is_assignable(source, target)
  → cache miss → is_assignable_impl
      normalize_assignability_operands       (no Lazy/Mapped; unchanged)
      check_assignable_fast_path             → None (need full check)
      enum_assignability_override            → None
      violates_weak_union / violates_weak_type → false (target not weak)
      check_excess_properties(source, target)
          find_excess_property_in: source is FRESH_LITERAL,
            target props = { "a" };  source prop "b" ∉ target,
            no index signature  → Some("b")  → returns false
  → is_assignable = FALSE
```

The relation fails on excess properties. The gateway then asks for a reason; the
Lawyer's `find_excess_property` returns `ExcessProperty { property_name: "b" }`,
which renders as TS2353. Note the *structural* part (`{ a: 1, b: 2 } <:
{ a: number }`) would have *passed* — freshness is what rejects it, and only the
Lawyer (not the Judge) knows about freshness.

## Worked example 2: `Box<Dog> <: Box<Animal>` with covariant `Box`

`Box<T> = { readonly value: T }`, `source = Box<Dog>`, `target = Box<Animal>`,
`Dog <: Animal`.

```text
check_subtype(Box<Dog>, Box<Animal>)
  fast paths: not equal, not any/unknown/never
  QueryCache lookup → miss
  canonical_id(source) != canonical_id(target)
  both Application, same base DefId (Box)  → both_same_base_app = true
       → def_pair = None (legitimate Box-vs-Box comparison, not a cycle)
  enter fuel frame, enter TypeId guard
  → check_subtype_inner_impl → SubtypeVisitor / generics rule
       try_variance_fast_path(Box, [Dog], [Animal]):
         compute_def_variances(Box) → [COVARIANT]   (value is read-only)
         run_application_variance_arg_loop:
            variance[0] covariant → arg_related(Dog, Animal)
                                  → check_subtype(Dog, Animal) → True
       → True
```

Variance short-circuits the body comparison: because `Box`'s sole parameter is
covariant, the relation reduces to `Dog <: Animal` without ever walking the
`{ readonly value: ... }` shape. The variance mask for `Box` is computed once
and cached in `shared_def_variance`, so every subsequent `Box<_> <: Box<_>`
relation reuses it.

## Edge cases and tsc parity

- **`any` is not assignable to `never`.** Both `check_subtype` and
  `check_assignable_fast_path` special-case `any → never` as `false`, mirroring
  tsc's `isSimpleTypeRelatedTo` (`if (s & Any) return !(t & Never)`).
- **`unknown == {} | null | undefined`.** `unknown` is assignable to a union
  covering all three constituents — handled both at the Lawyer
  (`empty_object_with_nullish_target`) and in the Judge fast path for nested
  checks that bypass the Lawyer.
- **Error types short-circuit to `true`.** Any `error` source or target is
  assignable in both directions, to stop cascading diagnostics (tsc `errorType`).
- **Disjoint unit types fail fast.** Distinct string/number/bigint/boolean
  literals and distinct `UniqueSymbol`s are `false` by identity — but two
  `Literal`s with the *same* value and different `TypeId`s (interned from
  different contexts, e.g. JSDoc vs expression) are equal.
- **Labeled tuples are not disjoint unit types.** `is_disjoint_unit_type`
  deliberately excludes tuples: `[a: 1]` and `[b: 1]` have different `TypeId`s
  but tsc treats them as compatible.
- **Primitive ↔ boxed wrapper.** `string <: String`, `"x" <: String`, etc. are
  accepted via `boxable_intrinsic_kind` *before* the apparent-shape comparison
  (whose structural shape would otherwise mismatch), but primitives are *not*
  assignable to a pure index-signature type or to `object`.
- **Open numeric enums.** `number` is assignable to a union containing a numeric
  enum (the target-union path consults `resolver.is_numeric_enum`).
- **Method bivariance vs callback strictness.** Methods are bivariant even under
  `strict_function_types`; but a *callback parameter* of a method is compared
  strictly (`SignatureCheckMode.Callback`), with `null`/`undefined` stripped
  before deciding callback-ness so `Promise.then`'s nullable callback still gets
  strict variance.
- **`() => void` accepts `() => T`.** The void-return exception
  (`allow_void_return`) lets any return type satisfy a `void` target return.
- **Recursion overflow is `true`, not `false`.** `DepthExceeded` from the
  100-deep `RecursionGuard`, the 100,000-iteration cap, or the 10,000-unit global
  fuel all resolve as `Ternary.Maybe`-style `true` when
  `assume_related_on_cycle` is set, so a pathological recursive constraint does
  not manufacture a spurious `TS2322`/`TS2344` — but the verdict is only cached
  as a budget-conditional `LimitTrue`, never an unconditional `True`.
- **TS2859 complexity.** A constituent cross-product over `1_000_000` marks the
  guard exceeded and returns `false` unless the pair is trivially related; the
  checker reads `iteration_exceeded()` vs `depth_exceeded()` to choose TS2859
  ("excessive complexity") vs TS2321 ("excessive stack depth").

## See also

- [checker-assignability-gateway](checker-assignability-gateway.md) — how the
  checker phrases relations and turns reasons into TS2322/TS2345/TS2416.
- [checker-calls-signatures-generics](checker-calls-signatures-generics.md) —
  overload resolution and the subtype/comparable passes it uses.
- [checker-flow-and-narrowing](checker-flow-and-narrowing.md) and
  [solver-narrowing](solver-narrowing.md) — the Comparable relation and overlap
  for `==`/`switch`/assertions.
- [solver-evaluation](solver-evaluation.md) — meta-type reduction the engine
  calls via `evaluate_type`.
- [solver-inference](solver-inference.md) — generic inference, which precedes
  the relation checks here.
- [solver-instantiation](solver-instantiation.md) — `this`-substitution and
  generic application the engine relies on.
- [solver-caches-objects-contextual-compat](solver-caches-objects-contextual-compat.md)
  — the broader cache landscape and `ObjectShape` collection.
- [solver-types-intern-def](solver-types-intern-def.md) — `TypeId`, `TypeData`,
  `DefId`, and `RelationCacheKey`.
