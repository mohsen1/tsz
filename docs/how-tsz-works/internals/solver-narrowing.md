# Narrowing and Type Guards in the Solver

Narrowing answers one family of questions for the checker's flow analysis:
*given a `TypeId` and a condition that is now known true (or false), what is the
refined `TypeId`?* The checker decides **where** a guard applies (which AST node,
which flow edge, which branch); the solver decides **what** the guard does to a
type. That split is enforced by a deliberately AST-free abstraction, the
`TypeGuard` enum: the checker extracts a `TypeGuard` from the AST, then hands a
`(source_type, guard, sense)` triple to the solver, which performs pure type
algebra and returns a narrowed `TypeId`. No `NodeIndex`, `SyntaxKind`, or
`SymbolId` ever crosses into the narrowing engine — only canonical `TypeId`
handles and interned property `Atom`s (see the module header in
`crates/tsz-solver/src/narrowing/mod.rs`).

The engine lives entirely under `crates/tsz-solver/src/narrowing`. Its public
face is `NarrowingContext<'a>` (`narrowing/core.rs`), a short-lived borrow of the
type database, an optional `TypeResolver`, and a shared `NarrowingCache`. Every
narrowing query funnels through one dispatcher, `NarrowingContext::narrow_type`,
which matches on the `TypeGuard` variant and routes to a kind-specific kernel
(`narrow_by_typeof`, `narrow_by_instance_type`, `narrow_by_discriminant_for_type`,
`narrow_by_property_presence`, the `Predicate` arm, …). This document traces how
those kernels work, how `TypeGuard` and `GuardSense` flow through the dispatch,
how the eleven caches are keyed and invalidated, how the recursion/fuel guards
bound the exclusion families, and where the engine bends pure set-theory to match
`tsc`'s legacy quirks.

Sibling internals docs cover the neighbors: the checker side of flow narrowing
([checker-flow-and-narrowing](checker-flow-and-narrowing.md)), the flow query
boundary that calls into this engine
([checker-context-and-state](checker-context-and-state.md)), the relation engine
that backs the subtype/assignability decisions narrowing leans on
([solver-relations](solver-relations.md)), evaluation of lazy/application/index
types ([solver-evaluation](solver-evaluation.md)), instantiation
([solver-instantiation](solver-instantiation.md)), inference
([solver-inference](solver-inference.md)), and the broader cache landscape
([solver-caches-objects-contextual-compat](solver-caches-objects-contextual-compat.md)).

## Owns / Must not own

**Narrowing owns:**

- The `TypeGuard` / `TypeofKind` / `GuardSense` vocabulary — an AST-agnostic
  description of every condition that can refine a type.
- The pure type algebra for each guard kind: typeof filtering, instanceof
  filtering, `in`-operator filtering, equality/literal exclusion, truthiness and
  falsy decomposition, discriminated-union filtering, user-defined predicate
  narrowing, assertion-function semantics, `Array.isArray` and `arr.every`
  narrowing, and `.constructor` identity narrowing.
- The exclusion families (`narrow_excluding_type`, `narrow_excluding_function`,
  `narrow_excluding_typeof_object`) and their per-request work budget.
- Narrowing-local semantic caches (`NarrowingCache`) plus the resolve cache that
  the checker reuses for optional-chain and property-access hot paths.

**Narrowing must not own:**

- *Where* a guard fires. The checker's flow graph picks the source `TypeId`, the
  branch sense, and the reference; the solver never inspects flow nodes.
- Extracting a guard from syntax. `extract_type_guard` /
  `extract_call_type_guard` live in
  `crates/tsz-checker/src/flow/control_flow/condition_narrowing.rs`, not here.
- Producing diagnostics. Narrowing returns a `TypeId`; downstream property
  access / assignability gateways decide whether the narrowed type triggers
  `TS2339`, `TS2322`, etc.
- The canonical relation kernels. Narrowing calls *into* the relation engine
  (`SubtypeChecker`, `is_subtype_of_with_db`) through a thin memoized boundary;
  it does not reimplement structural subtyping.

## Module map

