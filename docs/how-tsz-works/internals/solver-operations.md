# Operations: Binary, Unary, and Index/Property Access Type Computation

This document covers how `tsz` computes the *result type* (and the operand
diagnostics) of operator expressions: binary operators (`+`, `-`, `*`, `%`,
`**`, the bitwise/shift family, comparisons, equality, `&&`/`||`/`??`, `in`,
`instanceof`, comma), prefix/postfix unary operators (`+`, `-`, `~`, `!`,
`++`/`--`, `typeof`, `void`, `delete`), compound assignment (`+=`, `&&=`,
`??=`, …), and property/element access (`obj.p`, `obj["k"]`, the type-of-an
indexed-access expression). It is the operator-shaped slice of the solver's
`operations` directory plus the thin checker orchestration that drives it.

The split is the same one that runs through the whole compiler: the **solver**
owns *what* type an operator produces given the operand `TypeId`s (pure logic,
no AST), and the **checker** owns *where* — it walks the AST, fetches operand
types, decides which contextual type flows to which operand, emits diagnostics
at source locations, and threads the answers back. The two key solver structs
are `BinaryOpEvaluator` (in
`crates/tsz-solver/src/operations/binary_ops.rs`) and `PropertyAccessEvaluator`
(in `crates/tsz-solver/src/operations/property.rs`); the checker drivers are
`CheckerState::get_type_of_binary_expression_with_request` (in
`crates/tsz-checker/src/types/computation/binary.rs`) and
`CheckerState::get_type_of_element_access_with_request` (in
`crates/tsz-checker/src/types/computation/access.rs`). The bridge between them
is the `query_boundaries` layer, which is the *only* place allowed to construct
these solver evaluators.

This is the middle-tier companion to
[solver-evaluation](solver-evaluation.md) (which owns `T[K]`/`keyof`/mapped
reduction), [solver-narrowing](solver-narrowing.md) (truthiness/nullishness
splitting that the logical operators reuse), [solver-relations](solver-relations.md)
(the subtype kernel logical-result reduction calls), and the checker-side
[checker-flow-and-narrowing](checker-flow-and-narrowing.md) and
[checker-assignability-gateway](checker-assignability-gateway.md).

---

## Owns / Must not own

**The operations layer (solver side) owns:**

- The arithmetic/string/bigint coercion rules for `+` and the other arithmetic
  operators: `evaluate_plus`, `evaluate_arithmetic`, `evaluate_comparison`,
  `evaluate_logical` on `BinaryOpEvaluator`.
- The primitive-class classification used by every operator:
  `is_number_like`, `is_string_like`, `is_bigint_like`, `is_boolean_like`,
  `is_symbol_like`, built on the `primitive_visitor!`-generated `TypeVisitor`
  implementations.
- The logical-operator result-type math (`&&`/`||`/`??`), including the
  `NonNullable<T>` approximation (`apply_non_nullable_approximation`), subtype
  reduction of the result union (`union2_subtype_reduce`), and the
  display-order fix-up.
- `instanceof` left/right operand validity (`is_valid_instanceof_left_operand`,
  `is_valid_instanceof_right_operand`), arithmetic-operand validity
  (`is_arithmetic_operand`), and computed-property-key validity
  (`is_valid_computed_property_name_type`, `is_valid_mapped_type_key_type`).
- Property resolution: walking a type's apparent members, index signatures,
  unions/intersections, and deferred (`Application`/`Lazy`/`IndexAccess`) shapes
  to answer `obj.p` — `PropertyAccessEvaluator::resolve_property_access`.

**It must not own** (these belong to siblings):

- AST traversal, operand ordering, contextual-type propagation, source spans,
  or *emitting* diagnostics. The evaluator returns a structured
  `BinaryOpResult::TypeError { left, right, op }` and the checker decides what
  diagnostic code and anchor that becomes (`emit_binary_operator_error` in
  `crates/tsz-checker/src/error_reporter/operator_errors.rs`).
- The actual `T[K]` / `keyof` / mapped reduction. `PropertyAccessEvaluator`
  *calls* `evaluate_index_access_with_options` / `evaluate_type` on the
  `QueryDatabase`; the rewrite rules live in
  `crates/tsz-solver/src/evaluation/evaluate_rules/index_access.rs` (see
  [solver-evaluation](solver-evaluation.md)).
- The subtype relation. Logical-result reduction asks
  `crate::relations::subtype::is_subtype_of_with_db`; it never reimplements the
  kernel (see [solver-relations](solver-relations.md)).
- Truthiness/nullishness splitting. `evaluate_logical` constructs a
  `crate::narrowing::NarrowingContext` and calls `narrow_by_truthiness`,
  `extract_definitely_falsy_type`, `narrow_to_falsy`, `narrow_by_nullishness`
  (see [solver-narrowing](solver-narrowing.md)).

---

## Module map

