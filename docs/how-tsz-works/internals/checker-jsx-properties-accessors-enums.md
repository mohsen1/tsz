# JSX, Property Access, Accessors, Enums, Iterables, and Promises

This chapter covers the *syntax-family checkers*: the parts of `tsz-checker`
that take a specific expression or declaration shape — a JSX element, a
`a.b` property access, a `get`/`set` accessor pair, an `enum` member, a
`for...of` loop, an `await` — and turn it into a type plus the diagnostics tsc
would emit. They are siblings of the call/generic and class checkers and share
the same cardinal rule: each one **orchestrates** AST traversal, owns the
*source spans and the feature-specific policy*, but **asks the solver** for
every structural answer. None of them runs a relation, inference, or
instantiation kernel by hand. They route assignability through the shared
gateway, read structural facts through `query_boundaries`, and call solver
operations like `tsz_solver::operations::get_iterator_info` or
`PropertyAccessEvaluator::resolve_property_access` rather than re-deriving
member lookup.

These checkers are unified by one thing: they all need *property access on a
type*. JSX reads component props, `for...of` reads `[Symbol.iterator]`/`next`,
`await` reads `then`, accessors compare a getter return against a setter
parameter, and enums resolve `E.Member`. The single chokepoint for that is
`CheckerState::resolve_property_access_with_env`
(`crates/tsz-checker/src/state/state_checking/property_access.rs`), which wraps
the solver's `PropertyAccessEvaluator` with the checker's `TypeEnvironment` so
that `Lazy(DefId)` references, generic applications, mapped-type constraints,
and `typeof` queries resolve before the member lookup runs. Read this chapter
alongside [solver-operations](solver-operations.md) (the property/iterator
kernels), [solver-evaluation](solver-evaluation.md) (alias/application
evaluation), and [checker-assignability-gateway](checker-assignability-gateway.md)
(the `TS2322`/`TS2741`/`TS2763` routing).

## Owns / Must not own

| Owns | Must not own |
| --- | --- |
| Choosing *which* member name to look up (`children`, `next`, `then`, `[Symbol.iterator]`, `E.Member`) and on which receiver. | The structural member-lookup kernel (`PropertyAccessEvaluator`) and the iterator/promise extraction operations. |
| Pre-resolving the receiver: `Lazy(DefId)` → `TypeId`, `TypeQuery` (`typeof X`), generic `Application` expansion, mapped-constraint evaluation, before the lookup. | Constructing raw `TypeKey`/`TypeData` or interning solver types directly. |
| Feature policy: JSX intrinsic-vs-component split, accessor pairing rules, enum numeric-vs-string classification, ES5 downleveled iteration, `Awaited<T>` distribution. | Deciding assignability — that always routes through the relation kernel via the gateway. |
| Diagnostic selection and spans: `TS2339` at the property name, `TS2741` at the tag, `TS2488` at the iterable expression, `TS1062` at the await. | Reading printer/formatter output as a predicate (the printer reads types; types never read the printer). |
| Recursion/fuel guards specific to each walk (await depth, property-access depth, awaited-fold depth, enum-eval depth). | The cross-file delegation / cross-arena guards (owned by `state_domain`; this layer only resets them at file boundaries). |

## Module map

| Path | Role |
| --- | --- |
| `state/state_checking/property_access.rs` | `resolve_property_access_with_env` — the environment-aware property-lookup chokepoint every checker in this chapter routes through. |
| `types/property_access_type/resolve.rs` | `get_type_of_property_access_inner` — the AST-level dispatch for `a.b`/`a["b"]`: optional chaining, enum/namespace fast paths, type-only-import guards, `TS2339` emission. |
| `checkers/property_checker.rs` | `check_property_accessibility` (`TS2341`/`TS2445`), computed-name validation, `super` access policy, unused-member marking. |
| `checkers/property_checker/{union_restricted_property,super_static_access,private_error}.rs` | Union-missing-property, `super` static expando, private-brand mismatch helpers. |
| `checkers/accessor_checker.rs` | Accessor pairing, setter-parameter grammar (`TS1052`/`TS1053`/`TS7006`/`TS7032`), inferred get/set compatibility (`TS2322`), abstract consistency (`TS1044`). |
| `checkers/enum_checker.rs` | `is_enum_type`/`is_enum_like_type`/`is_boxed_primitive_type` — symbol-flag fallbacks for arithmetic operand classification. |
| `declarations/declarations.rs` (`check_enum_declaration`) | Enum-declaration grammar (`TS2431`/`TS2452`), const-enum initializer constancy (`TS2474`), merge conflicts (`TS2567`). |
| `types/utilities/enum_utils.rs`, `types/utilities/const_enum_eval.rs` | Constant-expression evaluation for enum member values, with the thread-local `EVAL_MEMO`/`EVAL_DEPTH` memo. |
| `checkers/iterable_checker.rs` | `is_iterable_type`, `for_of_element_type`, `check_for_of_iterability`, `check_spread_iterability`, `check_destructuring_iterability`, iterator-protocol completeness checks (`TS2488`/`TS2490`/`TS2763`/`TS2767`/`TS2802`). |
| `checkers/promise_checker.rs` | Promise/thenable classification, `Awaited<T>` folding helpers, generator type-argument extraction, async-return unwrapping. |
| `checkers/promise_checker_object_normalization.rs` | `Awaited<…>` assignability-normalization cycle guard + clamp epoch. |
| `types/computation/access_await.rs` | `get_type_of_await_expression_with_request`, `compute_awaited_type` — the AST-level `await` dispatch and `getAwaitedType` mirror. |
| `checkers/jsx/` | The JSX subsystem (orchestration, props, children, extraction, overloads, runtime, diagnostics). See its own map below. |