| File | Role |
| --- | --- |
| `narrowing/mod.rs` | Module wiring and re-exports (`TypeGuard`, `GuardSense`, `NarrowingContext`, `NarrowingCache`, free helpers). |
| `narrowing/core.rs` | The dispatcher (`narrow_type` → `narrow_type_inner`), `TypeGuard`/`TypeofKind`/`GuardSense` definitions, `NarrowingContext`, `NarrowingCache`, `resolve_type`, the exclusion families and their fuel/budget. |
| `narrowing/core/helpers.rs` | The narrowing-boundary relation helpers (`is_assignable_to`, `is_subtype_for_narrowing`), enum/function narrowing, type-parameter constraint narrowing, `resolve_for_exclusion_narrowing`. |
| `narrowing/discriminants.rs` | Discriminated-union narrowing: `find_discriminants`, `narrow_by_discriminant`, `narrow_by_excluding_discriminant`, the O(1) discriminant index, property-path traversal. |
| `narrowing/instanceof.rs` | `instanceof` and `.constructor` narrowing: `narrow_by_instance_type`, `narrow_by_instanceof_false`, `narrow_by_constructor[_false]`, nominal class-relation checks. |
| `narrowing/property.rs` | `in`-operator narrowing (`narrow_by_property_presence`), property-type lookup, object-likeness checks. |
| `narrowing/compound.rs` | typeof-negation, truthiness/falsy (`narrow_by_truthiness`, `narrow_to_falsy`, `extract_definitely_falsy_type`), objectish narrowing, `Array.isArray` (`narrow_to_array`). |
| `narrowing/request.rs` | `NarrowingRequest`, `NarrowingOptions`, and the `NarrowTypeCacheKey` for predicate-guard memoization. |
| `narrowing/utils.rs` | The `NarrowingVisitor` and standalone nullish helpers (`split_nullish_type`, `remove_nullish`, `is_nullish_type`, …) plus free convenience functions. |

`narrowing/core.rs` is the one engine module that exceeds the 2000-line ceiling
informally; the `core/` subdirectory (`helpers.rs`) and the per-concern siblings
exist precisely to stage the historically monolithic engine into named pieces.

## The vocabulary: `TypeGuard`, `TypeofKind`, `GuardSense`

`TypeGuard` (`narrowing/core.rs`) is the closed set of conditions the engine can
apply. The variants and their `tsc` analogues:

| Variant | Condition | Notes |
| --- | --- | --- |
| `Typeof(TypeofKind)` | `typeof x === "string"` | `TypeofKind` is an 8-value enum (`String`/`Number`/`Boolean`/`BigInt`/`Symbol`/`Undefined`/`Object`/`Function`), parsed by `TypeofKind::parse`; non-standard typeof strings don't narrow. |
| `Instanceof(TypeId, bool)` | `x instanceof C` | The `bool` records whether the constructor was an *explicit global* `Object`/`Function` name, which only matters for aggressive false-branch narrowing. |
| `LiteralEquality(TypeId)` | `x === lit` / `x !== lit` | Equality narrows to the literal; inequality excludes it (only when the literal is a *unit type*). |
| `NullishEquality` | `x == null` / `x != null` | Loose equality matches both `null` and `undefined`. |
| `Truthy` | `if (x)` | Removes falsy members in the true branch; keeps falsy components in the false branch. |
| `Discriminant { property_path, value_type }` | `x.kind === "circle"`, `x.payload.type === "user"` | `property_path` is an interned `Vec<Atom>` so nested discriminants work. |
| `InProperty(Atom)` | `"prop" in x` | Filters by property presence (positive) / required-property absence (negative). |
| `Predicate { type_id, asserts }` | `x is T`, `asserts x is T`, `asserts x` | `type_id: None` is a bare truthiness assertion; `asserts: true` makes the false branch unreachable. |
| `Array` | `Array.isArray(x)` | Narrows to array-like types without collapsing element types. |
| `ArrayElementPredicate { element_type }` | `arr.every(isString)` | Narrows the *element* type of an array. |
| `Constructor(TypeId)` | `x.constructor === C` | Exact constructor identity — unlike `instanceof`, excludes subclasses. |

`GuardSense` is a two-valued `enum { Positive, Negative }` with a `From<bool>`
conversion; the checker passes `GuardSense::from(is_true_branch)`. Internally the
dispatcher immediately reduces it to a `bool` named `sense`.

These types are AST-free by construction. The checker builds them in
`condition_narrowing.rs`: e.g. `typeof x === "string"` becomes
`(TypeGuard::Typeof(TypeofKind::String), x_node, false)`, `x instanceof Object`
becomes `TypeGuard::Instanceof(TypeId::OBJECT, true)`, and `isString(x)` becomes
`TypeGuard::Predicate { type_id: Some(string), asserts: false }`. The third tuple
element is the *target node* and an "optional-chain" flag, both checker-side
concerns the solver never sees.

## The dispatcher