| Path | Role |
| --- | --- |
| `tsz-solver/src/operations/binary_ops.rs` | `BinaryOpEvaluator`, `BinaryOpResult`, the primitive-class visitors, and all operator result-type math. |
| `tsz-solver/src/operations/compound_assignment.rs` | Pure operator-token classification: `is_compound_assignment_operator`, `is_logical_compound_assignment_operator`, `map_compound_assignment_to_binary`, `fallback_compound_assignment_result`. |
| `tsz-solver/src/operations/property.rs` | `PropertyAccessEvaluator`, `PropertyAccessResult`, the deferred-property memo, recursion guard. |
| `tsz-solver/src/operations/property_helpers.rs` | Member-resolution helpers (mapped, primitive, array, application, intersection). |
| `tsz-solver/src/operations/property_visitor.rs` | `TypeVisitor` impl that walks each `TypeData` variant for property lookup. |
| `tsz-solver/src/operations/expression_ops.rs` | Conditional-expression type, template-expression type, best-common-type, object-spread merge. |
| `tsz-solver/src/evaluation/evaluate_rules/index_access.rs` | `T[K]` reduction (sibling: [solver-evaluation](solver-evaluation.md)). |
| `tsz-checker/src/types/computation/binary.rs` | `get_type_of_binary_expression_with_request`: the driver. |
| `tsz-checker/src/types/computation/binary_support.rs` | `SyntacticNullishness` classifier, `in`/`instanceof` checks, TS2367 display widening. |
| `tsz-checker/src/types/computation/nullish_coalescing.rs` | `??` left-operand diagnostics (TS2869/TS2871) and result-type helper. |
| `tsz-checker/src/types/computation/helpers.rs` | Unary operator type computation (`+`/`-`/`~`/`++`/`--`). |
| `tsz-checker/src/types/computation/access.rs` + `access_helpers.rs` | Element/property access drivers (`get_element_access_type`). |
| `tsz-checker/src/query_boundaries/common.rs` | `new_binary_op_evaluator` — the single construction point. |
| `tsz-checker/src/query_boundaries/type_computation/core.rs` | `evaluate_plus_chain`, `is_arithmetic_operand`, `write_target_logical_result_type`. |
| `tsz-checker/src/error_reporter/operator_errors.rs` | `emit_binary_operator_error` (TS2362/TS2363/TS2365/TS2469). |

The `BinaryOpEvaluator` is constructed *only* through
`query_boundaries::common::new_binary_op_evaluator` (see
`crates/tsz-checker/src/query_boundaries/common.rs`, `fn
new_binary_op_evaluator`). An architecture-contract test
(`crates/tsz-checker/src/tests/architecture_contract_tests/part_02.rs`) enforces
a *ceiling* on direct `BinaryOpEvaluator::new()` call sites outside
`query_boundaries/` and `tests/`, so the checker never bypasses the boundary.

---

## `BinaryOpEvaluator`: the pure operator kernel

The evaluator holds only a `&dyn QueryDatabase` (`interner`). Its single dispatch
entry is `evaluate` / `evaluate_with_context`:

```
evaluate_with_context(left, right, op, contextual_type)
  ├─ op not in {&&,||,??} and (left==NEVER or right==NEVER) -> Success(NEVER)
  ├─ "+"                              -> evaluate_plus
  ├─ "-" "*" "/" "%" "**" "&" "|"
  │   "^" "<<" ">>" ">>>"            -> evaluate_arithmetic
  ├─ "==" "!=" "===" "!=="           -> Success(BOOLEAN)   (overlap check is the checker's job)
  ├─ "<" ">" "<=" ">="              -> evaluate_comparison
  └─ "&&" "||" "??"                  -> evaluate_logical
```

`never` is the bottom type, so every non-logical operator on `never`
short-circuits to `Success(NEVER)`. The logical operators are excluded because
they have their own `never` handling (a `never` truthy or falsy slice selects
the other branch).

### Primitive classification (the visitors)

Operator typing is built on five predicates: `is_number_like`, `is_string_like`,
`is_bigint_like`, `is_boolean_like`, `is_symbol_like`. The first three are
generated by the `primitive_visitor!` macro, which produces a `TypeVisitor`
whose `Output = bool`. The macro feature flags decide how compound types fold:

- `check_union_all`: `visit_union` returns true only when *every* member matches
  (a union is number-like iff all members are).
- `check_constraint`: a `TypeParameter`/`Infer` recurses into its `constraint`
  (so `T extends number` is number-like).
- `recurse_enum`: a numeric enum member type recurses into its underlying type.
- `match_template_literal`: only `StringLikeVisitor` sets this — a template
  literal type is string-like.
- `check_intersection_any`: `visit_intersection` returns true when *any* member
  matches (so `string & Brand` is string-like).

Each predicate has a fast path: if the `TypeId` is `NUMBER`/`STRING`/`BIGINT`
or `ANY` it returns true without a visit, and an `is_intrinsic()` `TypeId` that
is not the matching primitive returns false without a visit (an important
detail: `BOOLEAN_TRUE`/`BOOLEAN_FALSE` lookup as `Literal(Boolean)`, so they
cannot match `IntrinsicKind::Number` — skipping the visit avoids a needless
walk). `any` reports true for *all* of `is_number_like`/`is_string_like`/
`is_bigint_like`, which is why downstream code must special-case `any` when it
needs to distinguish (see unary `bigint` below).

### `+`: string concatenation vs addition

`evaluate_plus` encodes `tsc`'s `+` overload resolution. In order:

1. `unknown + anything` → `Success(UNKNOWN)` (no cascading error).
2. `ERROR` operands are coerced to `ANY` — error acts like `any` so a name that
   failed to resolve (TS2304) does not also produce a spurious `+` error, while
   still inferring `string + error = string`.
3. Symbol operand → `TypeError` (the checker turns this into TS2469).
4. `any + anything` → `Success(ANY)`.
5. If *either* side is string-like, the result is `string` **provided the other
   side is a valid string-concat operand** (`is_valid_string_concat_operand`:
   number/boolean/bigint/null/undefined/void and any non-symbol object/function;
   `unknown` is explicitly *not* valid). Otherwise `TypeError` (→ TS2365).
6. `number-like + number-like` → `number`; `bigint-like + bigint-like` →
   `bigint`.
7. Anything else → `TypeError`.

There is a fast path for `+` chains: `evaluate_plus_chain(&[TypeId])` checks
whether every operand is the *exact* `NUMBER`, `BIGINT`, or `STRING` `TypeId`
(or any `ANY`), and returns the uniform result without per-node evaluation. The
checker pre-walks a left-leaning `+` tree, collects the leaf operand types, and
calls this through `query_boundaries::type_computation::core::evaluate_plus_chain`
(see `binary.rs` near line 82). A symbol operand bails the fast path so the
checker can still emit TS2469.

### Other arithmetic and the `>>>` special case

`evaluate_arithmetic` handles `-`, `*`, `/`, `%`, `**`, and the bitwise/shift
family `&`, `|`, `^`, `<<`, `>>`, `>>>`. It mirrors `+`'s `unknown`/`ERROR`/
symbol/`any` prologue (but `any` here yields `number`, not `any`), then:

- `>>>` is **number-only**: JavaScript does not define unsigned right shift on
  `bigint`, so `bigint >>> bigint` is a `TypeError` even though `<<`/`>>`/`&`
  accept `bigint` pairs.
- `number-like ⊕ number-like` → `number`; `bigint-like ⊕ bigint-like` →
  `bigint`; otherwise `TypeError`.

### Comparison

`evaluate_comparison` (`<`, `>`, `<=`, `>=`) requires the operands to be the
*same* orderable kind: both number-like → `boolean`, both string-like →
`boolean`, both bigint-like → `boolean`, both boolean-like → `boolean`. There is
deliberately **no** catch-all "orderable" check: `number < string` falls to
`TypeError` so the checker's `is_type_comparable_to` (the `Comparable` relation)
can make the final call. `any` on either side short-circuits to `boolean`.

### Logical operators (`&&`, `||`, `??`)

`evaluate_logical` is where the recent contextual rules live. It constructs a
`NarrowingContext` and computes the result from truthy/falsy or nullish slices:

- **`&&`** (`a && b`): the falsy part is `extract_definitely_falsy_type(left)`
  — *not* `narrow_to_falsy` — to match `tsc`'s `getDefinitelyFalsyPartOfType`.
  For `string && X` this means the result is `"" | X` (only `""` is *definitely*
  falsy), not `string | X`. If the truthy slice is `never` (left is always
  falsy) the result is `left`; if the falsy slice is `never` (left is always
  truthy) the result is `right`; otherwise `union2(falsy_left, right)`.
  - There is a **contextual callable suppression**: when a contextual target and
    `right` are both callable and `left` is a subtype of `boolean` (e.g.
    `x = y && fn` where `x` is function-typed), the false branch is dropped — the
    result is just `right` (or the falsy narrowing when the left is always
    falsy). This is the only place `contextual_type` is consulted.
- **`||`** (`a || b`): the truthy slice is `narrow_by_truthiness(left)`, then
  fed through `non_nullable_type_parameter_result`. If the falsy slice is `never`
  *and* the left is not a type parameter, result is `left`; if the truthy slice
  is `never`, result is `right`; otherwise `union2_subtype_reduce(truthy, right)`
  with a **display-origin fix-up** (see below).
- **`??`** (`a ?? b`): the non-nullish operand is `getNonNullableType(left)`.
  `unknown ?? X` is special: `getNonNullableType(unknown)` is the empty object
  `{}`, not `unknown`, so the result is `{} | X` (e.g.
  `Object.entries(data ?? {})` with `data: unknown`). For everything else the
  non-nullish slice is `narrow_by_nullishness(left, ExcludeNullish)`. If the
  nullish slice is `never`, result is `left`; if the non-nullish slice is
  `never`, result is `right`; otherwise the non-nullish slice gets the
  `NonNullable<T>` approximation and is `union2_subtype_reduce`-d with `right`.

#### The `NonNullable<T>` approximation

`non_nullable_type_parameter_result(original, narrowed)` (public alias
`apply_non_nullable_approximation`) replicates `tsc`'s `NonNullable<T>` for
unconstrained generics. When the truthy/non-nullish slice is an *unconstrained*
type parameter (`is_unconstrained_type_parameter`), it becomes `T & {}`
(`intersection2(narrowed, object(vec![]))`). For `X = {}` this reduces to `{}`
since `T & {} <: {}`. It distributes over unions: `NonNullable<D | E>` becomes
`(D & {}) | (E & {})` for the unconstrained members. **Constrained** type
parameters (`D extends string`) deliberately do *not* get `& {}` — their
constraint already determines assignability, and adding `& {}` to a
primitive-constrained param would wrongly pass the `object` keyword check.