Solver-side counterparts: `tsz_solver::operations::property`
(`PropertyAccessEvaluator`, `PropertyAccessResult`),
`tsz_solver::operations::iterators` (`get_iterator_info`, `IteratorInfo`),
`tsz_solver::type_queries::iterable` (`classify_full_iterable_type`,
`classify_for_of_element_type`, `FullIterableTypeKind`, `ForOfElementKind`),
and `tsz_solver::type_queries::extended` (`classify_promise_type`,
`PromiseTypeKind`).

## The property-access chokepoint

Every member read in this chapter funnels through one method. Understanding it
once explains JSX prop reads, iterator-protocol walks, thenable unwrapping, and
enum member access at the same time.

```text
checker call site (for-of, await, jsx, a.b)
        |
   resolve_property_access_with_env(object_type, prop_name)
        | 1. resolve_type_query_type      (typeof X -> structural type)
        | 2. PropertyAccessDepthGuard::enter()      (cap 350, RAII)
        | 3. try_lazy_lib_member_property_access     (single-member fast path)
        | 4. ensure_relation_input_ready             (precondition non-trivial receivers)
        | 5. resolve_lazy_type                       (Lazy(DefId) -> TypeId)
        | 6. resolve_mapped_constraint_for_property_access  (Omit/Pick key unions)
        | 7. contains_unresolved_application -> evaluate_type_with_env
        v
   resolve_property_access_via_boundary  --> QueryDatabase / QueryCache
        |                                      (solver PropertyAccessEvaluator)
        v
   PropertyAccessResult { Success | PropertyNotFound | PossiblyNullOrUndefined | IsUnknown }
        |
   resolve_property_access_with_env_post_query  (Application expansion, mapped revalidation)
```

The solver returns a `PropertyAccessResult`
(`crates/tsz-solver/src/operations/property.rs`), a four-variant enum:
`Success { type_id, write_type, from_index_signature }`,
`PropertyNotFound { type_id, property_name }`,
`PossiblyNullOrUndefined { property_type, cause }`, and `IsUnknown`. The
`write_type` field carries the *setter* parameter type when it diverges from
the *getter* return type (TS 4.3+ divergent accessors); `None` means read and
write types coincide. `from_index_signature` distinguishes an explicit member
from an index-signature hit (drives `TS4111`).

Two layers of fallback live in `resolve_property_access_with_env` because the
cached query path uses a **noop `TypeResolver`** that cannot resolve
`Lazy(DefId)` interface bases:

- When the cached path returns `PropertyNotFound` but the receiver's `DefId`
  resolves to a builtin-lib interface symbol (and is not file-locally
  shadowed), it re-resolves the single own property via
  `resolve_simple_lib_interface_own_property` — this is how `document.title`
  resolves without materializing the whole `Document` member closure.