```
checker flow analysis
  │  builds TypeGuard from AST (condition_narrowing.rs)
  ▼
query_boundaries/flow_analysis.rs
  │  NarrowingContext::new(db)  +  narrow_type(type_id, guard, GuardSense)
  ▼
NarrowingContext::narrow_type            (core.rs)
  │   Predicate guards → narrow_predicate_cached (memoized)
  │   all others       → narrow_type_uncached    (uncached at this layer)
  ▼
narrow_type_uncached
  │   • preserve generic IndexAccess form
  │   • resolve IndexAccess source to concrete form
  ▼
narrow_type_inner(resolved_source, guard, sense: bool)   ← big match
  ├─ Typeof          → narrow_by_typeof / narrow_by_typeof_negation
  ├─ Instanceof      → narrow_by_instance_type / narrow_by_instanceof_false
  ├─ LiteralEquality → narrow_to_type / narrow_excluding_type
  ├─ NullishEquality → narrow_to_nullish / (exclude NULL then UNDEFINED)
  ├─ Truthy          → narrow_by_truthiness / narrow_to_falsy
  ├─ Discriminant    → narrow_by_discriminant_for_type
  ├─ InProperty      → narrow_by_property_presence
  ├─ Predicate       → (true/false/asserts arms; see below)
  ├─ Array           → narrow_to_array / narrow_excluding_array
  ├─ ArrayElementPredicate → narrow_array_element_type
  └─ Constructor     → narrow_by_constructor / narrow_by_constructor_false
```

`narrow_type` (`core.rs`) special-cases predicate guards: only
`TypeGuard::Predicate { .. }` reaches `narrow_predicate_cached`, which keys a
`NarrowTypeCacheKey` and consults `narrow_type_cache`. Every other guard kind is
re-derived on each call because its result already depends on structural lookups
cached at narrower query boundaries (discriminant index, property cache, the
resolve cache). `narrow_type_with_request` is the alloc-light entry for callers
who already hold a `NarrowingRequest` (it avoids re-cloning the guard).

Before the big `match`, `narrow_type_uncached` does one structural pre-pass that
is easy to overlook but load-bearing: if the source is a *generic* `IndexAccess`
(`A[K]` where `A` or `K` contains type parameters, via `contains_type_parameters_db`),
it remembers the original deferred form, resolves the source for narrowing, and —
if the narrowed result differs — wraps it back as `original & narrowed` rather
than leaking the eagerly-resolved constraint. This preserves assignability of the
narrowed value against the original return type (the comment cites a false
`TS2322` in `quickinfoTypeAtReturn…` otherwise).

## `resolve_type`: the gateway to structure

Narrowing constantly needs to see *through* `TypeData::Lazy(DefId)`,
`TypeData::Application`, `TypeData::IndexAccess`, and `TypeData::TypeQuery` to
reach a structural shape it can filter. `NarrowingContext::resolve_type`
(`core.rs`) is that gateway and the most-called function in the engine.

It is backed by `resolve_cache` (a `RefCell<FxHashMap<TypeId, TypeId>>`) and a
companion in-progress set `resolve_visiting`:

- A cache hit returns immediately — **unless** the cached entry is a self-mapping
  (`cached == type_id`) for a `Lazy`/`TypeQuery`, which means a *prior* resolution
  failed because the `TypeEnvironment` wasn't populated yet. Those self-mappings
  fall through and re-attempt, because a later context may have a resolver.
- Re-entry on a `type_id` already in `resolve_visiting` returns the *original*
  deferred type rather than recursing — this is the cycle guard for recursive
  `keyof` / indexed-access / conditional graphs. Returning the generic form
  preserves it and prevents stack overflow.
- The uncached body (`resolve_type_uncached`) loops with a fuel counter
  (`fuel = 100`), unwrapping `Lazy` via `resolver.resolve_lazy` (falling back to
  `db.evaluate_type`), `Application` via the resolved base body plus type
  parameters, and so on, until a fixpoint or fuel exhaustion.
- Only *real* resolutions are cached. A `Lazy → Lazy` self-mapping is **not**
  stored (`is_unresolved_symbolic`), so the environment can supply the real
  mapping on a later pass.

`resolve_for_exclusion_narrowing` (`core/helpers.rs`) is a sibling used on the
false/exclusion branches, where the engine must resolve top-level wrappers but
must *not* resolve inside, because the exclusion recursion relies on `TypeId`
identity comparisons (`narrowed == constraint`).

## Walk-through: `typeof` on a union

```typescript
function f(x: string | number | null) {
  if (typeof x === "string") {
    x; // string
  } else {
    x; // number | null
  }
}
```

True branch. The checker emits `TypeGuard::Typeof(TypeofKind::String)`,
`GuardSense::Positive`. `narrow_type_inner` takes `sense == true` and calls
`narrow_by_typeof(source, "string")` (`core.rs`). For a non-`any`/non-`unknown`
source it maps `"string"` to `TypeId::STRING` and delegates to
`narrow_to_type(source, STRING)`. `narrow_to_type` resolves the source, finds a
`Union`, and `filter_map`s each member: `string` is assignable to `STRING`
(kept), `number` and `null` are not (dropped), so the result is `string`.