#### Subtype reduction of the result union

`union2_subtype_reduce(left, right)` implements
`getUnionType([...], UnionReduction.Subtype)` for the `||`/`??` result: if one
side is a subtype of the other (via `is_subtype_of_with_db`), drop the subtype —
so `number[] | never[]` reduces to `number[]`. The one guard: an *empty object*
`{}` (`is_empty_object_type`) is never allowed to absorb a more specific object
(`{ a: string }` is not reduced to `{}`), matching `tsc`'s
`strictSubtypeRelation` for these result unions.

#### Display order

After forming the `||` result, `evaluate_logical` calls
`replace_union_origin_for_display`. `tsc` orders union members by type-creation
id. When the truthy side is a `NonNullable<T>` approximation synthesized *by this
very operation* (`T & {}` is newer than `right`), it must display last:
`t || u` renders `U | NonNullable<T>`. The fix-up records `[right, truthy_left]`
in that case, otherwise `[truthy_left, right]`. This is display-only metadata;
the type identity is unchanged.

---

## The checker driver: `get_type_of_binary_expression_with_request`

The driver in `binary.rs` is a hand-rolled iterative stack walk (a
`SmallVec<[(NodeIndex, bool); 4]>` of `(node, visited)` plus a `type_stack`) so
deep `+` chains and nested logical expressions never recurse on the Rust stack.
Each binary node is pushed unvisited, its operands typed, then revisited to
combine. The notable per-operator responsibilities the checker keeps:

1. **TS5076** ("`??` cannot be mixed with `||`/`&&` without parentheses"):
   detected syntactically on first visit by inspecting whether a child binary
   uses a conflicting logical operator.
2. **Contextual-type routing** for `&&`/`||`/`??`. The right operand of `&&`
   inherits the whole-expression contextual type. For `||`/`??` the right
   operand prefers the *outer* contextual type when present, and otherwise falls
   back to the left type with nullish removed (`remove_nullish`) — but only for
   context-sensitive right operands (arrow/function/object/array/conditional).
   This is what gives `let g = f || (x => …)` the parameter type from `f`.
3. **Literal preservation** for logical operands. `logical_operand_is_primitive_literal`
   (in `binary_support.rs`) gates the `ctx.preserve_literal_types` flag to
   *syntactic* primitive-literal operands so the logical evaluator can prove
   "always truthy"/"always falsy" (e.g. `"baz" || z`), without suppressing
   array/object-literal element widening. After commit #13997 this preservation
   is applied to the **right** operand symmetrically (`const r: Strategy = v ||
   'warn'` keeps `'warn'` so the result is the literal union, not `string`).
4. **Comma** (TS2695 "left side of comma is unused"), **`in`**
   (`check_in_operator`), and **`instanceof`** (`check_instanceof_operator`) are
   dispatched to dedicated methods; assignment / compound assignment route to
   `check_assignment_expression` / `check_compound_assignment_expression`.
5. For the arithmetic/bitwise/comparison families, the checker evaluates types
   (`evaluate_type_for_binary_ops`), runs **per-operand** validity checks (a
   subtlety below), calls `evaluator.evaluate`, and on `TypeError` calls
   `emit_binary_operator_error`.

### Per-operand `any` validity (TS2362/TS2363)

`BinaryOpEvaluator::evaluate_arithmetic` returns `Success(NUMBER)` as soon as one
operand is `any`. But `tsc` still requires the *other* operand to be a valid
arithmetic type. So the checker pre-checks: when `eval_left` (or `eval_right`)
is `any`/`ERROR`, it asserts the other side via `is_arithmetic_operand` (and the
checker-only `is_enum_type`), emitting TS2362 (left) / TS2363 (right) if it
fails. Without this, `any * someObject` would silently pass. The same per-operand
gate runs for the bitwise family (`binary.rs` near line 1034).

### Boxed primitives, `unknown`, `**` grammar, shift simplification

The checker handles cases the solver kernel cannot, because they need symbol /
option / source context:

- **Boxed primitives** (`Number`/`String`/`Boolean` interface types): checked
  *before* `evaluate_type_for_binary_ops` (which would convert `Number` →
  `number`). `Number ** 2` is a TS2362, not a silent success.
- **`unknown`** with non-equality operators emits TS18046 under
  `strictNullChecks` (via `error_is_of_type_unknown`); equality operators are
  allowed on `unknown`.
- **`**` grammar** (TS17006/TS17007): a unary or type-assertion LHS of `**`
  (`-x ** y`, `<T>x ** y`) is a grammar error; when it fires, the checker pushes
  `ERROR` and skips arithmetic checks to avoid cascading TS2362. **TS2791**:
  `bigint ** bigint` requires target ≥ ES2016.
- **TS6807** "shift is identical to" suggestion: only emitted for `<<`/`>>`/`>>>`
  with a constant shift ≥ 32 *inside an enum-member initializer* — the checker
  walks ancestors to confirm an `ENUM_MEMBER` parent before emitting.

