# Contextual Typing and Reverse Inference

## Orientation

Most of the solver answers a "forward" question: given an expression's
syntactic type and a target type, *are they related?* Contextual typing is the
opposite arrow. It pushes an *expected* type **into** an expression so that
otherwise-unknowable pieces — an arrow's bare parameters, an array literal's
element types, an object literal's property values, a `yield` operand — acquire
a type *before* they are checked. tsc calls this "the contextual type"; tsz
implements the extraction half of it in one self-contained subsystem,
`crates/tsz-solver/src/contextual`. This document fills the gap that
[solver-caches-objects-contextual-compat](solver-caches-objects-contextual-compat.md)
and [solver-inference](solver-inference.md) left at the boundary: those docs
describe *that* a contextual type exists and how it interacts with the
inference engine; this one walks the actual extraction kernel
(`ContextualTypeContext` and the `*Extractor` visitors), the
`apply_contextual_type` merge rule, and the two distinct "reverse inference"
machineries — the `compat_mapped` structural mapped-to-mapped path
(`crates/tsz-solver/src/relations/compat_mapped.rs`) and the homomorphic
*reverse-mapped* inference path
(`crates/tsz-solver/src/operations/constraints/reverse_mapped.rs`).

The subsystem is deliberately *narrow*. It does not own when a contextual type
is available (that is the checker's flow/declaration logic), nor how a
contextual type drives generic inference (that is the inference engine), nor
the call-signature plumbing that supplies callable contextual types (that lives
in the `CallEvaluator`). It owns exactly one job: given an expected `TypeId` and
a *position* (parameter index, property name, tuple slot, return, yield), return
the `TypeId` that the expression at that position should be checked against — or
`None` when no contextual type is provided. The whole file is structured as a
recursive descent over the *shape* of the expected type, normalizing
`Union`/`Intersection`/`Application`/`Lazy`/`Mapped`/`Conditional`/`IndexAccess`
/`TypeParameter` wrappers down to the callable, object, array, or tuple core a
visitor can read.

## Owns / Must not own

| Owns | Must not own |
| --- | --- |
| Extracting a positional contextual type from an expected `TypeId` (`ContextualTypeContext`) | Deciding *whether* an expression has a contextual type at all (checker flow / declaration sites) |
| Normalizing `Union`/`Intersection`/`Application`/`Lazy`/`Mapped`/`Conditional`/`IndexAccess`/`TypeParameter` wrappers before extraction | Running the generic inference fixpoint (the inference engine / `CallEvaluator`) |
| The `apply_contextual_type` merge: how an *expression* type and a *contextual* type combine into the expression's final type | Reporting diagnostics; the checker decides `TS7006`/`TS2322`/`TS2345` |
| Structural mapped-to-mapped assignability (`compat_mapped.rs`) | Emitting any output or pattern-matching printer text |
| Homomorphic reverse-mapped inference into a source object literal (`reverse_mapped.rs`) | Constructing raw `TypeKey` in the checker; the checker calls these via the query boundary |

## Where the pieces live

| Path | Role |
| --- | --- |
| `contextual/mod.rs` | Re-exports `ContextualTypeContext`, `apply_contextual_type`, `rest_argument_element_type`; declares the `extractors` submodule |
| `contextual/core.rs` | `ContextualTypeContext` — the wrapper-normalizing recursive descent; `apply_contextual_type`; the per-index mapped substitution helpers |
| `contextual/extractors.rs` | The `TypeVisitor`-based leaf extractors (`ParameterExtractor`, `PropertyExtractor`, `TupleElementExtractor`, …) plus positional helpers like `extract_param_type_at_for_call` and `variadic_tuple_element_type` |
| `relations/compat_mapped.rs` | `CompatChecker` methods for *structural* mapped-type compatibility (e.g. `Readonly<T>` ↔ `Partial<T>`) when the mapped can't be concretely expanded |
| `operations/constraints/reverse_mapped.rs` | `constrain_reverse_mapped_type` / `reverse_infer_through_template` — inference of `T` from a source object against `{ [K in keyof T]: ... }` |
| `operations/core/call_evaluator.rs` | `get_contextual_signature_for_arity_inner` — flattens callable/overload shapes into a single `FunctionShape` for the extractors to read |
| `state/type_analysis/computed_helpers.rs` (checker) | `contextual_type_for_expression` — the checker entry point that normalizes a declared type before handing it to the solver |
| `query_boundaries/common.rs` (checker) | Re-exports `ContextualTypeContext` / `apply_contextual_type` so the checker never touches solver internals directly |

`TypeVisitor` itself is defined in
`crates/tsz-solver/src/visitors/visitor.rs` (`pub trait TypeVisitor`, around
line 65); the contextual extractors are ordinary implementations of it.

## The data-flow picture

```
                 checker (AST orchestration)
                     |
   declared/expected type  (e.g. const f: Handler = (x) => ...)
                     |
   CheckerState::contextual_type_for_expression   <- computed_helpers.rs
   (resolves typeof, preserves callable/index-access/type-param shells,
    evaluates the rest)
                     |
                     v
   ContextualTypeContext::with_expected(interner, expected)   <- core.rs
                     |
       +-------------+----------------------------------------+
       | position-specific query:                             |
       |   get_parameter_type(i)      get_property_type(name) |
       |   get_parameter_type_for_call(i, n)  get_tuple_..(i) |
       |   get_array_element_type()   get_return_type()       |
       |   get_generator_yield_type() get_this_type_..()      |
       +-------------+----------------------------------------+
                     |
        recursive wrapper normalization (in core.rs):
          Union     -> per-member, then collect_single_or_union*
          Intersect -> per-member, then collect_from_intersection
          Applicat. -> get_contextual_signature_* OR evaluate OR base
          TypeParam -> constraint
          Lazy/Mapped/Conditional/IndexAccess -> evaluate_type, retry
                     |
                     v
        leaf TypeVisitor extractor (extractors.rs):
          Function/Callable -> extract_param_type_at_for_call / ...
          Object            -> PropertyExtractor
          Array/Tuple       -> ArrayElementExtractor / TupleElementExtractor
                     |
                     v
        Option<TypeId>  (the contextual type, or None)
                     |
            checker checks expr against it, then:
   apply_contextual_type(interner, expr_type, Some(ctx_type))  <- core.rs
                     |
                     v
            final expression TypeId
```

The split is intentional: `core.rs` knows nothing about *leaf* shapes (it never
reaches into an `ObjectShape` or a `FunctionShape` directly for extraction); the
visitors in `extractors.rs` know nothing about wrappers (they only see the
fully-normalized core). Every recursive branch in `core.rs` constructs a fresh
child `ContextualTypeContext::with_expected_and_options(...)` and re-dispatches
the same query — so the normalization is uniform across every position.

## `ContextualTypeContext`: the normalizing descent

`ContextualTypeContext<'a>` (`core.rs`) holds three fields: the `interner`
(`&dyn TypeDatabase`), an `Option<TypeId> expected`, and a `no_implicit_any`
flag. The flag matters because tsc's contextual typing of *multi-signature*
function targets is sensitive to `noImplicitAny` — a union of disagreeing
callable members yields no contextual parameter type, which under
`noImplicitAny` surfaces as `TS7006`.

Each public query (`get_parameter_type`, `get_property_type`,
`get_array_element_type`, `get_tuple_element_type`, `get_return_type`,
`get_this_type`, `get_generator_yield_type`, …) follows the **same skeleton**:

1. `let expected = self.expected?;` — no expected type, no contextual type.
2. A `Function`-intrinsic short-circuit where relevant: if the expected type is
   the boxed or intrinsic `Function` type, parameters are contextually `any`
   (`is_function_boxed_or_intrinsic`, `core.rs`). This prevents false `TS7006`
   for callbacks constrained by `T extends Function`.
3. Wrapper normalization, dispatched on `self.interner.lookup(expected)`:
   - **`Union`** — recurse per member, collect with a reducer chosen for that
     position (see "Union collectors" below).
   - **`Intersection`** — recurse per member, then `collect_from_intersection`,
     which drops broad `any` candidates when a precise one exists.
   - **`Application`** — try `evaluate_type`; if it changes, retry on the
     evaluated type; otherwise fall back to the application `base`. For call
     positions it first tries `get_contextual_signature_for_arity_with_compat_checker`
     so application-instantiated signatures (e.g. `Iterable<readonly [K, V]>`)
     are preserved rather than lost to the bare base.
   - **`TypeParameter`/`Infer`** — recurse into the *constraint*
     (`get_type_parameter_constraint`), so `f<T extends (p: number) => number>`
     contextually types the callback parameter as `number`.
   - **`Mapped`/`Conditional`/`Lazy`/`IndexAccess`** — `evaluate_type`, and if
     it changed, retry. `Conditional` additionally recurses into *both*
     branches and unions the results (a contextual type valid in either branch
     of `T extends U ? A : B`).
4. Leaf extraction: a `TypeVisitor` (`ParameterExtractor`, `PropertyExtractor`,
   `TupleElementExtractor`, `ReturnTypeExtractor`, …) `extract(expected)`.

Because step 3 always reconstructs a child context and re-enters the same query,
arbitrarily nested wrappers (`Partial<Readonly<Foo>> | Bar`) are peeled one
layer per recursion without any one branch needing to understand the others.

### Union collectors — and why three of them exist

`extractors.rs` exposes three union-combining helpers, and the *choice between
them is load-bearing* for tsc parity:

| Helper | Reduction | Used for |
| --- | --- | --- |
| `collect_single_or_union` | `db.union` (full subtype reduction) | `this`, return, generator slots, `get_parameter_type` |
| `collect_single_or_union_no_reduce` | `db.union_literal_reduce` (literal-only) | `get_parameter_type_for_call`, `ParameterForCallExtractor` |
| `collect_single_or_union_preserve` | `db.union_preserve_members` (none) | tuple/property element extraction |

The `_no_reduce` variant exists because callback parameters are
*contravariant*: for `Array<string>.map | Array<never>.map`, the callback
`(value: string) => U` is a subtype of `(value: never) => U`, so a full subtype
reduction would discard the `string` variant and lose contextual information.
The `_preserve` variant mirrors tsc's `mapType(..., /*noReductions*/ true)`: a
fresh literal element keyed by `number | 2` must not be folded to `number`, or
a literal element like `[2]` checked against `[number, boolean] | [2]` would
widen and fail to match its literal arm. The doc comment on
`collect_single_or_union_preserve` names this exact case.

`collect_from_intersection` is the intersection counterpart: it first strips
`TypeId::ANY` members when any non-`any` member exists (so an
`{ [k: string]: any }` index member does not erase a precise property type),
then combines via a caller-supplied closure (`db.union` for parameters,
`db.intersection` for properties/elements).

### The leaf extractors

Each extractor is a `TypeVisitor` whose `Output = Option<TypeId>` and whose
`default_output()` is `None`, so any unhandled shape simply yields "no
contextual type":

- **`ParameterExtractor`** / **`ParameterForCallExtractor`** read a
  `FunctionShape` or `CallableShape` and call `extract_param_type_at` /
  `extract_param_type_at_for_call`. The `_for_call` variant additionally filters
  call signatures by arity (`signature_accepts_arg_count`) so an overload set
  contributes only signatures that can actually accept `arg_count` arguments.
  Both refuse to provide a contextual type for an overload set mixing generic
  and non-generic call signatures — mirroring tsc's `getIntersectedSignatures`
  returning `undefined` (see `visit_callable` in both).
- **`PropertyExtractor`** reads an `ObjectShape`, matching by interned `Atom`,
  then falling back to a number/string index signature
  (`index_signature_applies` runs a real `query_relation` subtype check for
  non-trivial key types). `new_for_assignment` sets `strip_optional_undefined`
  so `{ x: 1 }` against `{ x?: number }` is checked against `number`, not the
  read-side `number | undefined`.
- **`ArrayElementExtractor`** / **`TupleElementExtractor`** read `Array`,
  `Tuple`, or an index-signatured object; the tuple variant additionally
  handles variadic tuples through `variadic_tuple_element_type`.
- **`ReturnTypeExtractor`**, **`ThisTypeExtractor`**, **`ThisTypeMarkerExtractor`**,
  **`ApplicationArgExtractor`** (for `Generator<Y, R, N>` slots),
  **`RestParameterExtractor`**, **`RestPositionCheckExtractor`**,
  **`RestOrOptionalTailPositionExtractor`** round out the position set.

### Positional parameter math: `extract_param_type_at_inner`

The single most intricate leaf helper is `extract_param_type_at_inner`
(`extractors.rs`). It maps an argument index onto a parameter list that may end
in a rest parameter whose type is an array, tuple, union, intersection, or type
parameter:

- `index < rest_start` → the fixed parameter type directly.
- index lands on a rest parameter `...args: T`:
  - `T = U[]` → `U`.
  - `T = [A, B, ...C[]]` variadic tuple → `variadic_tuple_element_type` maps the
    position through prefix / variadic / tail (only when `arg_count` is known,
    which is why `get_parameter_type_for_call` threads `arg_count`).
  - `T` a non-variadic tuple → direct indexing into the tuple elements.
  - `T = A | B` → recurse per member with a mocked single-member rest param,
    then `collect_single_or_union_no_reduce`.
  - `T = A & B` → recurse per member, returning the first concrete (non
    type-parameter, non-intersection) result.
  - `T` a bare/constrained `TypeParameter` → return `T` itself when `arg_count`
    is known (preserving the generic for downstream inference), else recurse
    into its constraint.

`evaluate_rest_like_type` is consulted first to normalize evaluatable rest
wrappers (`ReadonlyType`, `Lazy`, `Mapped`, `Conditional`, `IndexAccess`,
`TypeQuery`, `KeyOf`, `Application`) before the structural cases run.

`variadic_tuple_element_type` deserves its own note: given `[A, ...B[], C, D]`
and a concrete rest-arg count, it computes a `suffix_start =
rest_arg_count - total_suffix_len` and routes each `offset` to prefix, the
expanded rest's `fixed`, the variadic element, the expansion `tail`, or the
outer tail. A probe past the outer tail returns the variadic element type so
"is there a rest parameter?" probes correctly answer "yes".

## A walk-through: contextually typing an arrow's parameters

```ts
type Handler = (e: MouseEvent, i: number) => void;
const h: Handler = (x, y) => { /* x: MouseEvent, y: number */ };
```

1. The checker computes the declared type of `h` (`Handler`, a `Lazy(DefId)`
   alias whose body is a `Function`) and calls
   `CheckerState::contextual_type_for_expression` on it
   (`computed_helpers.rs`). Because the resolved shape is a function type, the
   helper *preserves it as-is* (the "preserve direct callable shapes" branch) so
   contravariant parameter unions are not collapsed.
2. The checker builds `ContextualTypeContext::with_expected_and_options(types,
   handler_ty, no_implicit_any)` and asks `get_parameter_type(0)`.
3. `get_parameter_type` sees a `Lazy`, hits the
   `Mapped/Conditional/Lazy/IndexAccess` branch, calls `evaluate_type`, gets the
   underlying `Function`, and re-enters on the evaluated type.
4. Now `expected` is a `Function`. None of the wrapper branches match, so the
   `ParameterExtractor::new(interner, 0, no_implicit_any)` runs `visit_function`,
   which calls `extract_param_type_at(db, &shape.params, 0)` → `MouseEvent`.
5. `get_parameter_type(1)` repeats and returns `number`.
6. The checker binds `x: MouseEvent`, `y: number` and checks the body. No
   `TS7006` fires because both parameters received a contextual type.

`repair_array_callback_value_param` (`extractors.rs`) is a small parity fix on
this path: for a 3-parameter array callback `(value, index, array)`, if the
value-param contextual type is a strict supertype of the array's element type,
it is narrowed to the element type — matching how tsc derives the callback value
from the array operand.

## A walk-through: object-literal property context

```ts
const obj: { x: number; y?: string } = { x: 1, y: "hi" };
```

1. For each property assignment, the checker calls
   `ContextualTypeContext::with_expected(types, obj_ty).get_property_assignment_type("x")`.
2. `get_property_type_inner("x", strip_optional_undefined=true)` does a single
   `lookup` to dispatch. The expected is an `Object`, so none of the
   `Union`/`Intersection`/`Mapped`/`Application`/`Conditional`/`TypeParameter`
   arms fire; it falls to the visitor.
3. `PropertyExtractor::new_for_assignment(db, "x")` runs `visit_object`, finds
   the `x` property by `Atom`, and returns `number`.
4. The checker computes the value `1`'s type and merges via
   `apply_contextual_type(interner, /* expr */ literal_1, Some(number))`. Since
   `1` is a literal and the contextual `number` is not a union, the function
   falls through to the subtype check: `1 <: number`, so it returns `1` (the
   more specific expression type), letting freshness/widening policy decide
   later.
5. For `y`, `get_property_assignment_type` returns `string` (not
   `string | undefined`) because `strip_optional_undefined` is set, so the
   present assignment `"hi"` is checked against `string`.

For a **discriminated union** target `{ ok: true; v: number } | { ok: false }`,
`get_property_type_inner` recurses per union member and combines with
`union_preserve_members`, so the contextual type of `ok` is `true | false`,
never widened to `boolean` — exactly tsc's behavior.

## `apply_contextual_type`: the merge rule

After the checker has *both* the expression's own type and the contextual type,
it merges them with `apply_contextual_type(interner, expr_type,
Some(ctx_type))` (`core.rs`). This is bidirectional inference's final step, and
its ordering encodes several tsc parity rules:

1. **`expr_type == any` stays `any`.** A contextual property type influences
   widening and freshness but never *overwrites* an `any` value. Substituting
   here once turned a shorthand `{ value }` (`value: any`) into the union of
   contextual property types across a discriminated union and produced a false
   `TS2322`.
2. If `expr_type` is `any`/`unknown`/`error` (the non-`any` cases), use the
   contextual type — the expression carried no information.
3. If `expr_type == ctx_type`, return it.
4. If `expr_type` is a `Literal` and `ctx_type` is a `Union`, **preserve the
   literal** (it is more specific than the union).
5. If `ctx_type` is a `Union` and `expr_type` equals or is assignable to any
   member, use `expr_type`.
6. If `expr_type` is assignable to `ctx_type`, use `expr_type`.
7. **Default: prefer `expr_type`.** Crucially the function never narrows
   `expr_type` *down* to a narrower contextual type. If `ctx = "foo"` and
   `expr = string`, it returns `string`; pre-narrowing to `"foo"` would mask the
   real `TS2322` that the assignability checker is responsible for catching.
   `apply_contextual_type` deliberately refuses to do the assignability
   checker's job.

The function reuses a single `SubtypeChecker` (`relations::subtype`) across all
its membership/assignability probes (`checker.reset()` between checks) to avoid
re-allocating per member.

## Reverse inference, path 1: structural mapped-to-mapped (`compat_mapped.rs`)

The first "reverse inference" machinery is *not* in `contextual/`; it lives in
`CompatChecker` and handles assignability between two mapped types or between a
source and a homomorphic mapped target *when neither can be concretely
expanded* (because the source `T` is still generic). It is reverse inference in
the structural sense: it reasons about `{ [K in keyof T]: ... }` by matching the
*shape* of source and target without instantiating `T`. See
[solver-relations](solver-relations.md) and
[solver-mapped-and-tuple-shards](solver-mapped-and-tuple-shards.md) for the
surrounding relation machinery.

Key methods (all `pub(super)` on `CompatChecker<'a, R>`):

- **`is_source_assignable_to_homomorphic_mapped_target`** — is `S` assignable to
  `M<S>` where `M` maps over `keyof S`? It first rejects filtering `as`-clauses
  and `Required`-style optional removal (a mapped removing optionality demands
  properties the source may lack), extracts `mapped_source` via
  `keyof_inner_type`, requires `homomorphic_mapped_sources_match`, then checks
  the template either as a direct `T[K]` index-access identity or via a real
  subtype check of `mapped_source[K]` against the template.
- **`homomorphic_mapped_sources_match`** — the structural identity test that
  underpins everything: equal `TypeId`s; equal type-parameter *names*; or
  recursively-equal `IndexAccess` parts. This is the rule that lets `T` match
  `T`, `T[K]` match `T[K]`, etc. without evaluation.
- **`check_mapped_to_mapped_assignability`** — the workhorse for `Readonly<T>`
  vs `Partial<T>` and friends. It first tries a fast `flatten_mapped_chain`
  (which collapses nested homomorphic chains like `Partial<Readonly<T>>` and
  returns `None` for any `as`-renamed mapped, making name-type compatibility
  implicit), comparing key constraints (`mapped_key_constraint_covers`) and
  sources. The fallback substitutes the *target's* iteration variable with the
  *source's* (`TypeSubstitution::single`), applies the source/target optional
  modifiers (an added `?` makes the template effectively `template | undefined`),
  pushes a `type_param_equivalences` entry so the subtype checker treats the two
  iteration variables as equal, and finally does either a structural template
  comparison (`mapped_template_structurally_assignable`) or a real subtype check
  — restoring the equivalence stack via `truncate(equiv_start)` on every exit.
- **`mapped_template_structurally_assignable`** — recurses through
  `Application` expansion, both `Conditional` branches, `IndexAccess` part
  matching, union membership, and intersection membership to compare two
  templates that still contain `T[K]`.

`structurally_same_recursive_member` (depth-bounded at 8) is the deep structural
equality used by `union_structurally_contains_source` — it compares
tuples, arrays, conditionals, applications, unions, mapped types, objects, and
`Lazy(DefId)` identity, terminating the recursion when `depth == 0` (returning
`true` to favor the structural match rather than spuriously diverging).

## Reverse inference, path 2: homomorphic reverse-mapped (`reverse_mapped.rs`)

The second machinery is the genuine *value-to-type* reverse inference. Given an
argument like `{ a: 1, b: "x" }` flowing into a parameter typed
`{ [K in keyof T]: Box<T[K]> }`, it recovers `T = { a: ..., b: ... }` by
reversing the mapped template per property. It runs inside the inference engine
(`InferenceContext`), so it is owned by `operations/constraints`, not
`contextual/`. See [solver-inference](solver-inference.md) and
[solver-call-evaluator-and-inference-kernel](solver-call-evaluator-and-inference-kernel.md)
for the engine that calls it.

`constrain_reverse_mapped_type` (`reverse_mapped.rs`) drives it:

1. For each property of the *source* object shape, it reads the source property
   type — preferring the **display** property type
   (`get_display_properties`) so as-written literal types survive even when the
   canonical checking type is widened.
2. It substitutes the iteration variable `K` with the property-key literal
   (`literal_key_for_property_name`, which yields `Number(1)` for a bare numeric
   key `{ 1: ... }`) and instantiates the template.
3. If the instantiated template is a `T[K]` index-access through `T`'s *upper
   bound* rather than the placeholder, it rebinds the access to the inference
   placeholder so the template still reconstructs `T`.
4. `reverse_infer_through_template` reverses the source property type back
   through the (instantiated) template to find what `T[K]` must be.
5. On reversal **failure**, it does not give up: a source property that is a
   function with only `any`-typed parameters (untyped method shorthand) yields
   `unknown` (matching tsc's `getPartiallyInferableType`), *unless* the source
   property is a nullish union (`Document | null`, via
   `prop_source_has_nullish_union`), in which case it yields `any` so a later
   `out.prop.sub` access does not raise `TS18046` while the outer assignability
   check still rejects the nullish member and emits the expected `TS2345`. This
   approximates tsc's lazy `ReverseMappedType` materialization.
6. It reverses the mapped *modifiers*: an added `?`/`readonly` is removed in the
   reconstruction; a removed one is added back; `None` preserves the source's.
7. The reconstructed properties become an object shape that becomes the inferred
   `T`. The inference is only committed when at least one property (or index
   signature) was actually reversed (`any_reversed`).

`constrain_reverse_mapped_tuple` is the tuple-shaped analogue (skipping rest
elements, which "complicate reverse inference"). The whole module guards against
cycles with a per-pair `reverse_mapped_visited` set and a `reverse_mapped_depth`
counter, plus a thread-local `REVERSE_MAPPED_VISITED_POOL` scratch
`FxHashSet<TypeId>` reused across `type_contains_placeholder` calls to avoid
per-call allocation.

## How the checker supplies callable contextual types

The extractors read a `FunctionShape`. The bridge that turns a possibly-overloaded
`Callable` (or `Application` of one) into a single shape is
`CallEvaluator::get_contextual_signature_for_arity_inner`
(`operations/core/call_evaluator.rs`), reached through the query-boundary
wrappers `get_contextual_signature_*_with_compat_checker`
(`operations/core/call_resolution.rs`) and the checker re-exports in
`query_boundaries/checkers/call.rs`. Its `combine_contextual_signatures` helper:

- returns the single signature unchanged when there is one;
- refuses (`None`) to flatten a **mixed-arity** overload set — flattening would
  widen shorter overloads through trailing optionals;
- refuses (`None`) when any signature is generic and there are multiple —
  matching `getIntersectedSignatures`;
- otherwise builds a synthetic signature whose per-index parameter type is the
  `any`-filtered, `union_literal_reduce`d combination across signatures.

This is why `get_parameter_type_for_call` in `core.rs` reaches for
`get_contextual_signature_for_arity_with_compat_checker` on the `Application`
branch *before* unwrapping to the base: it preserves instantiation.

## Caches and invariants

- **No dedicated contextual cache.** `ContextualTypeContext` is a stateless,
  per-query wrapper; each call rebuilds child contexts and re-extracts. The
  expensive shared caches it *relies on* live elsewhere: `evaluate_type`'s
  evaluation cache (see [solver-evaluation](solver-evaluation.md)), the relation
  caches behind `query_relation` / the `SubtypeChecker` (see
  [solver-relations](solver-relations.md)), and the interner's structural
  identity (see [solver-types-intern-def](solver-types-intern-def.md)).
- **Contextual signature memoization** is opt-in: `get_contextual_signature_cached`
  threads a `QueryDatabase` so the flattened-signature instantiation can use
  `instantiate_type_cached`. The checker always uses the `_cached_*` variants
  via `query_boundaries/checkers/call.rs`.
- **`contextual_sensitivity_cache`** on `CallEvaluator`
  (`RefCell<FxHashMap<TypeId, bool>>`) memoizes `is_contextually_sensitive` to
  keep that check from going exponential; it is operation-local and reported in
  `cache_statistics`.
- **Canonicalization invariant.** Every union the extractors build goes through
  a `db.union*` constructor, which canonicalizes member order — so a contextual
  result is stable under permutation of the source union, keeping downstream
  caches keyed on it stable. `ThisTypeMarkerExtractor::visit_union` documents
  this explicitly.
- **Recursion guards.** `rest_argument_element_type_inner` is depth-capped at 8;
  `core.rs`'s `Conditional` branches skip a branch that equals `expected`
  (self-recursive aliases); `reverse_mapped.rs` carries an explicit
  visited-pair set plus depth counter; `structurally_same_recursive_member` is
  bounded at depth 8.

## Edge cases and tsc parity

- **`Function` intrinsic ⇒ `any` parameters.** Returning `None` for a `Function`
  target produced false `TS7006` for `T extends Function` callbacks;
  `is_function_boxed_or_intrinsic` short-circuits to `TypeId::ANY` (handles the
  intrinsic, the boxed `Function`, and a `Lazy(DefId)` boxed-`Function` alias).
- **Disagreeing callable union members ⇒ no context.** A union of callables
  whose parameter types differ at an index provides *no* contextual parameter
  type (`get_parameter_type`'s `Union` arm), matching the spec rule that
  identical-ignoring-return-type call-signature sets are required; under
  `noImplicitAny` this is what produces the expected `TS7006` for
  `IWithCallSignatures | IWithCallSignatures3`. But a union mixing a *generic
  overload's own free type parameter* with a concrete sibling widens the
  type-parameter slot to its constraint (or `any`), so `Array.reduce`'s
  `(prev: T) => T | (prev: U) => U` still contextually types the accumulator.
- **Mixed generic/non-generic overloads ⇒ no context.** Both
  `ParameterExtractor::visit_callable` and `ParameterForCallExtractor::visit_callable`
  return `None` for `{ (x: string): string; <T>(x: T): T }`, mirroring
  `getIntersectedSignatures`.
- **Optional property assignment uses the declared type.** `{ x: 1 }` against
  `{ x?: number }` is checked against `number` (assignment path,
  `strip_optional_undefined`), not the read-side `number | undefined`.
- **Discriminated-union property context preserves literals**
  (`union_preserve_members`), so `success` against `{ success: false } |
  { success: true }` is `false | true`, never `boolean`.
- **Index-signature `any` is filtered.** A union member contributing `any` from
  `{ [k: string]: any }` alongside specific members is dropped before unioning,
  so literal types are not widened by the index `any`.
- **Conditional true-branch parameter substitution.** In a call position,
  `get_parameter_type_for_call` substitutes the conditional's check type with
  `check & extends` inside callback parameter slots
  (`apply_conditional_true_branch_param_substitution`), so a parameter typed
  `(n: Check) => ...` becomes `(n: Check & Extends) => ...` to prevent false
  `TS2345` inside callbacks while keeping direct-argument contexts intact.
- **Per-index mapped templates.** For a deferred homomorphic mapped contextual
  type over positional keys, `try_mapped_per_index_template`
  substitutes `K` with the index literal before evaluation, but refuses when key
  remapping (`as`) or a nested same-name source/key collision
  (`template_has_nested_same_name_source_key_collision`) would misalign indices —
  falling back to the non-positional path rather than producing a wrong
  per-element type.
- **Empty-tuple rest context.** A callback `...args` past the end of a
  fixed-arity contextual signature gets contextual type `[]` (not the default
  `any[]`), so the rest can be spread into another fixed-arity callee without a
  spurious `TS2556` (`extract_rest_param_type_at`).
- **`IndexAccess` benefit-of-the-doubt.** When a callable shape is unknowable
  because the position resolves to an unevaluatable `T[K]`,
  `is_rest_parameter_position` / `allows_non_tuple_spread_position` return
  `true` to suppress a false `TS2556` at a generic call site.

## Cross-references

- [solver-caches-objects-contextual-compat](solver-caches-objects-contextual-compat.md) — the broader caches/objects/compat surface this doc drills into.
- [solver-inference](solver-inference.md) — how contextual types feed the generic inference fixpoint that `reverse_mapped.rs` participates in.
- [solver-call-evaluator-and-inference-kernel](solver-call-evaluator-and-inference-kernel.md) — `CallEvaluator`, `get_contextual_signature_for_arity`, and the call-side plumbing.
- [solver-relations](solver-relations.md) — `CompatChecker`, `SubtypeChecker`, and `query_relation` that the compat_mapped path and `apply_contextual_type` lean on.
- [solver-mapped-and-tuple-shards](solver-mapped-and-tuple-shards.md) — mapped/tuple evaluation that backs `evaluate_type` here.
- [solver-evaluation](solver-evaluation.md) — the `evaluate_type` engine used at every wrapper-normalization step.
- [checker-calls-signatures-generics](checker-calls-signatures-generics.md) — the checker-side call orchestration that supplies callable contextual types.
- [checker-context-and-state](checker-context-and-state.md) — `CheckerState` and `contextual_type_for_expression`, the entry point into this subsystem.
- [solver-types-intern-def](solver-types-intern-def.md) — `TypeId`, `TypeData`, and `Lazy(DefId)` resolution underlying every lookup here.