False branch. `sense == false`. The dispatcher first checks the `any` escape
hatch — `tsc` does **not** narrow `any` in the false branch of `typeof`, so an
`any` source returns unchanged. Otherwise it resolves for exclusion and calls
`narrow_by_typeof_negation(resolved, "string")` (`compound.rs`), which maps
`"string"` to the excluded `TypeId::STRING` and calls
`narrow_excluding_type(source, STRING)`. That filters the union to
`number | null`.

The `"object"` and `"function"` typeof results take dedicated paths.
`typeof x === "function"` routes to `narrow_to_function` (positive) /
`narrow_excluding_function` (negative). `typeof x === "object"` narrows to
`TypeId::OBJECT` (which *includes* `null` at runtime); its negation first
excludes `null`, then excludes object-typeof members via
`narrow_excluding_typeof_object` (`compound.rs`), whose `is_typeof_object`
predicate treats `Object`/`ObjectWithIndex`/`Mapped`/`Tuple`/`Array` as
`"object"` but classifies a call-signature-bearing intersection as `"function"`.

### `any` / `unknown` parity in `narrow_by_typeof`

`narrow_by_typeof` hard-codes the `tsc` asymmetry between `any` and `unknown`:

- `any` narrows **only** for primitive typeof checks (`string`/`number`/
  `boolean`/`bigint`/`symbol`/`undefined`); `"object"` and `"function"` leave
  `any` unchanged.
- `unknown` narrows for **all** typeof checks, with `"object"` →
  `object | null` and `"function"` → the global Function type.

## Walk-through: discriminated union

```typescript
type Action =
  | { type: "add"; value: number }
  | { type: "remove"; id: string }
  | { type: "clear" };

function handle(a: Action) {
  if (a.type === "add") {
    a.value; // a is { type: "add"; value: number }
  }
}
```

The checker produces `TypeGuard::Discriminant { property_path: ["type"], value_type: "add" }`.
`narrow_type_inner` routes to `narrow_by_discriminant_for_type`
(`discriminants.rs`), which:

1. Handles type-parameter sources first: if the source is a constrained type
   parameter, it narrows the *constraint* and returns `T & NarrowedConstraint`
   when the constraint changed (`classify_for_type_parameter_constraint`).
2. Mirrors `tsc`'s `getDiscriminantPropertyAccess` gate: discriminant narrowing
   only applies when the type has the Union flag. For a top-level *intersection*
   the engine bails (returns the type unchanged) to avoid collapsing
   `RuntimeValue & { type: 'number' }` to `never` and emitting a spurious
   `TS2339` in the else branch — **unless** the intersection still contains an
   undistributed union member, in which case `try_distribute_intersection_for_narrowing`
   distributes it so the union form re-appears.
3. Dispatches to `narrow_by_discriminant` (true branch) or
   `narrow_by_excluding_discriminant` (false branch).

`narrow_by_discriminant` filters union members by reading the property at the
path and comparing to the literal. For performance it has a fast top-level path,
`fast_narrow_top_level_discriminant`, plus an O(1) **discriminant index**.

### The discriminant index

`fast_narrow_via_discriminant_index` (`discriminants.rs`) builds, once per
`(union_type, discriminant_property)` pair, a map `literal_value → Vec<member>`
stored as `Arc<DiscriminantMembers>` in `cache.discriminant_index`. Each case
clause then resolves in O(1) instead of re-scanning all members; without it, an
N-case switch over an N-member union is O(N²). The index is only built for the
**positive** path with `members.len() >= 8`, because each false-branch condition
in an `if`-chain produces a *new* sub-union with a fresh `TypeId`, so the index
would never be reused — for that case the code uses `union_excluding_one` to drop
exactly one member without a full `Vec` allocation. Members whose discriminant
property is `any` (or top-level `any`/`unknown` members) are added to *every*
bucket, since they can hold any literal value.

The constructor path is special-cased: if the discriminant property is
`"constructor"`, the property type is run through `construct_return_type_for_type`
so `x.constructor === C` compares against the *instance* type.

## Walk-through: user-defined type predicate

```typescript
function isCircle(s: Shape): s is Circle { ... }
if (isCircle(s)) { s; /* Circle */ } else { s; /* not Circle */ }
```

`TypeGuard::Predicate { type_id: Some(Circle), asserts: false }` is the most
intricate arm of `narrow_type_inner`. It follows `tsc`'s `narrowType` /
`narrowTypeByTypePredicate` logic closely:

**True branch.** After resolving source and target, it strips impossible nullish
members (`remove_impossible_nullish_for_positive_predicate`) and then:

- If the (cleaned) source equals the target → return it.
- If the source is `any` → narrow to the target, **except** when the asserted
  type is exactly the global `Object`/`Function` interface, where `any` stays
  `any` (so a following `Array.isArray` guard doesn't intersect it to `never`).
- If the source is `unknown` → return the target.
- If the source is a **union** → filter members with `narrow_to_type`, falling
  back to an intersection when nothing matches, and upgrading a collapsed empty
  object `{}` to the structurally richer target.
- If the source is a bare type parameter **and** the predicate references type
  parameters (e.g. `Extract<T, Function>`) → return the target directly (the
  intersection `T & Extract<T, Function>` would be redundant and break callability).
- Otherwise (non-union) → check `is_conditional_subtype_of_source`, the
  empty-object upgrade, then `narrow_to_type`; if the source is returned
  unchanged but isn't a clean subtype, intersect to preserve target structure.

Crucially the predicate arm uses `is_subtype_for_narrowing` (a *subtype* check),
not assignability, to decide "is the source already specific enough?" — because
`narrow_to_type`'s internal assignability is too loose for predicates (`{}` is
assignable to `Record<string, unknown>` but not a subtype).

**False branch.** `tsc`'s `getNarrowedTypeWorker(assumeTrue=false)` first computes
the true type, then *shallow-filters* the source: keep members not subset of the
true type. The engine replicates this cheap path: it computes
`positive = narrow_to_type(source, target)` and, when that reduces the source,
calls `narrow_excluding_positive_subset` (a top-level identity/containment pass —
no deep structural walk). Only when the shallow filter can't reduce the source
does it fall back to the general `narrow_excluding_type`. This shallow path is a
deliberate performance fix: the general exclusion recurses into every member with
a deep `is_assignable_to`, which explodes on recursive-schema unions (typebox /
ts-morph `value is T` guards where each nested schema instantiates to a distinct
`TypeId`, so the `(source, excluded)` memo never hits).

**Assertion functions.** For `Predicate { asserts: true }`, the false branch is
unreachable (the function throws on failure), so the engine returns the source
unchanged in the false branch and narrows normally in the true branch. A bare
`asserts x` (`type_id: None`) behaves exactly like `Truthy` in the asserted
(true) branch.

## Truthiness and falsy

`narrow_by_truthiness` (`compound.rs`) removes the JavaScript falsy values
(`null`, `undefined`, `void`, `false`, `0`, `-0`, `NaN`, `""`, `0n`) from a
type. It recurses structurally:

- **Intersections**: if any member narrows to `never`, the *whole* intersection
  is falsy → `never` (matching JS short-circuiting).
- **Unions**: filter out members that narrow to `never`.
- `boolean` → `BOOLEAN_TRUE` (true branch); `unknown` → `{}` (the non-nullish
  empty object, so subsequent property/`in` access works); `any` stays `any`.
- **Type parameters**: narrow the constraint and intersect when it changed.

`narrow_to_falsy` is the dual for the false branch. It keeps only definitely-falsy
representatives: `boolean` → `false`, but **`string`/`number`/`bigint` stay wide**
in the false branch (matching `tsc`, which does not narrow `string` to `""`).
`extract_definitely_falsy_type` is the related-but-distinct helper that mirrors
`tsc`'s `getDefinitelyFalsyPartOfType` (used for `a && b` type computation): there
`string` → `""`, `number` → `0`, `bigint` → `0n`. The two functions exist
side-by-side precisely because flow narrowing and `&&` typing want different
answers for the same primitive.

## `instanceof` and `.constructor`

`narrow_by_instance_type` (`instanceof.rs`) handles the true branch. The checker
has already extracted the *instance* type from the constructor expression, so the
solver receives the instance type directly. Key behaviors:

- If the instance type is `any` (e.g. a `new (): any` constructor signature), the
  source is returned unchanged — narrowing by `any` would make every member
  assignable and collapse to itself wrongly.
- `any` source narrows to the instance type **unless** the instance type is the
  global `Object`/`Function` interface (then it stays `any`); `unknown` source
  narrows to the instance type.
- For **unions**, members are filtered with `instanceof` semantics: primitives
  are dropped (they can never pass `instanceof`), type parameters are absorbed,
  and *class-to-class* comparisons use `nominal_instanceof_relation` (nominal
  identity / extends), **not** structural subtyping — two unrelated but
  structurally compatible classes never match.

The dispatcher (`narrow_type_inner`) layers fallbacks on top: if standard
narrowing returns `never` from a non-`never` source it tries an intersection
(when `are_instanceof_types_overlapping` holds), then `narrow_to_objectish`. An
empty-object source narrowed by `instanceof Object` returns the intrinsic
`TypeId::OBJECT` (not the Object *interface*), so a subsequent `in` check doesn't
trip `TS2638`. The false branch (`narrow_by_instanceof_false`) keeps primitives
and excludes non-primitives assignable to the instance type — for
`instanceof Object` that correctly removes every non-primitive.