### Equality and overlap (TS2367, TS2839)

Equality operators always type as `boolean` in the evaluator. The "no overlap"
diagnostic (TS2367) is the checker's job: it computes narrowed comparison types
(`typeof_result_type_if_typeof`, `literal_type_from_initializer`,
`apply_flow_narrowing` for property/element accesses), then calls
`types_have_no_overlap`. There is a loop-narrowing guard
(`declared_type_has_overlap_in_loop`) so flow-narrowed-too-far loop variables do
not produce false TS2367. TS2839 ("compares objects by reference") fires when an
object/array literal is an equality operand. Cross-primitive-family display
widening lives in `widen_for_ts2367_cross_family_display` /
`get_primitive_family` (so `'symbol' and 'boolean'`, not `'symbol' and 'true'`).

---

## Nullish coalescing diagnostics: TS2869 / TS2871

The `??` left-operand checks are **purely syntactic**, matching `tsc`'s
`checkNullishCoalesceOperandLeft`. The classifier is
`CheckerState::get_syntactic_nullishness` (in `binary_support.rs`), returning
`SyntacticNullishness::{Always, Sometimes, Never}` — it never consults the
operand's static type. Both checks anchor at
`skip_parenthesized_and_assertions(left)`, i.e. `tsc`'s `skipOuterExpressions(left,
All)`: through parentheses, `as`/`satisfies`/`<T>` assertions, and non-null `!`.
So `(1 as any) ?? x` classifies on the literal `1`, and `null! ?? x` on `null`.

The classifier rules (`get_syntactic_nullishness`):

| Syntax | Result |
| --- | --- |
| `await`, call, tagged template, element access, meta-property, `new`, property access, `yield`, `this` | `Sometimes` |
| binary `\|\|`/`\|\|=`/`&&`/`&&=` | `Sometimes` |
| binary `??`/`??=`/`=`/comma | nullishness of the **right** operand (recurse) |
| any other binary (arithmetic/comparison/bitwise) | `Never` |
| conditional `c ? a : b` | `Never` iff both branches `Never`; `Always` iff both `Always`; else `Sometimes` |
| `null` keyword | `Always` |
| identifier `undefined` | `Always` |
| other identifier | `Sometimes` |
| everything else (literals, object/array literal, function/arrow/class, regex, template) | `Never` |

`nullish_coalescing_left_diagnostics` (in `nullish_coalescing.rs`) maps the
classification to a diagnostic node:

- `Never` → **TS2869** "Right operand of `??` is unreachable because the left
  operand is never nullish."
- `Always` → **TS2871** "This expression is always nullish." TS2871 still falls
  through to the shared result-type computation, because an asserted type can
  retain a non-nullish slice (`(null as string | null) ?? "x"` produces
  `string`).
- `Sometimes` → no diagnostic.

Both anchor at `target` (the skipped-outer node), so the error sits on `1`, not
`(1 as any)`. Commit #13999 widened the recognized forms from a literal handful
to this full classifier.

### `??` result type in the checker

`nullish_coalescing_result_type` (in `nullish_coalescing.rs`) computes the
result once the left is split. It re-applies the `unknown → {}` rule
(`factory().object(Vec::new())`), applies `apply_non_nullable_approximation` for
unconstrained generics, then reduces: if the non-nullish slice equals `right`,
or `right` is a subtype of the non-nullish slice (and `right` is not a *fresh*
object unless it is an empty object literal), the result is the non-nullish
slice; if the non-nullish slice is a subtype of `right`, the result is `right`;
otherwise `union2(non_nullish, right)`. The fresh-object guard prevents an
excess-property-checkable fresh literal RHS from being silently absorbed.

---

## Unary operators

Unary type computation lives in
`crates/tsz-checker/src/types/computation/helpers.rs`. The result-type rules per
operator:

| Operator | Result | Notes |
| --- | --- | --- |
| `!` (logical not) | `boolean` | always |
| `+` (unary plus) | `number` | **TS2736** on `bigint` operand (`+1n` throws at runtime); on `any` → `number` |
| `-` (unary minus) | `bigint` for bigint operand, else `number` | `-1n` is valid; preserves bigint literals (`const x: 0n = -0n`) |
| `~` (bitwise not) | `bigint` for bigint operand, else `number` | TS2469 on symbol |
| `++` / `--` | same numeric type as operand (bigint or number) | TS2356 invalid operand; TS2357 invalid l-value |

The recurring **`any` yields `number`, not `bigint`** rule appears verbatim at
three sites (`helpers.rs` near lines 469, 518, 602). The subtlety:
`BinaryOpEvaluator::is_bigint_like` returns `true` for `any` (and the `ERROR`
sentinel), because `any` is every primitive. But a unary arithmetic operator
(`+`, `-`, `~`, `++`, `--`) on an `any` operand produces `number` in `tsc`, not
`bigint`. So each site resolves the operand
(`evaluate_type_with_env(operand_type)`) and only returns `BIGINT` when the
*resolved* type is neither `ANY` nor `ERROR` and is genuinely bigint-like:

```rust
if resolved != TypeId::ANY
    && resolved != TypeId::ERROR
    && evaluator.is_bigint_like(resolved)
{ TypeId::BIGINT } else { TypeId::NUMBER }
```

Without this guard `--x`/`+x` on `any` became `bigint`, poisoning downstream
comparisons with false TS2367 and arithmetic with false TS2365. `typeof` (→
`string`, or a precise typeof-string literal union for narrowing), `void` (→
`undefined`), and `delete` (→ `boolean`) are handled elsewhere in the unary
dispatch; `unary_operator_name` (in `binary_support.rs`) maps the tokens to the
strings used in TS17006 messages.

---

## Compound assignment

`compound_assignment.rs` is pure token bookkeeping — no type math. The four
`const fn` classifiers are the canonical predicate set used by flow, narrowing,
and emit:

- `is_compound_assignment_operator` — any `op=` form.
- `is_assignment_operator` — `=` or a compound form.
- `is_logical_compound_assignment_operator` — `&&=`, `||=`, `??=` (these
  short-circuit; the RHS is only evaluated when the LHS fails the guard).
- `map_compound_assignment_to_binary` — `+=` → `"+"`, `??=` → `"??"`, etc., so
  `check_compound_assignment_expression` can reuse `BinaryOpEvaluator::evaluate`.
- `fallback_compound_assignment_result` — when the operand type is otherwise
  unresolved, the arithmetic/bitwise compound forms produce `number`, and `+=`
  produces `number` only when the RHS is a number literal/`number`.

The "write target" path (`get_type_of_write_target_base_expression` in
`binary_support.rs`) is used when a `||`/`??` expression appears as the *base* of
an assignment target (`(options || {}).a = …`). It routes through
`write_target_logical_result_type` (in
`query_boundaries/type_computation/core.rs`), which uses the same
`NarrowingContext` truthy/non-nullish slices but normalizes object union
members for the write context, returning either a concrete write type or
`FallbackToLogicalExpression`.

---

## Property and element access

`PropertyAccessEvaluator` resolves `obj.p` / `obj["k"]` to a
`PropertyAccessResult`:

| Variant | Meaning |
| --- | --- |
| `Success { type_id, write_type, from_index_signature }` | Found. `write_type` is the setter-parameter type for divergent accessors (TS 4.3+); `from_index_signature` distinguishes an index-signature hit (TS4111) from a declared member. |
| `PropertyNotFound { type_id, property_name }` | Drives TS2339. |
| `PossiblyNullOrUndefined { property_type, cause }` | Drives TS2531/TS2532/TS18047/TS18048; carries the non-null property type for optional-chaining recovery. |
| `IsUnknown` | Access on `unknown` (TS18046). |

The evaluator is constructed **per access** (its only mutable state is a
recursion guard and a per-access memo) through the
`query_boundaries::property_access` wrappers — `resolve_property_access`,
`resolve_property_access_with_resolver`, `resolve_property_access_raw_this`,
etc. The checker never instantiates `PropertyAccessEvaluator` directly.

### Resolution flow

`resolve_property_access(obj_type, prop_name)` interns the name once
(`intern_string`) and calls `resolve_property_access_atom`, which is `Atom`-keyed
the rest of the way (integer comparisons, no re-hashing). The inner
`resolve_property_access_inner` dispatches on the object's `TypeData` through the
`property_visitor.rs` `TypeVisitor`:

```
resolve_property_access_atom(obj, atom)
  └─ resolve_property_access_inner(obj, atom)
       ├─ Object/ObjectWithIndex  -> named member, then index signature
       ├─ Union                   -> resolve on each member, combine (every member must have it)
       ├─ Intersection            -> first member that has it
       ├─ Array/Tuple             -> Array.prototype / element members
       ├─ Application / Lazy       -> instantiate base, walk heritage (memoized)
       ├─ IndexAccess(o, i)        -> evaluate_index_access_with_options, then re-resolve
       ├─ intrinsic primitives    -> apparent type (Number/String/Boolean prototype)
       └─ ...