- When the cached path returns a degenerate `Success { type_id: ANY,
  from_index_signature: false }` for a bare `Lazy` receiver, it re-queries
  through `resolve_property_access_via_resolver` (the solver evaluator *with*
  the checker's `TypeResolver`). The replacement is only committed when it is a
  `is_concrete_member_success` — a `Success` whose type is neither `any`, a
  type parameter, nor `this` — so a still-generic or `this`-bearing member is
  left to the post-query instantiation paths instead of being prematurely
  frozen. A genuine `PropertyNotFound` from the resolver overrides the `any`,
  so a real missing cross-file member still reports `TS2339`.

The post-query stage (`resolve_property_access_with_env_post_query`) handles
generic `Application` receivers (`Promise<number>`, `Pick<T,K>`): it expands
them via `evaluate_application_type` so mapped-property revalidation can see the
real object shape, but only *retries the lookup* when the first pass failed,
preserving the original successful result otherwise.

### Recursion guard

`MAX_PROPERTY_ACCESS_DEPTH = 350` with the RAII `PropertyAccessDepthGuard` and
thread-local `PROPERTY_ACCESS_DEPTH` counter. A self-referential receiver
(typically a malformed cross-arena generic alias whose instantiated body
transitively contains itself, refs #13212) would otherwise recurse until the
stack overflows; at the cap the guard bails to `Success { type_id: ANY, .. }`
rather than crashing. This is far above any legitimate nesting and far below
the stack-exhaustion frontier.

### AST-level property access dispatch

`get_type_of_property_access_inner`
(`crates/tsz-checker/src/types/property_access_type/resolve.rs`) is the entry
for `a.b`/`a["b"]` expression nodes. Before any member lookup it:

- handles `import.meta` (`TS1343`/`TS1470`), parser-recovery placeholders, and
  the optional-property-chain narrowing cache;
- guards type-only-import chains: if the base resolves to a type-only
  alias used in a value position, it reports the wrong-meaning diagnostic
  (`TS1361`/`TS1362`) and returns `TypeId::ERROR` *before* member lookup so no
  spurious follow-on `TS2339` fires;
- checks abstract-property-in-constructor (`TS2715`) even when `this` is `any`;
- takes the **enum/namespace fast path**
  (`try_resolve_enum_namespace_member_access`) for `E.Member`/`Ns.Member`.

When the receiver is a real type and the member is absent, the not-found path
calls `error_property_not_exist_at`
(`crates/tsz-checker/src/error_reporter/properties.rs`), which emits
`PROPERTY_DOES_NOT_EXIST_ON_TYPE` (`TS2339`,
`crates/tsz-common/src/diagnostics/data/parts/part_000.rs`). That reporter
deliberately suppresses on `ERROR`/`ANY` and on bare `__infer_*` placeholders,
but **not** on `unknown` or `never` — tsc emits `TS2339` for property access on
both, so tsz mirrors that.

## Accessors: getter/setter pairing and compatibility

`crates/tsz-checker/src/checkers/accessor_checker.rs` owns the `get x()` /
`set x(v)` family. Pairing is by *resolved property name*:
`paired_getter_member_for_setter` and `check_accessor_type_compatibility` use
`get_property_name_resolved` so a computed name like `[G.B]` that resolves to a
literal pairs with its partner, while object-literal accessors use the
stricter `get_property_name` to avoid pairing non-literal computed names like
`[0 + 1]` (tsc only pairs syntactically-resolvable names there).

`Owns:` which getter pairs with which setter; the setter-parameter grammar
checks; the inferred-getter compatibility check. `Must not own:` the relation —
the compatibility check delegates to `check_assignable_or_report_at`.

- **Setter grammar** (`check_setter_parameter`): `TS1052` (initializer, anchored
  at the accessor name), `TS1053` (rest parameter), `TS7006` (implicit-any
  parameter), and `TS7032` on the setter name. `TS7006`/`TS7032` are suppressed
  when the setter has a *paired getter* (the parameter is contextually typed
  from the getter return) or an inline/accessor-level JSDoc `@param`/`@type`.
- **Inferred compatibility** (`check_accessor_type_compatibility`): since TS 5.1
  a `get`/`set` pair may have *unrelated* explicit types. tsz mirrors this by
  only checking when the setter has an explicit annotation **and** the getter
  does **not** — i.e. the getter's type is inferred from its body. It infers via
  `infer_getter_return_type`, reads the setter annotation via
  `get_type_from_type_node`, and routes the comparison through
  `check_assignable_or_report_at`, anchoring `TS2322` at the first `return` in
  the getter body. In JS/`checkJs`, accessor pairs are co-inferred from the
  property shape, so this check is skipped entirely.
- **Abstract consistency** (`check_accessor_abstract_consistency`): a getter and
  setter for the same property must both be abstract or both concrete, else
  `TS1044` (`ACCESSORS_MUST_BOTH_BE_ABSTRACT_OR_NON_ABSTRACT`) on *both*
  accessor names.

The contextual setter-parameter type
(`contextual_setter_parameter_types_for_class_accessor`) feeds the inferred
getter type into the setter parameter so its body sees the right type — only
when the setter parameter has no annotation (or in JS files).

## Enums

Enum work is split between *grammar/value* checking and *operand
classification*.

### Declaration and value evaluation

`check_enum_declaration`
(`crates/tsz-checker/src/declarations/declarations.rs`) validates enum grammar:
`TS2431` (reserved name like `number`), `TS2452` (numeric member name —
careful: `NaN`/`Infinity`/`-Infinity` are *legal* member names per tsc, while
canonical finite numeric strings like `"3"` are not), `TS2474`/`TS2477`/`TS2478`
(const-enum initializer constancy), and `TS2567`/`TS1294`
(`erasableSyntaxOnly`).

Member *values* are computed by `evaluate_constant_expression` and friends in
`crates/tsz-checker/src/types/utilities/enum_utils.rs`. This walk is memoized
by the thread-local `EVAL_MEMO` (`NodeIndex -> Option<f64>`) with a
`DepthGuard` RAII counter (`EVAL_DEPTH`, cap `MAX_EVAL_DEPTH = 100`). The memo
caches both successes and failures and is **cleared when depth returns to 0**
(end of a top-level evaluation chain), so unrelated chains never see stale
results. Because the keys are arena-local `NodeIndex`es reused across
compilations, the whole memo is also cleared per-file via
`clear_enum_eval_memo` (called from `reset_per_file_resolution_guards`). The
sibling `const_enum_eval.rs` owns the const-enum initializer evaluator.

`TS18033` (computed enum member must be assignable to `number`) is checked in
`state/state_checking_members/statement_helpers.rs`: it first tries to evaluate
the initializer constant; only if evaluation fails to yield a number/string
does it route the type through the assignability gateway, displaying the
*widened* type (`'string'`, not `'"bar"'`) to match tsc.

### Operand classification

`crates/tsz-checker/src/checkers/enum_checker.rs` is a thin set of symbol-flag
predicates used by binary-operator checking. `is_enum_type` resolves a `TypeId`
to a `SymbolId` and tests `symbol_flags::ENUM`. `is_enum_like_type` handles
unions of `Lazy(DefId)` enum-member references (e.g. `Choice.Yes | Choice.No`)
that the solver's `NumberLikeVisitor` cannot resolve — but **only as a
fallback** when the resolved type is still `Lazy` (`is_unresolved_lazy_type`).
When the type *is* resolved to `Enum(DefId, member_type)`, the solver's
`BinaryOpEvaluator::is_arithmetic_operand` is authoritative and distinguishes
numeric from string enums via the visitor. `is_boxed_primitive_type` detects
the boxed interface wrappers (`Number`/`String`/`Boolean`/`BigInt`/`Symbol`)
that are *not* valid arithmetic operands, driving `TS2362`/`TS2363`/`TS2365`.

This is a textbook anti-hardcoding boundary: the *built-in* boxed names are
matched against the resolved symbol's `escaped_name`, not against rendered
output or user identifiers, and the enum/enum-member decision is a binder
`symbol_flags` test, not a name check.

## Iterables: `for...of`, spread, destructuring

`crates/tsz-checker/src/checkers/iterable_checker.rs` is the largest file in
this family. It answers two questions — *is this type iterable?* and *what is
its element type?* — by delegating classification to the solver and walking the
iterator protocol through the property-access chokepoint.

### Iterability

`is_iterable_type` short-circuits the intrinsics (`any`/`unknown`/`error` →
true to avoid false positives, `string` → true, the other primitives → false)
then delegates to `is_iterable_type_classified`, which switches on
`classify_full_iterable_type` (`tsz_solver::type_queries::iterable`,
`FullIterableTypeKind`):

| `FullIterableTypeKind` | Decision |
| --- | --- |
| `Array` / `Tuple` / `StringLiteral` | always iterable |
| `Union(members)` | all members iterable |
| `Intersection(members)` | at least one member iterable |
| `Object(shape)` | `object_has_iterator_method` on the shape; `None` → fall back to property access |
| `Application { base }` | property-access resolve `[Symbol.iterator]`; else resolve the alias and re-classify |
| `TypeParameter { constraint }` | iterable iff the constraint is (unconstrained → not iterable, no `TS2488`) |
| `Readonly(inner)` | unwrap and recurse |
| `ComplexType` (index/conditional/mapped) | resolve to apparent type and re-classify |
| `FunctionOrCallable` / `NotIterable` | property-access resolve `[Symbol.iterator]` |

`object_has_iterator_method` peeks at the `ObjectShapeId` for a non-optional,
zero-required-arg `[Symbol.iterator]` member; an *optional* `[Symbol.iterator]`
is `Some(false)` (not a valid iterable). When the shape can't decide, the code
falls through to `type_has_symbol_iterator_via_property_access`, which actually
follows the protocol: resolve `[Symbol.iterator]`, call it
(`get_call_return_type`), then verify the returned iterator has a `next`
method — all via `resolve_property_access_with_env`.

### `for...of` dispatch and element type

`check_for_of_iterability(expr_type, expr_idx, is_async)` is the policy core:

```text
expr_type
  any/error -> ok ; unknown -> TS2571-family
  null/undefined -> TS18050 (report_nullish_object)
  is_async?
    yes -> is_async_iterable_type || is_iterable_type
            -> check_iterator_next_type_assignability(UNDEFINED) ; ok
        else: no AsyncIterator/AsyncIterable in lib -> fall back to ES5 array/string check
        else: TS2504 (must have [Symbol.asyncIterator])
    no  -> ES5 target & !downlevelIteration?
            array/tuple/string -> ok
            has [Symbol.iterator] -> TS2802 ; string-in-union -> TS2461 ; else TS2495
         else (ES2015+):
            is_iterable_type?
              check_iterator_next_returns_value  -> TS2490 if next().value missing
              check_iterator_return_is_method    -> TS2767 if `return` is non-callable
              check_iterator_next_type_assignability(UNDEFINED) -> TS2763 if TNext != undefined
              ok
            else -> TS2488 (emit_ts2488_not_iterable)
```

The element type comes from `for_of_element_type` → `for_of_element_type_with_depth`
(depth cap 100). For sync iteration it uses
`tsz_solver::operations::get_iterator_info` (returning `IteratorInfo` with
`yield_type`/`return_type`/`next_type`), with array/tuple/string fast paths and
a checker-side `resolve_iterator_element_type_via_property_access` fallback for
`Application(Lazy(DefId), …)` receivers the solver can't see through. For
`for await...of` it tries the async iterator protocol first, then falls back to
the sync protocol plus `apply_awaited` (Promise unwrapping of the element).
Crucially, `get_iterator_info` fast-paths `Array`/`Tuple` as *sync* iterators
regardless of `is_async`, so the async path must additionally `apply_awaited`
their element type to match `for await (const x of arr)` semantics.

`check_iterator_next_type_assignability` is where iterables touch the
assignability gateway: it extracts `TNext` via
`get_generator_next_type_argument`, and if `undefined` is not assignable to it,
routes `call_arg_relation_outcome(sent_type, next_type)` and emits
`TS2763`/`TS2764`/`TS2765`/`TS2766` by `IterationUseKind`. It defers (returns
true) whenever `TNext` is generic, contains free type parameters, or contains
`infer` types — comparing a not-yet-instantiated `TNext` would produce false
positives.

`check_spread_iterability` and `check_destructuring_iterability` reuse the same
`is_iterable_type`/element-type machinery. Destructuring layers extra parity:
`const [] = f()` on `unknown` emits `TS2488` *then* `TS2571`, but catch-clause
destructuring suppresses `TS2571`; `never` still reports `TS2488` in array
destructuring.

## Promises, thenables, and `await`

`crates/tsz-checker/src/checkers/promise_checker.rs` classifies Promise-like
types and extracts their type arguments; `types/computation/access_await.rs`
owns the `await` expression.

### `await` dispatch

`get_type_of_await_expression_with_request`
(`crates/tsz-checker/src/types/computation/access_await.rs`):

1. `TS2524` if the await sits in a parameter initializer (suppressed near parse
   errors to avoid cascades).
2. tsc's `await(...)` quirk: in a sync function body, `await(x)` is treated as
   an undefined-identifier use and reports `TS2311`-family, not an
   await-context error.
3. **Contextual typing**: when a contextual type `T` is present, the operand is
   given the contextual union `T | PromiseLike<T> | Promise<T>`. Including the
   concrete `Promise<T>` (not just `PromiseLike<T>`) is what lets
   `const x: T = await new Promise(...)` infer the constructor type argument,
   because the constraint `Promise<__infer_0> <: Promise<T>` shares a base and
   unifies by type-argument matching.
4. `await_operand_invalid_thenable_this_type` → `TS1320`-family ("type of await
   operand must either be a valid promise or must not contain a callable
   `then`").
5. `check_self_referencing_promise_cycle` → `TS1062` for types like
   `type T1 = 1 | Promise<T1> | T1[]` that would loop forever in
   `Awaited<T>` resolution.
6. The result type is `compute_awaited_type(expr_type, 0)`.

### `compute_awaited_type` — the `getAwaitedType` mirror

`compute_awaited_type` (depth cap `MAX_AWAIT_DEPTH = 10`) mirrors tsc's
`getAwaitedType`:

- a top-level **union** distributes — `Awaited<A | B>` → `Awaited<A> |
  Awaited<B>` — then rejoins through the canonicalizing union factory (so
  `T | T` collapses to `T`); this is why `await x` for `x: T | Promise<T>`
  yields `T`, not the original union;
- a top-level **intersection** distributes the same way — `Awaited<A & B>` →
  `Awaited<A> & Awaited<B>` — so awaiting a `Promise<number> & Promise<string>`
  recovery type yields `number & string` (`never`);
- otherwise it iteratively unwraps nested Promise applications via
  `promise_like_return_type_argument`, re-entering union distribution if an
  unwrap reveals a union.

A second, eager fold path —
`fold_concrete_awaited_application` / `try_evaluate_awaited_application` /
`compute_explicit_awaited_application_type` (depth cap 8) — exists because the
generic conditional-type evaluator cannot structurally match `Promise<T>`'s
`then` shape through a free type parameter, and its per-instance budget would
bail nested `Awaited<Promise<Promise<T>>>` to a deferred conditional (a
spurious `TS2322`). The fold unwraps a single thenable layer via the
`builtin_promise_like_application_arg` fast path (the `Promise`/`PromiseLike`
*application* form), and only when that misses does it `evaluate_type_with_env`
and inspect the structural `{ then }` object shape via `classify_promise_type`
→ `PromiseTypeKind::Object`. It explicitly returns `None` for *generic*
`Awaited<…>` (which must stay deferred) and stops on a self-referential thenable.

### Promise classification

`is_promise_type`/`is_global_promise_type`/`type_ref_is_promise_like` decide
Promise-ness; `promise_like_return_type_argument`,
`promise_like_type_argument_from_base`/`_from_alias`/`_from_class` extract the
`T` from `Promise<T>` / `PromiseLike<T>` / a user thenable. These build on the
solver's `classify_promise_type` (`tsz_solver::type_queries::extended`,
`PromiseTypeKind` = `Application`/`Lazy`/`TypeQuery`/`Object`/`Union`/
`NotPromise`). The `def_is_lib_promise*` helpers gate "is this the *standard
library* Promise" on `DefId`/symbol provenance, not on the name string alone,
so a user-declared `interface Promise<T>` doesn't masquerade as the lib type.

The companion `promise_checker_object_normalization.rs` owns the `Awaited<…>`
assignability-normalization cycle guard and clamp epoch; its
`reset_awaited_eval_thread_local_state` is part of the per-file reset because
its visiting set keys on arena-local `TypeId`s reused across compilations.

`requires_return_value`/`type_requires_return_ts2355` and the
`get_generator_{return,yield,next}_type_argument` extractors live here too and
feed implicit-return checking (`TS2355`/`TS7030`) and generator
return-type assignability (`check_generator_return_type_assignability`, which
verifies `Generator<TYield, any, any>` satisfies a custom annotated return
type, skipping the standard generator-like names and single-heritage
no-own-member interfaces).

## JSX

JSX is its own subsystem under `crates/tsz-checker/src/checkers/jsx/`. It maps a
`<Tag .../>` element to a type and validates its attributes against the
component's props.

### JSX module map

| Path | Role |
| --- | --- |
| `jsx/orchestration/resolution.rs` | `get_type_of_jsx_opening_element_with_children` — the element entry: tag classification, intrinsic-vs-component split, namespace/`IntrinsicElements`/`IntrinsicAttributes` resolution and caches. |
| `jsx/orchestration/component_props.rs` | Function/class component props extraction and instantiation. |
| `jsx/props/resolution.rs` | `check_jsx_attributes_against_props` — the attribute-checking driver and union-props narrowing. |
| `jsx/props/attr_check_pipeline.rs` | The phased attribute pipeline (collect → spread → cascade) that orders the overlapping `TS2322`/`TS2741` paths. |
| `jsx/props/validation.rs` | `check_missing_required_jsx_props` (`TS2741`), spread `TS2698`, intrinsic-attribute `TS2322`, grammar. |
| `jsx/children.rs` | Child normalization, shape validation, contextual `children` typing. |
| `jsx/extraction.rs` | Component-signature extraction, SFC return validation. |
| `jsx/overloads.rs` | Overloaded stateless-function-component resolution (`TS2769`). |
| `jsx/runtime.rs` | JSX factory scope, `jsxImportSource`, fragment factory diagnostics. |
| `jsx/diagnostics.rs` | Display-target building and message rendering for JSX errors. |

### Element resolution

`get_type_of_jsx_opening_element_with_children` first runs factory/import-source
checks, then a `TS2698` spread-attribute pre-pass (emitted once per element,
independent of which downstream path handles the rest), then classifies the
tag:

```text
tag_name
  Identifier, lowercase first char  -> intrinsic
  JsxNamespacedName (svg:path)      -> intrinsic (TS2639 if namespace is uppercase in React mode)
  PropertyAccessExpression / Uppercase identifier -> component
```

**Intrinsic** tags look up `JSX.IntrinsicElements[tag]`. The interface type is
cached on `CheckerContext`: `get_intrinsic_elements_symbol_id` populates
`jsx_intrinsic_elements_symbol_cache`, and `get_intrinsic_elements_type`
populates `jsx_intrinsic_elements_type_cache`, merging all declaration-merged
`IntrinsicElements` interfaces via `merge_interface_types`. The accessed tag's
props are read with the *element index* via `get_jsx_intrinsic_props_for_tag`,
then `check_jsx_attributes_against_props` validates the supplied attributes.
**Component** tags resolve the component type (function or class), extract its
props parameter, and validate against that. The `JSX.IntrinsicAttributes` /
`JSX.Element` / `JSX.ElementClass` reference types are resolved through
`get_jsx_namespace_export_symbol_id` and `type_reference_symbol_type`.

### Attribute checking and the `TS2322`/`TS2741` cascade

`check_jsx_attributes_against_props` (`jsx/props/resolution.rs`) is where JSX
joins the assignability gateway. After a `TS17000` grammar check and
`normalize_jsx_required_props_target` (so managed/mapped/application prop
surfaces like `JSX.LibraryManagedAttributes<…>` and
`DetailedHTMLProps<…>` read through the same structural path), it either:

- delegates **union props** to `check_jsx_union_props` (whole-object
  assignability against the narrowed union), or
- runs the phased pipeline: `prepare_jsx_attr_check_context` →
  `compare_jsx_attributes_loop` (per-attribute walk recording provided
  names/types, spread entries, and override anchors) →
  `emit_deferred_jsx_spread_diagnostics` → `emit_jsx_children_synthesis_diagnostics`
  → `emit_jsx_attr_final_assignability_diagnostics`.

The phasing exists because tsc's diagnostic precedence is order-sensitive:
per-attribute `TS2322` (value-type assignability, excess property, `key`/`ref`)
anchors inline at the current attribute span, while whole-attrs `TS2322` and
`TS2741` (missing required prop) defer to a precedence cascade so only one
fires. `key` and `ref` are tracked as "provided" for missing-prop accounting
but *not* type-checked against component props — they belong to
`IntrinsicAttributes`/`IntrinsicClassAttributes`. Every assignability decision
routes through `query_boundaries::checkers::jsx::props_are_assignable` /
`should_report_jsx_class_missing_props_via_assignability` (the gateway), never a
hand-rolled structural walk. `check_missing_required_jsx_props`
(`jsx/props/validation.rs`) emits `TS2741` when a required prop is absent and
not covered by a spread.

`Owns:` tag classification, the intrinsic/component split, the precedence
cascade, attribute spans, contextual `children` typing. `Must not own:` the
assignability relation (gateway), member lookup (the props type comes from the
property-access/extraction machinery), or any output surgery.

## Caches and invariants

| Cache / guard | Where | Invalidation |
| --- | --- | --- |
| `optional_property_chain_cache` | `flow_shared.narrowing_cache` (read in `get_type_of_property_access_inner`) | Per narrowing epoch; keyed by access node + flow request. |
| `jsx_intrinsic_elements_symbol_cache`, `jsx_intrinsic_elements_type_cache` | `CheckerContext` | Per-file `CheckerContext` lifetime (one resolved `IntrinsicElements` surface per file). |
| `EVAL_MEMO` / `EVAL_DEPTH` | `types/utilities/enum_utils.rs` (thread-local) | Cleared when `EVAL_DEPTH` returns to 0 (top-level chain end) and per-file via `clear_enum_eval_memo`. |
| const-enum eval memo | `types/utilities/const_enum_eval.rs` (thread-local) | `clear_const_eval_memo` in `reset_per_file_resolution_guards`. |
| `PROPERTY_ACCESS_DEPTH` | `state/state_checking/property_access.rs` (thread-local) | RAII `PropertyAccessDepthGuard`; reset by `STACK_STATE`/per-file resets. |
| Awaited-eval cycle guard + clamp epoch | `promise_checker_object_normalization.rs` (thread-local) | `reset_awaited_eval_thread_local_state` per file. |
| `QueryCache` member-lookup results | solver `QueryDatabase` | Solver-owned; see [solver-caches-objects-contextual-compat](solver-caches-objects-contextual-compat.md). |

Invariants worth stating explicitly:

- **Receiver pre-resolution is the checker's job.** The solver's cached query
  path runs a noop `TypeResolver`; `Lazy(DefId)`, `TypeQuery`, generic
  `Application`, and unresolved mapped constraints must be resolved by the
  checker (via `TypeEnvironment`/`evaluate_type_with_env`) *before* the lookup,
  or member access silently degrades to `any`. The two fallback re-queries in
  `resolve_property_access_with_env` exist to repair exactly that degradation.
- **All thread-local memos key on arena-local `NodeIndex`/`TypeId`** that are
  reused across files and compilations, so they are reset at file boundaries
  (`reset_per_file_resolution_guards`) and compilation boundaries
  (`clear_all_thread_local_state`); a mid-walk bail (stack-overflow breaker,
  fuel exhaustion) must not leak a dirty guard onto a shared worker thread
  (#13255/#13368 schedule-sensitivity).
- **Depth caps are local and generous:** property access 350, await 10, awaited
  fold 8, enum eval 100, for-of element 100. They guard genuinely cyclic or
  pathological inputs and are far above any valid nesting.

## Edge cases and tsc parity

- **`unknown`/`never` property access** still reports `TS2339`
  (`error_property_not_exist_at` suppresses only `ERROR`/`ANY`/`__infer_*`).
- **Divergent accessors (TS 4.3+):** `PropertyAccessResult::Success.write_type`
  carries the setter parameter type when it differs from the getter return, so
  assignment checking uses the write type while reads use the read type.
- **TS 5.1 unrelated get/set types:** `check_accessor_type_compatibility` only
  fires when the getter type is *inferred* and the setter is *annotated*; two
  explicitly annotated accessors may be unrelated.
- **Enum member names:** `NaN`/`Infinity`/`-Infinity` are legal member names
  (no `TS2452`); canonical finite numeric strings are not.
- **Numeric vs string enum operands:** decided by the solver's
  `Enum(DefId, member_type)` visitor when resolved; the checker's
  symbol-flag `is_enum_like_type` is only a `Lazy`-fallback.
- **`for await` over arrays:** `get_iterator_info` treats arrays as sync
  iterators, so the async path additionally `apply_awaited`s the element.
- **ES5 downleveled iteration:** without `downlevelIteration`, `for...of` over a
  non-array/string iterable emits `TS2802` (has `[Symbol.iterator]`),
  `TS2461` (string stripped from a union leaves a non-array remainder), or
  `TS2495` (neither array nor string).
- **Iterator protocol completeness:** `TS2490` (`next().value` missing),
  `TS2767` (`return` present but non-callable), `TS2763`-`TS2766` (`TNext`
  rejects the sent type).
- **Self-referencing Promise cycle:** `await` of `type T = 1 | Promise<T> |
  T[]` emits `TS1062` before unwrapping.
- **`Awaited<A | B>` distributes**; `Awaited<A & B>` distributes too, so a
  no-overload-match `Promise<number> & Promise<string>` awaits to `never`.
- **Type-only import in value position:** property access on a type-only alias
  reports `TS1361`/`TS1362` and returns `ERROR` before member lookup, so no
  spurious `TS2339` follows.
- **JSX `key`/`ref`:** tracked as provided for `TS2741` accounting but never
  type-checked against component props (they belong to `IntrinsicAttributes`).
- **JSX namespaced tags:** `svg:path` is always intrinsic; `<A:foo>` (uppercase
  namespace) in React mode emits `TS2639`.

## Cross-links

- The member-lookup, iterator, and promise *kernels* live in
  [solver-operations](solver-operations.md); alias/application expansion in
  [solver-evaluation](solver-evaluation.md); type intern/`DefId` handling in
  [solver-types-intern-def](solver-types-intern-def.md).
- Assignability routing for `TS2322`/`TS2741`/`TS2763` is in
  [checker-assignability-gateway](checker-assignability-gateway.md); the
  underlying relation in [solver-relations](solver-relations.md).
- Contextual typing of `await` operands and JSX `children` interacts with
  inference ([solver-inference](solver-inference.md)) and the contextual cache
  ([solver-caches-objects-contextual-compat](solver-caches-objects-contextual-compat.md)).
- Flow/optional-chain narrowing that feeds property access is in
  [checker-flow-and-narrowing](checker-flow-and-narrowing.md) and
  [solver-narrowing](solver-narrowing.md).
- Accessors as *class members* (pairing/abstract orchestration order) are in
  [checker-classes](checker-classes.md); call/overload resolution that JSX SFC
  overloads reuse is in
  [checker-calls-signatures-generics](checker-calls-signatures-generics.md).
- Per-file caches, `CheckerContext` fields, and the thread-local reset surface
  are in [checker-context-and-state](checker-context-and-state.md); diagnostic
  code constants and message formatting in
  [checker-error-reporter-diagnostics](checker-error-reporter-diagnostics.md).