`TypeGuard::Constructor` (`x.constructor === C`) routes to `narrow_by_constructor`
/ `narrow_by_constructor_false`, which match *exact* constructor identity: `C2`
narrowed by `Constructor(C1)` is `never` even when `C2 extends C1`, because
`C2.constructor !== C1`.

## The `in` operator

`narrow_by_property_presence` (`property.rs`) filters union members by property
presence. The positive branch keeps members that have the property (and drops
those without it, since `"prop" in x` being true proves `x` has `prop`); the
negative branch drops members where the property is *required*
(`is_property_required`) and keeps members where it is optional or absent.

Special sources:

- `any` / `never` pass through unchanged; `unknown` narrows to
  `object & { [prop]: unknown }` in the positive branch and stays `unknown` in
  the negative branch.
- A constrained **type parameter** narrows its constraint and intersects when
  changed; a *bare* type parameter in the positive branch is intersected with a
  synthesized `Record<prop, unknown>` (via `make_record_type`) so chained
  `"a" in x && "b" in x` checks treat the second `x` as a valid `in`-RHS — this
  is strictly more informative than `T & object` because `Record` both extends
  `object` and records the known key.

## Exclusion narrowing and its work budget

The exclusion family — `narrow_excluding_type`, `narrow_excluding_function`,
`narrow_excluding_typeof_object`, and their `narrow_type_param_excluding*`
recursions — is the engine's deepest recursion and its only one that mints fresh
types as it descends. `narrow_type_param_excluding` re-creates
`source & narrowed_constraint` at every level, so a self-referential constraint
(`T extends Foo`) presents a *different* `source` `TypeId` on each recursion, and
the per-pair in-flight guard (keyed on a *stable* `(source, excluded)`) never
fires.

To bound this, the engine carries a **per-request cumulative work budget**, the
narrowing analogue of `tsc`'s `instantiationCount` cap:

- `enter_exclusion_frame` (`core.rs`) returns a `ExclusionFrame` RAII guard that
  tracks re-entrancy depth in `narrow_excluding_depth`. The **outermost** frame
  (`depth == 0`) primes `narrow_excluding_fuel` from
  `NARROW_EXCLUDING_WORK_BUDGET` (`1_000_000`), or from
  `narrow_excluding_budget` when a test lowered it.
- `charge_exclusion_work` decrements the fuel by one per *fresh* exclusion narrow
  and returns `false` once spent; on exhaustion the recursion bails to the
  *unchanged source* — the same conservative answer the in-flight cycle guard
  gives.
- The frame restores the prior depth on every return path (including panic
  unwinds), so the budget priming stays balanced and is scoped per top-level
  request, never leaking across the many independent narrowings a shared cache
  serves.

`narrow_excluding_type_uncached` is the recursive body. It does **not** resolve
`Lazy`/`Application` (identity comparisons would break); it decomposes
`Enum(D, inner)` so exclusion runs on the inner literal union and the nominal
wrapper survives (issue #6823); it distributes intersections (any `never` member
makes the whole intersection `never`); it filters unions member-by-member; and it
special-cases `boolean` as the implicit `true | false` union so excluding one
boolean literal yields the other. The terminal case is `is_assignable_to(source,
excluded) ? never : source`.

`narrow_excluding_types` is the batched variant for switch `default` clauses:
for ≤4 excluded types it narrows sequentially; for larger sets it avoids
intermediate unions, reducing O(N²) to O(N).

## The narrowing-boundary relation helpers

Narrowing must ask the relation engine structural questions, but it does so
through two thin, memoized wrappers in `core/helpers.rs` so it never re-runs a
deep walk redundantly:

- `is_assignable_to(source, target)` — trivial answers (`source == target`,
  `source == never`, `target` is `any`/`unknown`) short-circuit inline;
  intrinsic-vs-intrinsic pairs go straight to `is_assignable_to_uncached`;
  everything else is memoized in `narrow_assignable_cache` keyed by
  `(source, target, resolver_generation)`. `is_assignable_to_uncached` layers
  class-subtype, enum-member equality, literal-to-base, `typeof "object"`
  (null/object) handling, intersection/union decomposition, and a structural
  fallback before delegating to the full subtype check.