```

For the top-level entry only, when the inner resolver returns the deferred `ANY`
fallback for a conditional, `resolve_property_access_atom` re-checks the
*apparent* (branch-union) type to catch genuine not-found cases, without
disturbing the union/intersection handlers that depend on
`is_deferred_any_fallback_member`.

`this` substitution is handled by `bind_object_receiver_this`: when the resolved
member contains `this` (`contains_this_type`), the receiver is nominalized
(`nominalize_object_receiver` → `Lazy(DefId)` when the resolver can map the
symbol) and `this` is substituted at return positions via
`substitute_this_type_at_return_position`. The `skip_this_binding` flag preserves
raw `ThisType` when resolving through a type-parameter constraint so the checker
can supply the correct receiver.

### The type of an indexed-access *expression*

`obj["k"]` and the type-level `T[K]` share machinery but are entered
differently. The checker driver
`get_type_of_element_access_with_request` (in `access.rs`) decides between
property-name lookup (when the index is a string/number *literal* —
`resolve_property_access_with_env`) and the general indexed form
(`get_element_access_type` → the solver's `evaluate_index_access_with_options`).
In **write context** it deliberately preserves the canonical
`IndexAccess(object, index)` shell rather than resolving through a generic
receiver's constraint (`factory().index_access(...)`), so that
`obj[k] = undefined` (where `k: K extends keyof typeof obj`) is correctly
rejected instead of being widened to `T | undefined` on the read side. The actual
`T[K]` reduction (tuple literal indexing, union distribution `T[A|B] = T[A] |
T[B]`, mapped substitution, the `noUncheckedIndexedAccess` `| undefined`
addition) lives in `evaluate_index_access` (`index_access.rs`) and is documented
in [solver-evaluation](solver-evaluation.md). `PropertyAccessEvaluator` *uses*
that reduction (`evaluate_index_access_with_options`) when it walks into an
`IndexAccess` object type; it does not reimplement it.

### Computed-property-name and mapped-key validity

`BinaryOpEvaluator` (not `PropertyAccessEvaluator`) also owns key-type validity:

- `is_valid_computed_property_name_type` — TS2464: a computed key must be
  `string | number | symbol | any` (literals/enums/template literals/unique
  symbols included); a union is valid iff every member is. Deferred types that do
  not evaluate to a primitive key (e.g. the `Symbol` interface `Lazy(DefId)`)
  are **invalid** in concrete contexts.
- `is_valid_mapped_type_key_type` — the same `is_valid_key_type_impl` with
  `defer_unresolved = true`, so unresolvable generic forms (a generic
  `Application`/`Conditional`/`IndexAccess` whose index constraint relates to
  `keyof object`) are conservatively accepted and re-checked at instantiation.

Both share `is_valid_key_type_impl`, which uses an `FxHashSet<TypeId>` `seen`
set to guard recursion through self-referential constraints.

---

## Caches and invariants

| Cache / guard | Location | Invalidation |
| --- | --- | --- |
| `deferred_property_memo` | `PropertyAccessEvaluator` field (`property.rs`) | Scoped to one access tree — the evaluator is constructed per access, so the table is dropped when the access completes. No cross-access invalidation needed. Collapses cyclic interface-heritage re-walks from combinatorial to O(1). |
| `RecursionGuard<TypeId>` (`RecursionProfile::PropertyAccess`) | `PropertyAccessEvaluator::guard` (`property.rs`) | Reset between accesses. `max_depth = 50`, `max_iterations = 100_000` (see `crates/tsz-solver/src/recursion.rs`). Prevents infinite property recursion; `PropertyAccessGuard`'s `Drop` impl calls `leave` to balance enter/leave even on early return. |
| `split_nullish_cache` | `flow_shared.narrowing_cache` (checker, `types/queries/core.rs`) | Memoizes `split_nullish_type(TypeId) -> (non_nullish, cause)` so `??`, `in`, and unary-operand nullish checks do not re-split. Keyed by `TypeId`; lives in the per-check flow-shared state. |
| `node_types` | checker context | Read in `get_type_of_write_target_base_expression` to short-circuit non-binary write-target base resolution (deep optional chains `a?.b?.c?.d`). |
| Display-origin metadata | `replace_union_origin_for_display` (interner) | Recorded by `evaluate_logical` for `||` result unions; affects diagnostic display order only, not type identity. |

**Invariants the operations layer must hold:**

- The evaluator is **pure**: it takes `TypeId`s and returns `BinaryOpResult` /
  `PropertyAccessResult`; it never reads the AST, never emits a diagnostic, and
  never reads printer output. The checker turns `TypeError`/`PropertyNotFound`
  into a coded diagnostic.
- `BinaryOpEvaluator` is constructed only through `new_binary_op_evaluator`; the
  architecture-contract test ceiling enforces it.
- Equality operators (`==`/`!=`/`===`/`!==`) **always** type as `boolean`,
  even when the operands have no overlap — TS2367 is a separate warning, not a
  type change.
- A failed *arithmetic* op (non-comparable operands) types as `any` in the
  checker fallback (`binary.rs` near line 1467), so downstream destructuring/key
  checks see `any` rather than a misleading concrete type; a failed *bitwise* op
  types as `number` (`operator_error_result_type`); a failed comparison types as
  `boolean`.

---

## Edge cases and `tsc` parity

- **`unknown ?? X` is `{} | X`, not `unknown | X`.** `getNonNullableType(unknown)`
  is `{}`. Both `evaluate_logical` (solver) and `nullish_coalescing_result_type`
  (checker) special-case `unknown` to `object(vec![])`. Witness:
  `Object.entries(data ?? {})` with `data: unknown`.
- **`string && X` is `"" | X`, not `string | X`.** The `&&` falsy slice uses
  `extract_definitely_falsy_type` (only `""` is definitely falsy), matching
  `getDefinitelyFalsyPartOfType`.
- **`(T | undefined) ?? X` / `(T | undefined) || X` is `(T & {}) | X`** for an
  unconstrained `T`. Constrained `T extends string` does *not* get `& {}`.
- **`||` display order**: `t || u` renders `U | NonNullable<T>` because the
  synthesized `T & {}` is the newest type id; `replace_union_origin_for_display`
  records the order.
- **Unary `+`/`-`/`~`/`++`/`--` on `any` yield `number`, not `bigint`** — even
  though `is_bigint_like(any)` is `true`.
- **`+1n` is TS2736** (unary plus on bigint), but `-1n` is valid.
- **`bigint >>> bigint` is a `TypeError`** even though `<<`/`>>`/`&`/`|`/`^`
  accept bigint pairs — `>>>` is JS-undefined for bigint.
- **TS2869/TS2871 are syntactic**: anchored at `skipOuterExpressions(left, All)`;
  `(1 as any) ?? x` errors at `1`, never consulting the static type.
- **`+` with `ERROR` operand acts like `any`** (`string + error = string`), so a
  TS2304 name does not also produce a `+` error; `unknown + x` is `unknown`.
- **Per-operand `any` validity**: `any * obj` still emits TS2362/TS2363 even
  though the kernel returns `Success(NUMBER)`.
- **Boxed `Number ** 2` is TS2362**, checked before boxed→primitive conversion.
- **Equality on object/array literals** is TS2839 ("compares by reference");
  in `.js` files only for `===`/`!==`.
- **Computed key validity**: the `Symbol` interface as a computed key is invalid
  (TS2464) because its `Lazy(DefId)` does not evaluate to a primitive key type,
  but the same shape in a *mapped* constraint is deferred and accepted.

---

## A worked example: `let r: Strategy = v || 'warn'`

Where `Strategy = 'warn' | 'error'` and `v: Strategy | undefined`.

```
get_type_of_binary_expression_with_request(idx, request{ctx: Strategy})
 │  op = "||"
 ├─ logical_operand_is_primitive_literal(left = v)? no  -> normal widening
 ├─ left_type  = get_type_of_node(v)            = Strategy | undefined
 ├─ outer_context = Some(Strategy)
 │   right is a literal, not a context-sensitive form -> right_request = NONE,
 │   BUT logical_operand_is_primitive_literal(right = 'warn') = true
 │     -> preserve_literal_types = true while typing 'warn'
 ├─ right_type = get_type_of_node('warn')       = 'warn'   (literal preserved, #13997)
 ├─ revisit: reduce_literal_index_access_property_types on both (no-op here)
 └─ evaluator.evaluate(Strategy | undefined, 'warn', "||")
      └─ evaluate_logical:
           plain_truthy_left = narrow_by_truthiness(Strategy | undefined) = Strategy
           truthy_left       = non_nullable_type_parameter_result(.., Strategy) = Strategy  (not a type param)
           falsy_left        = narrow_to_falsy(Strategy | undefined)       = undefined
           falsy != never    -> union2_subtype_reduce(Strategy, 'warn')
                                  'warn' <: Strategy  -> drop 'warn' -> Strategy
           display_origin recorded; result = Strategy
```

Result `Strategy`, which is assignable to the annotation — no TS2322. Before
#13997, `'warn'` widened to `string`, the result became `string`, and the
assignment produced a false TS2322. The fix was scoping
`preserve_literal_types` to the *right* primitive-literal operand symmetrically
with the left.

---

## A worked example: `data ?? {}` with `data: unknown`

```
get_type_of_binary_expression_with_request(idx)
 │  op = "??"
 ├─ left_type  = unknown,  right_type = {}  (empty object literal)
 ├─ evaluated_left = evaluate_type_with_env(unknown) = unknown
 ├─ split_nullish_type(unknown) -> non_nullish = Some(unknown)  (split keeps unknown whole)
 ├─ get_syntactic_nullishness(data) = Sometimes  -> no TS2869/TS2871
 └─ nullish_coalescing_result_type(unknown, Some(unknown), {}, right_idx)
      ├─ evaluated_left == UNKNOWN  -> non_nullish := factory().object(vec![])  = {}
      ├─ apply_non_nullable_approximation({} for unknown, {}) -> {}  (not a type param)
      ├─ right_is_empty_object_literal = true
      ├─ right_subtype: {} <: {}  -> true
      └─ non_nullish == right_type ({})  -> return {}
```

Result `{}`. So `Object.entries(data ?? {})` sees `{}`, exactly as `tsc`. The
key insight encoded here is that `getNonNullableType(unknown)` is `{}`, not
`unknown` — and both the solver and checker re-apply it, because the nullish
*split* deliberately keeps `unknown` whole for flow `!= null` narrowing.

---

## Cross-references

- [solver-evaluation](solver-evaluation.md) — `T[K]`, `keyof`, mapped, and
  conditional reduction that property/index access calls into.
- [solver-narrowing](solver-narrowing.md) — `NarrowingContext` truthiness and
  nullishness splitting reused by the logical operators.
- [solver-relations](solver-relations.md) — the subtype kernel that
  `union2_subtype_reduce` queries.
- [checker-flow-and-narrowing](checker-flow-and-narrowing.md) — flow narrowing
  applied to operands (TS2367, loop widening).
- [checker-assignability-gateway](checker-assignability-gateway.md) — the
  TS2322/TS2345 path the contextual `||`/`??`/`in` checks route through.
- [checker-error-reporter-diagnostics](checker-error-reporter-diagnostics.md) —
  `emit_binary_operator_error` and the operator diagnostic codes.
- [solver-caches-objects-contextual-compat](solver-caches-objects-contextual-compat.md)
  — object shapes and contextual-property caches property resolution uses.