- `is_subtype_for_narrowing(source, target)` — the single chokepoint that
  constructs a fresh `SubtypeChecker` (resolver-backed when available, else
  `is_subtype_of_with_db`). It is memoized in `narrow_subtype_cache` by
  `(source, target, resolver_generation)`. Both the positive predicate branch and
  `is_assignable_to` funnel here, so memoizing the boolean lets the same
  `(source, target)` pair — recurring across many predicate guards over one
  recursive `TSchema` — reuse the first `collect_properties_cached` walk (the
  structural fix for issues #13242 / #13250).

`is_class_subtype_for_narrowing` walks the class-extends chain nominally with a
`fuel = 50` cap, using `resolver.get_class_extends` and `defs_are_equivalent`,
never structural shape.

## Caches and invariants

`NarrowingCache` (`narrowing/core.rs`) is a bundle of `RefCell` maps plus the
exclusion `Cell` counters. It is created once per file session and shared as
`std::borrow::Cow<NarrowingCache>` — borrowed when the checker supplies the
shared cache (`with_cache`), owned and ephemeral otherwise (`new`). The shared
instance lives on the checker as `ctx.flow_shared.narrowing_cache`
(`crates/tsz-checker/src/context/mod.rs`).

| Cache | Key → Value | Purpose / invariant |
| --- | --- | --- |
| `resolve_cache` | `TypeId → TypeId` | Lazy/App/IndexAccess/TypeQuery → structural. Self-mappings for unresolved `Lazy`/`TypeQuery` are **not** stored; they re-attempt when the environment is later populated. |
| `resolve_visiting` | `Set<TypeId>` | In-progress resolution; re-entry returns the original deferred type (cycle guard). |
| `property_cache` | `(TypeId, gen, Atom) → Option<CachedPropertyType>` | Top-level property lookups for discriminant/`in` narrowing; tracks `from_index_signature`. |
| `required_property_cache` | `(TypeId, gen, Atom) → bool` | Required-property checks for negative `in` narrowing. |
| `split_nullish_cache` | `TypeId → (non_nullish, nullish)` | Reused by the checker's optional-chain / property-access fast paths. |
| `contains_type_parameters_cache` | `TypeId → bool` | "Type contains type parameters" checks. |
| `optional_chain_cache` | `(TypeId, Atom) → TypeId` | Complete optional-chain result (including nullish union and added `undefined`), skipping split/resolve/lookup on a hit. |
| `optional_property_chain_cache` | `OptionalPropertyChainKey → TypeId` | Full identifier-rooted optional chains keyed by semantic root + atomized path + optional-segment bitmask. |
| `contextual_resolve_cache` | `TypeId → TypeId` | Contextual type resolution for object-literal property typing. |
| `discriminant_index` | `(TypeId, Atom) → Arc<Map<TypeId, Vec<TypeId>>>` | Built once per (union, property) pair; O(1) per case clause. |
| `narrow_type_cache` | `NarrowTypeCacheKey → TypeId` | Memoizes **predicate-guard** narrowing only. |
| `narrow_excluding_cache` | `NarrowExcludingKey → TypeId` | `narrow_excluding_type` memo; only stored when the subtree stayed within budget (a truncated result is request-local and must not poison later requests). |
| `narrow_excluding_visiting` | `Set<NarrowExcludingKey>` | In-flight `(source, excluded)`; re-entry returns the source unchanged (recursive-alias cycle guard). |
| `narrow_assignable_cache` | `NarrowExcludingKey → bool` | `is_assignable_to` memo. |
| `narrow_subtype_cache` | `NarrowExcludingKey → bool` | `is_subtype_for_narrowing` memo. |

### Cache keys and generation

The two structured keys are the heart of cache correctness:

- `NarrowTypeCacheKey` (`narrowing/request.rs`) is
  `(source_type, guard, sense, options, resolver_generation)`. `options` is a
  `NarrowingOptions` newtype wrapping `IndexAccessOptions`
  (`no_unchecked_indexed_access`, `exact_optional_property_types`) — any compiler
  option that changes a guard's result must live here so the key stays accurate
  without ad-hoc bit-flag maintenance.
- `NarrowExcludingKey` (`narrowing/core.rs`) is
  `(source, excluded, resolver_generation)`.

`resolver_generation` (from `NarrowingContext::resolver_generation`, which is
`resolver.resolver_generation() + 1`, or `0` with no resolver) is folded into
every option-sensitive key. This is the central invalidation lever: when the
checker's `TypeEnvironment` resolves a `Lazy` alias differently (e.g. after a
later definition is bound), the resolver bumps its generation, every prior cache
key becomes unreachable, and stale predicate / exclusion / subtype results are
never reused. The `request.rs` tests pin this: different generations and
different options must produce different keys.

### Invalidation paths (checker side)

The checker owns the cache lifetime and clears or prunes it:

- **Per file**: `file_session_reset.rs` rebuilds the whole cache with
  `narrowing_cache = NarrowingCache::new()` so no cross-file `TypeId` leaks.
- **Per-`DefId`**: `clear_type_evaluation_caches_for_def`
  (`context/env_eval_cache.rs`) `retain`s the `resolve_cache` and
  `contextual_resolve_cache`, dropping any entry whose key or value
  `type_mentions_def(def_id)` — used when a definition's body changes.
- **Contextual rounds**: `clear_contextual_resolution_cache`
  (`state/cache_invalidation.rs`) clears `contextual_resolve_cache` before
  re-running generic-call argument typing, so stale contextual resolutions aren't
  reused across inference rounds.

## Edge cases and `tsc` parity

The engine encodes a long tail of `tsc`-specific behaviors. The notable ones:

- **`any` is asymmetric.** It narrows in the *true* branch of `typeof` /
  `instanceof` (to the primitive / instance type) but is preserved in the *false*
  branch and by exclusion (`narrow_excluding_type` returns `any` for an `any`
  source). A user predicate or `instanceof` against the global `Object`/`Function`
  interface leaves `any` as `any` (so a following `Array.isArray` doesn't
  intersect it to `never`).
- **`unknown` narrows more than `any`.** `typeof x === "object"` on `unknown` →
  `object | null`; truthiness on `unknown` → `{}`; `instanceof`/predicate on
  `unknown` → the target type.
- **Loose `== null` matches both.** `NullishEquality` keeps the nullish facet
  (`narrow_to_nullish`) in the true branch and excludes both `null` and
  `undefined` in the false branch.
- **`!==` only narrows unit types.** `LiteralEquality` exclusion bails (returns
  the source) unless the literal is a *unit type* (`is_unit_type`), matching
  `tsc`'s refusal to exclude non-unit operands.
- **`boolean` is `true | false`.** Exclusion treats `boolean` as the implicit
  union: `boolean` minus `true` is `false`; `true` minus `true` is `never`.
- **Falsy primitives stay wide.** `narrow_to_falsy` keeps `string`/`number`/
  `bigint` whole in the false branch (`tsc` does not narrow them to `""`/`0`/`0n`
  there), while `extract_definitely_falsy_type` does narrow them for `&&` typing.
- **Discriminant gate.** Discriminant narrowing only fires when the type has the
  Union flag; a top-level intersection is left unchanged (avoiding spurious
  `TS2339`) unless an undistributed union member is found and distributed.
- **`instanceof` is nominal for classes.** Class-to-class matching uses nominal
  relation, not structural subtyping; primitives are always excluded; an
  empty-object source under `instanceof Object` becomes the intrinsic `object`
  (not the Object interface) to avoid `TS2638`.
- **`Array.isArray` preserves element types.** `string[] | number[]` stays
  `string[] | number[]`; it does not collapse to `any[]`.
- **`Array.isArray` / predicate over recursive schemas terminate.** The false
  branch's shallow `narrow_excluding_positive_subset` filter and the exclusion
  work budget keep typebox / ts-morph `value is T` guards from exploding into
  non-terminating self-recursion (issues #13242 / #13250).
- **Generic `IndexAccess` keeps its deferred form.** Narrowing `A[K]` for generic
  `A`/`K` wraps the result as `original & narrowed` rather than leaking the
  resolved constraint, preserving assignability against the original return type.

## Free helpers and the `NarrowingVisitor`

`narrowing/utils.rs` exposes standalone nullish helpers used widely by the
checker without constructing a full `NarrowingContext`: `split_nullish_type`
(non-nullish part + nullish cause, including deferred-conditional handling),
`remove_nullish` / `remove_nullish_query` / `remove_undefined`, `is_nullish_type`,
`is_definitely_nullish`, and `type_contains_undefined`. These are the building
blocks the checker's optional-chain and non-null-assertion logic consume; they
fast-path intrinsics (`type_id.is_intrinsic()`) before any `TypeData` lookup.

The `NarrowingVisitor` (`narrowing/utils.rs`) is a `TypeVisitor` implementation
that performs the structural "narrow `type_id` by `narrower`" overlap algebra for
non-union sources: it resolves `Lazy`/`TypeQuery`/`Application`, and for
`Object`/`Function` sources checks both subtype directions (keep the more
specific, else narrow down, else intersect or `never`), holding a reusable
`SubtypeChecker` to avoid per-call hash allocations. It treats `any`/`unknown`
as "narrow to the narrower" and `never` as `never`.

The free functions `find_discriminants`, `narrow_by_discriminant`, and
`narrow_by_typeof` (`utils.rs`) are thin convenience wrappers that build a
throwaway `NarrowingContext::new` and forward to the method of the same name —
handy for call sites that don't already hold a context.
