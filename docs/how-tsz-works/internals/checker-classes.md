# Class Checking: Declarations, Inheritance, Implements, and Members

Class checking is the part of the checker that turns a `ClassDeclaration` (or
`ClassExpression`) AST node into the full set of class-specific diagnostics:
inheritance compatibility (`TS2415`/`TS2416`/`TS2417`), `implements` conformance
(`TS2420`/`TS2422`/`TS2720`), abstract-member completeness
(`TS2515`/`TS2654`/`TS2653`/`TS2656`), `override` discipline
(`TS4112`-`TS4123`), accessor/property kind clashes
(`TS2610`/`TS2611`/`TS2423`/`TS2425`/`TS2426`), strict property initialization
(`TS2564`/`TS2565`), and the rich family of `super` and parameter-property
rules. It lives almost entirely in `crates/tsz-checker/src/classes`, driven from
`crates/tsz-checker/src/state/state_checking/class.rs`.

The cardinal rule of this subsystem is the same as for the rest of the checker:
it **orchestrates** and **attaches diagnostics to source locations**, but it
**asks the solver** for every semantic answer. Whether one member type is
assignable to another is never decided here by hand-rolled structural recursion;
it routes through the shared assignability gateway (`relation -> reason ->
diagnostic`). What this subsystem owns is the *class-specific policy*: which
members pair up, which substitution maps a base type parameter to a derived
type argument, which of several overlapping diagnostics tsc actually emits, and
where the squiggle goes.

```text
ClassDeclaration AST
        |
   binder symbols + flow skeleton (no types)
        |
   state_checking/class.rs  ── orchestrates the per-class check sequence
        |
   classes/*  ── class-specific policy (override, accessor, abstract, super, ...)
        |
   query_boundaries/class.rs  ── should_report_member_type_mismatch* gateways
        |
   assignability/* relation outcome  ── solver relation kernel
        |
   error_reporter  ── TS24xx / TS41xx / TS25xx diagnostics with spans
```

## Owns / Must not own

| Owns | Must not own |
| --- | --- |
| Member pairing: which derived member overrides which base member, by name and static-ness. | The structural relation kernel that decides assignability. |
| Composing the extends-clause type-parameter substitution across the whole chain. | Raw `TypeKey` construction or direct interning of solver types. |
| Diagnostic selection among overlapping class errors (e.g. `TS2415` vs `TS2416` for private/private clashes). | Pattern-matching solver internals to re-derive a relation answer. |
| Source spans: pointing `TS2416` at the member name, `TS2415` at the class name, `TS4115` at the parameter declaration. | Reading printer/formatter output as a predicate to drive control flow. |
| `override`/`noImplicitOverride`, abstract completeness, accessor pairing, `super` ordering, parameter-property desugaring policy. | Building the class instance/constructor type shape (that is the lowering/solver side, consumed here). |

## Module map

| Path | Role |
| --- | --- |
| `state/state_checking/class.rs` | Top-level orchestration: sets `enclosing_class`, manages the instance/constructor type caches, and calls every class check in tsc-compatible order. |
| `classes/class_checker.rs` | `check_property_inheritance_compatibility` — the `extends` member-by-member loop (`TS2415`/`TS2416`/`TS2417`/`TS2610`/`TS2611`/`TS2423`-`TS2426`, `override`). |
| `classes/class_member_info.rs` | `extract_class_member_info` and the `override`/dynamic-name helpers; builds one `ClassMemberInfo` per member node. |
| `classes/class_summary.rs` | `ClassChainSummary` / `ClassInitializationSummary` construction, the chain-summary cache, accessor-pair canonicalization, and strict-init field collection. |
| `classes/class_implements_checker/core.rs` | `check_implements_clauses` and `check_abstract_member_implementations`. |
| `classes/class_implements_helpers.rs` | Inherited-member collection and interface member shaping used by the implements path. |
| `classes/class_abstract_checker.rs` | Type-level (expression heritage / mixin) abstract-member completeness fallback. |
| `classes/constructor_checker.rs` | Constructor accessibility, abstract/private constructor sets, mixin constraints, instance-type classification. |
| `classes/super_checker.rs` | `super` call/property validation and `super`-before-`this` ordering. |
| `classes/private_checker.rs` | Private/protected nominal "brand" extraction and mismatch wording. |
| `classes/class_checker_compat*.rs`, `class_index_signature_compat.rs` | Visibility-conflict detection, index-signature compatibility (`TS2415`), overload compat. |
| `classes/interface_heritage_*` | Interface `extends` compatibility (`TS2430`) and heritage display naming. |
| `query_boundaries/class.rs` | The class-side assignability gateways and `build_own_member_summary`. |

Cross-links: the relation kernels these gateways drive are documented in
[solver-relations](solver-relations.md); the substitution machinery in
[solver-instantiation](solver-instantiation.md); the shared assignability
routing contract in [checker-assignability-gateway](checker-assignability-gateway.md);
the constructor/call/generic side in
[checker-calls-signatures-generics](checker-calls-signatures-generics.md);
accessors/enums in
[checker-jsx-properties-accessors-enums](checker-jsx-properties-accessors-enums.md);
and the per-file caches and `enclosing_class` state in
[checker-context-and-state](checker-context-and-state.md).

## Orchestration order

`state/state_checking/class.rs` runs the checks for one class in a fixed
sequence, because tsc's diagnostic precedence is order-sensitive (a `TS2415`
class-level error suppresses some member-level `TS2416`s, and so on). The
sequence, abbreviated from the orchestration body:

```text
set enclosing_class (push outer onto enclosing_class_chain)
clear/temporarily-restore class_instance_type_cache + class_constructor_type_cache
for each member: check_class_member(member_idx)
check_duplicate_class_members            (TS2300/TS2393)
check_class_member_modifier_disagreements(TS2687)
check_class_member_implementations       (TS2389/2390/2391)   [non-declare]
check_static_instance_overload_consistency(TS2387/2388)        [non-declare]
check_abstract_overload_consistency      (TS2512)
check_abstract_method_consecutive_declarations (TS2516)
check_accessor_abstract_consistency      (TS2676)
check_accessor_type_compatibility        (TS2322 getter/setter)
check_property_initialization            (TS2564/2565)
classExtendsNull2 static-side check      (TS2417)
check_base_constructor_return_type       (TS2509)
check_property_inheritance_compatibility (TS2415/2416/2417/2610/2611/...)
check_mixin_abstract_construct_constraint(TS2797)             [non-abstract]
check_mixin_constructor_rest_parameter   (TS2545)
check_abstract_member_implementations    (TS2515/2653/2654/2656)
check_implements_clauses                 (TS2420/2422/2720/...)
check_jsdoc_implements_clauses / check_jsdoc_extends_name_mismatch  (JS only)
check_index_signature_compatibility      (TS2411)
check_class_declaration
```

Before the member loop runs, the orchestrator builds `EnclosingClassInfo` and
pushes it into `self.ctx.enclosing_class`, recording the class node index,
member nodes, declare-ness, and the class's type-parameter names. It also
deliberately **clears** `class_instance_type_cache` and
`class_constructor_type_cache` for this class so member bodies observe the
checked class shape rather than a provisional snapshot taken during the earlier
environment-building pass — but it *temporarily restores* the constructor type
during member checking to break a cycle when a generic class has a private
static member whose type references itself (e.g. `private static instance:
Bar<string>`). `prior_instance_type_snapshot` is preserved into
`cached_instance_this_type` so a re-entrant `this` lookup from inside an
in-progress arrow-property initializer does not trigger a recursive rebuild that
would return `TypeId::ERROR`.

## ClassMemberInfo: the unit of comparison

`extract_class_member_info` (`classes/class_member_info.rs`) is the funnel that
turns a single `PROPERTY_DECLARATION`, `METHOD_DECLARATION`, `GET_ACCESSOR`, or
`SET_ACCESSOR` node into a `ClassMemberInfo`. This struct is the currency of the
whole subsystem:

```rust
pub(crate) struct ClassMemberInfo {
    name: String,
    type_id: TypeId,
    name_idx: NodeIndex,
    visibility: MemberVisibility,   // Public | Protected | Private
    is_method, is_static, is_accessor, is_setter, is_abstract: bool,
    has_override, is_jsdoc_override: bool,
    has_dynamic_name, has_computed_non_literal_name, from_interface: bool,
}
```

Several subtleties live in this extraction, each chosen for tsc parity:

- **Property type selection.** A property uses its declared annotation if
  present; otherwise the *cached* initializer type, then widened with
  `widen_literal_type` unless the member is `readonly` (readonly properties keep
  their literal type). Crucially, if no cached initializer type exists and we
  are *outside* the member-checking context (`enclosing_class.is_none()`), the
  type falls back to `TypeId::ANY` rather than calling `get_type_of_node`,
  because the `this_type_stack` may hold the constructor type and a `this.prop`
  reference in an instance initializer would otherwise resolve against the
  static side and emit a false `TS2551`.
- **Method type.** Methods are lowered to a `FunctionShape` via
  `call_signature_from_method` with `is_method: true`, which the relation kernel
  later uses to honour method-parameter bivariance.
- **Accessor type.** A getter's type is its annotation, else its inferred return
  type (`infer_getter_return_type`); a setter's type is its first parameter's
  annotation, else `ANY`. `is_setter` marks the setter half so that accessor
  pairs are canonicalized (below).
- **Dynamic vs late-bindable names.** `is_computed_name_dynamic` resolves a
  computed name `[expr]` to a type and treats only string/number literals and
  `unique symbol` as non-dynamic (late-bindable). Because tsz's solver does not
  yet infer `unique symbol` for `const x = Symbol()`, there is an explicit
  workaround: an identifier referencing an un-annotated `const` is treated as
  non-dynamic. A dynamic name disqualifies the member from satisfying `override`
  (`TS4127`).

## The extends path: check_property_inheritance_compatibility

This is the largest single function in the subsystem
(`classes/class_checker.rs`). It resolves the base class, builds the
type-parameter substitution, builds the base chain summary, and iterates the
derived members.

### Resolving the base class

The function reads the first `extends` heritage clause and unwraps it through
several shapes: `ExpressionWithTypeArguments` (`Base<T>`), call expressions
(mixin calls), parenthesized class expressions, and bare identifiers / property
accesses (`React.Component`). The target symbol is resolved with
`resolve_heritage_symbol`, then `get_class_declaration_from_symbol` tries to find
an *in-arena* class declaration. Cross-file or computed bases intentionally
return `None` here so the function falls through to a **type-level fallback**:

- `base_instance_type_from_expression` / `base_constructor_type_from_expression`
  ask the solver for the instance and static (constructor) types.
- `check_override_members_against_type` and
  `check_non_public_member_inheritance_conflicts_against_type` then perform the
  same override and visibility checks against a `TypeId` instead of an AST node,
  using `resolve_property_access_with_env` for member lookup.

`base_heritage_sym` is tracked separately: it gates `TS4112` (`override` with no
base) so that a present `extends` clause whose target *is* a class but whose
instance type tsz could not resolve (a cross-file generic base) does **not**
spuriously emit `TS4112`. The rule mirrors tsc: `TS4112` fires only when there
is no class base at all.

### Building the substitution

When an in-arena base class is found, the function:

1. Reads extends-clause type-argument nodes into `type_args` via
   `get_type_from_type_node`.
2. Pads or truncates `type_args` to the base's type-parameter count, filling
   missing slots from each parameter's `default`, then `constraint`, then
   `TypeId::UNKNOWN`. (For the lib `Iterator` special case, the second slot
   defaults to `UNDEFINED` and the third to `UNKNOWN`.)
3. Builds `TypeSubstitution::from_args` mapping base type parameters to the
   supplied arguments.
4. Calls `compose_ancestor_substitutions`, which walks the whole inheritance
   chain and adds mappings for *ancestor* type parameters. Given `X extends
   L<X>` where `L<RT> extends T<RT[RT['a']]>`, the initial map only covers
   `RT -> X`; the ancestor walk instantiates `RT[RT['a']]` under the current
   map and adds `A -> X[X['a']]` for `T`'s parameter `A`. Without this, an
   inherited member whose type depends on `A` would compare against a stale,
   only-partially-substituted base type and emit a false `TS2416`.

The base member types come from `summarize_class_chain` (below); each one is
instantiated through `substitution` (via `query_boundaries::common::instantiate_type`)
before being compared. Instantiation is owned by the solver — the checker only
supplies the map.

### The per-member loop

For each derived member, `lookup` on the `ClassChainSummary` finds the
corresponding base member (with and without private members, depending on the
check). The loop then resolves, in tsc's precedence order:

| Condition (structural) | Diagnostic | Span |
| --- | --- | --- |
| `override` keyword but name is dynamic | `TS4127` | member name |
| `override` keyword, no visible base member | `TS4113`/`TS4117` (`TS4122`/`TS4123` for JSDoc), with a spelling suggestion if found | member name |
| `noImplicitOverride`, base member exists, not `declare`, base not abstract method | `TS4114`/`TS4116` (or JSDoc `TS4119`/`TS4121`) | member name |
| derived/base visibility clash, both `private` with type mismatch | `TS2416` | member name |
| derived/base visibility clash otherwise | `TS2415` (instance) or `TS2417` (static), with a visibility elaboration line | class name |
| non-method instance property vs base accessor | `TS2610` | member name |
| non-method instance accessor vs base property | `TS2611` | member name |
| instance accessor vs base method | `TS2423` | member name |
| instance method vs base property | `TS2425` | member name |
| instance method vs base accessor | `TS2426` (does **not** stop the type check) | member name |
| type mismatch (instance) | `TS2416` | member name |
| type mismatch (static) | `TS2417` | member name |

The visibility-conflict wording is built by `visibility_conflict_elaboration`,
which encodes the exact catalogue tsc uses for each `(derived, base)` visibility
pair — e.g. both-`private` becomes "Types have separate declarations of a
private property", base-`private`/derived-public becomes "Property '…' is
private in type '…' but not in type '…'".

Note the deliberate **non-`continue`** comments: `TS2610`/`TS2611` and `TS2426`
do not short-circuit the loop, because tsc treats the kind mismatch and the type
incompatibility as *independent* diagnostics and emits both.

### Accessor-pair canonicalization

tsc treats a get/set accessor pair as a single property whose type is the
**getter return type**, and runs override-compat once per pair. tsz reproduces
this with `class_accessor_pair_getter_types` (`classes/class_summary.rs`): for
every `(name, is_static)` that has both a `GET_ACCESSOR` and a `SET_ACCESSOR` in
the body, it records the getter's return type. The getter's iteration runs the
compat check against that canonical type; the setter iteration is **skipped**
when the pair exists, so the setter parameter type never independently relates
against the base (which would emit a false `TS2416` whenever the setter
parameter type legitimately differs from the getter return type). The chain
summary applies the same canonicalization when it stores the setter entry.

### Overloaded methods

When a method name has multiple declarations on either side, comparing one AST
declaration in isolation is wrong (the implementation signature is intentionally
wider; individual overloads are narrower). `check_overloaded_method_compat`
swaps the per-node signature for the externally-visible **`CallableShape`** —
the bodyless overload signatures if present, else the single implementation
signature — and runs the compat check **once per name** (tracked in
`overload_compat_checked`). The combined types come from
`build_class_method_overload_types` (derived) and
`ClassChainSummary::method_overload_type` (base). Bodyless overload signatures
are otherwise skipped from the per-node type compat (`is_overload_signature`),
because tsc checks inheritance against the combined symbol type.

### Parameter properties

A `constructor(public p: T)` parameter property is sugar for a class field but
lives inside the constructor node, so it is checked separately by
`check_constructor_parameter_property_overrides` (override discipline, `TS2610`
accessor clash, `TS4115`) and `check_parameter_property_compatibility` (type and
visibility, reported as `TS2415` at the class name). Optional parameter
properties (`p?: T`) become `T | undefined` under `strictNullChecks` before the
comparison. `check_property_initialization` also notes that a parameter property
does **not** count as initializing a *separately declared* field of the same
name — tsc emits both `TS2300` (duplicate) and `TS2564` (uninitialized) there.

## ClassChainSummary and its cache

`summarize_class_chain` (`classes/class_summary.rs`) walks the inheritance chain
from a class upward, collecting each class's own members
(`collect_class_members_for_chain`) and merging them with a `first-wins`
(`entry().or_insert`) policy so a derived declaration shadows an inherited one.
Inherited member types are rewritten from ancestor type parameters into the root
class's parameter space using a `cumulative_substitution` that accumulates as the
walk descends — the same composition idea as `compose_ancestor_substitutions`,
applied while building the summary.

The result is wrapped in an `Rc<ClassChainSummary>` and memoized in
`self.ctx.class_chain_summary_cache` (keyed by `NodeIndex`). The cache is
classified `FileLocalReset` / `FileTypeCache` in
`context/checker_context_lifetimes.toml`, meaning it is reset at the
file-session boundary rather than mutated piecemeal during a check. During
summary construction `preserve_literal_types` is temporarily disabled so that
inferred method return types are widened (e.g. `"base"` to `string`) — otherwise
a literal return would falsely diff against a widened base.

A `ClassChainSummary` stores, per axis (instance/static), a single unified
`name -> MemberEntry` map (replacing what used to be six parallel maps), plus
`instance_method_overloads` / `static_method_overloads` holding the combined
overload callable per method name. `lookup(name, is_static, skip_private)` is
the single accessor the extends path uses; `skip_private` filters on the
entry's `is_visible` flag.

## The implements path: check_implements_clauses

`check_implements_clauses` (`classes/class_implements_checker/core.rs`) iterates
`implements` heritage clauses. Highlights:

- **`TS2422`** ("A class can only implement an object type…") fires when a class
  implements one of its own type parameters — checked structurally by comparing
  the implements target identifier against the collected
  `class_type_param_names`, even when the type parameter would otherwise resolve.
- The class's own members are collected into a `name -> NodeIndex` map (member
  types computed lazily), parameter properties included. Overloaded method names
  are detected and their combined type pulled from the **class instance type**
  via `instance_member_types_by_name`, because the instance-type builder already
  aggregates overloads and hides the implementation signature.
- Inherited **public** members from the base chain
  (`collect_inherited_public_members`) can satisfy an interface requirement;
  inherited **private/protected** members cannot, but
  `collect_inherited_non_public_members` records them so they are not falsely
  reported as missing when the interface extends the same base class.
- If the implements target is a *class* with private/protected members, tsz
  emits the dedicated "Did you mean to extend…?" form
  (`CLASS_INCORRECTLY_IMPLEMENTS_CLASS_DID_YOU_MEAN_TO_EXTEND…`) instead of the
  per-member `TS2420`.
- Interface type parameters are pushed into scope (with a solver-side
  `definition_store` fallback for lib interfaces like `AsyncIterator<T, …>`
  whose AST lives in another arena), then a substitution maps them to the
  supplied implements type arguments. Interface members are shaped by
  `implemented_interface_members`, which rebuilds method signatures from the AST
  (the object-shape property for a method only stores its return type) and
  **combines** overload signatures from merged interface declarations into one
  callable so the N×M `signaturesRelatedTo` rule applies.

Per-member type compatibility ultimately routes through
`should_report_member_type_mismatch` / `should_report_own_member_type_mismatch`,
the same gateways as the extends path.

## Abstract members: completeness vs compatibility

Two distinct rules apply to abstract classes:

- **Completeness** (`check_abstract_member_implementations`,
  `classes/class_implements_checker/core.rs`): a *non-abstract* class that
  extends an abstract base must implement every inherited abstract member.
  Implemented names are gathered by
  `collect_concrete_member_names_for_abstract_impl` (including members
  contributed by declaration-merged interfaces). For a named class the codes are
  `TS2515` (one missing), `TS2654` (2-4 missing, listed), and `TS2655` (5+,
  truncated with "and N more"); the class-expression variants are `TS2653`,
  `TS2656`, and `TS2650` respectively (these literal codes are emitted directly
  in `core.rs`). When the base is an expression/mixin rather than an
  AST class, the check falls back to `check_abstract_members_from_type`
  (`classes/class_abstract_checker.rs`), which walks the solver-resolved instance
  type (handling intersections like `AbstractBase & Mixin`), finds abstract
  members via the class symbol's member table or per-property `parent_id`, and
  produces `TS2515`/`TS2654`/`TS2656`. Ambient (`declare`) classes are exempt.
- **Per-member compatibility** still runs even for abstract classes via the
  implements/extends paths: an abstract class need not implement every member,
  but a member it *does* declare must be type/visibility compatible
  (`TS2416`/`TS2420`). Only the *completeness* diagnostics are gated on
  `is_abstract_class`.

Abstract-constructor identity is tracked as a side set:
`is_abstract_ctor`/`is_private_ctor`/`is_protected_ctor`
(`classes/constructor_checker.rs`) consult `ctx.abstract_constructor_types`,
`ctx.private_constructor_types`, `ctx.protected_constructor_types` — keeping the
abstractness flag in the checker while the solver's call resolution preserves
the raw target requirement through generic inference
(`constructor_abstractness_for_assignability`).

## Super behaviour

`check_super_expression` (`classes/super_checker.rs`) classifies a `super` node
as a call, a `new super(...)`, or a property/element access, then validates:

- `TS2466` — `super` in a computed property name (checked first).
- `TS2335` / `TS2660` / `TS2337` — `super` outside a derived class / outside a
  member / outside a constructor. Context is found by walking parents; arrow
  functions are *transparent* (they preserve the `super` binding) while regular
  functions/function expressions break it (`is_super_in_valid_member_context`,
  `is_super_in_nested_function`).
- `TS2336`/`TS17011` — `super` property access in constructor parameters.
- Super-call ordering. When the class `requires_super_call` *and* has
  super-call-position-sensitive members, tsz enforces:
  - `TS2376` — a `super` call must be the *first statement* (only when a
    pre-super `this`/`super`-property reference exists and it is not the single
    narrow expression-statement case that `TS17009` suppresses).
  - the root-level-statement requirement
    (`A_SUPER_CALL_MUST_BE_A_ROOT_LEVEL_STATEMENT…`).
  Additional `super` calls after the first root-level one are tolerated.

`super()` call **presence** is tracked separately for `TS2377` ("constructors
for derived classes must contain a `super` call"): any `super()` in the
constructor body (not nested) sets
`enclosing_class.has_super_call_in_current_constructor`, which is a looser
condition than the position-sensitive ordering checks.

When the file has structural parse errors, `check_super_expression` *suppresses*
the semantic super diagnostics (tsc does the same to avoid cascading noise) but
still records `super()` presence.

## Private and protected: nominal brands

Private and `#`-prefixed members are *nominal*. `classes/private_checker.rs`
exposes `get_private_brand`, `types_have_same_private_brand`, and
`private_brand_mismatch_error`, which read a synthetic brand property off the
type shape (via solver query helpers `get_private_brand_name` /
`get_private_field_name`). When two types carry different brands but share a
non-public member of the same name and visibility, the wording becomes "Types
have separate declarations of a private/protected property"; otherwise it is
"Property '…' refers to a different member that cannot be accessed…". `#private`
members are skipped entirely from override checking in the extends loop
(`member_name.starts_with('#')`) because they do not participate in the
inheritance hierarchy.

## The assignability gateways

Class member type compatibility never calls the relation kernel directly. It
goes through three gateways in `query_boundaries/class.rs`, each mapping a
class-specific policy onto a relation mode:

| Gateway | Used for | Relation mode |
| --- | --- | --- |
| `should_report_member_type_mismatch` | instance property/method override (extends + implements) | `no_erase_generics_relation_outcome` |
| `should_report_own_member_type_mismatch` | own-member vs type-level base (expression heritage) | `no_erase_generics` + `class_implements_whole_type` retry |
| `should_report_member_type_mismatch_bivariant` | static side, and combined overload sets | bivariant (`should_report_assignability_mismatch_bivariant`) |

The `no_erase_generics` mode (`no_erase_generics_relation_outcome` in
`assignability/assignability_relation.rs`) mirrors tsc's
`compareSignaturesRelated`: a non-generic override like `m(x: string): string`
is **not** assignable to a generic base `m<T extends string>(x: T): T`, so
`TS2416` is correctly produced. The base member's method-local generics stay
universally quantified rather than being dropped to their constraints. The
static side uses a bivariant relation because tsc checks `typeof Derived` vs
`typeof Base` structurally, which without `strictFunctionTypes` is bivariant —
hence `TS2417` ("Class static side … incorrectly extends base class static
side").

`should_report_member_type_mismatch` layers several **suppressions** before
returning `true` (mismatch): `should_suppress_assignability_diagnostic`,
parse-recovery suppression, an acceptable `this`-parameter case, weak-union
skipping, and `is_coinductive_return_type_cycle` — which silences the false
`TS2416` that arises when a recursive class hierarchy
(`interface I { foo(): I }`, `class A implements I { foo(): B }`,
`class B extends A {}`) was resolved while one of the instance types was still
incomplete (0 properties but carrying a class symbol — see
`is_incomplete_class_type`).

Several finely-tuned helpers govern when the *strict* generic relation can be
relaxed: `generic_erasure_fallback_is_safe`,
`callable_return_mentions_own_method_local_generic`, and the construct-signature
recheck predicates. These all key on **signature shape** (which type parameters
appear in which positions), never on identifier text — consistent with the
repo's anti-hardcoding gate.

## Interface heritage (TS2430)

Interface `extends` compatibility is the dual of class extends and lives in
`classes/interface_heritage_*`. The strict `no_erase_generics` relation governs
member overrides between an interface and its base (so `m(): string` overriding
`m<T>(): T` is rejected when the dropped `T` appears in the return), with a
deliberately relaxed mode when the base method's return does *not* reference its
own dropped generic (a self-returning method is an ordinary covariant position).
This is the same family of decisions documented for the class side, sharing the
gateway in `query_boundaries/class.rs`.

## Strict property initialization

`check_property_initialization` (`state/state_checking/class.rs`) gates on
`strictPropertyInitialization && strictNullChecks`, skips ambient/`.d.ts`
classes, and **bails when the file has structural parse errors** (matching tsc's
`containsParseError` propagation to the source-file node). It consults a
`ClassInitializationSummary` (`classes/class_summary.rs`):

- `required_instance_fields` are property declarations that need a value;
  `property_needs_strict_check` excludes initialized, optional (`?`),
  definite-assignment (`!`), static, abstract, `declare`, and
  string/number-literal-named properties.
- `constructor_assigned_fields` (from
  `query_boundaries::definite_assignment::constructor_assigned_properties`) is
  the set of fields the constructor definitely assigns; a field in this set is
  satisfied.
- A field that is neither initialized nor constructor-assigned yields `TS2564`
  ("Property '…' has no initializer and is not definitely assigned in the
  constructor"), anchored at the field name.
- `ts2565_field_keys` then drive `check_constructor_property_use_before_assignment`
  for `TS2565` ("Property '…' is used before being assigned").

## Caches and invariants

| Cache / state | Owner | Keyed by | Lifetime / invalidation |
| --- | --- | --- | --- |
| `class_chain_summary_cache` | `ctx` (`RefCell<FxHashMap>`) | class `NodeIndex` | `FileLocalReset`; reset at file-session boundary, not mutated mid-check. |
| `class_instance_type_cache` | `ctx` | class `NodeIndex` | Cleared (or kept stable when `class_shape_cache_is_stable`) before member checking; refreshed after. Snapshot preserved into `cached_instance_this_type` for re-entrant `this`. |
| `class_constructor_type_cache` | `ctx` | class `NodeIndex` | Cleared before member checking but temporarily restored during it to break self-referential static-member cycles. |
| `abstract/private/protected_constructor_types` | `ctx` | `TypeId` | Side sets recording constructor abstractness/visibility. |
| `enclosing_class` + `enclosing_class_chain` | `ctx` | — | Push/pop around each class; the chain lets protected-access checks walk the hierarchy. |
| `overload_compat_checked`, `accessor_mismatch_reported`, `class_extends_error_reported` | local to the extends loop | name / flag | Dedupe one diagnostic per name / per class within a single pass. |

Invariants worth preserving when editing:

- Member types are always **instantiated through the chain substitution** before
  comparison; comparing a raw base type-parameter type is a parity bug.
- The accessor pair is **one property**: run override-compat on the getter's
  iteration, skip the setter when both exist.
- `TS2415`/`TS2417` are class-level (anchor at the class name) and fire at most
  once per class (`class_extends_error_reported`); `TS2416` is member-level.
- `override` policy is suppressed for ambient/`declare` classes, for `declare`
  property re-declarations, and for a concrete member implementing an abstract
  base method (only abstract-to-abstract re-declarations require the keyword).

## Edge cases and tsc parity

- **`override` with no base member, did-you-mean.** Spelling suggestions route
  through the single canonical weighted scorer (`best_spelling_suggestion`), and
  tsc does **not** suggest for names of length ≤ 3 — preserved at the
  `find_override_name_suggestion` call site.
- **Computed-but-literal names.** `[someVar]` where `const someVar = "foo"` is
  late-bindable and behaves like a normal property; `[expr]` with a widened or
  symbol type is dynamic and skips `noImplicitOverride` (`isComputedNonLiteralName`
  parity).
- **Private/private clash with type mismatch.** When both members are `private`,
  tsc prefers `TS2416` (type incompatibility) over `TS2415` (branding) if the
  types actually differ; tsz reproduces this branch explicitly before falling
  back to `TS2415`.
- **Namespace-merged static side (`TS2417`).** When a class merges with a
  `namespace`, `typeof Class` gains the namespace exports. tsz only runs the
  whole-type static-side `TS2417` check when the *derived* class has a merged
  namespace **and** its exported names overlap the base static side
  (`has_name_overlap`), avoiding a false positive on self-referential clodule
  generics where the constructor relation rejects a technically-compatible pair
  purely because of the self-reference.
- **`class extends null`.** A valid construct; `super()` inside it is `TS17005`/
  the dedicated "constructor cannot contain a super call when its class extends
  null" diagnostic, and merging with a heritage-bearing interface forces a
  static-side `TS2417` against `'null'` (`classExtendsNull2`).
- **`TS2509` base constructor return type.** `check_base_constructor_return_type`
  asks the solver for the base instance type and is permissive for
  `any`/`error`/`null`.
- **Lib `Iterator` heritage.** Extending the lib `Iterator` is special-cased in
  display and default type arguments (`heritage_reference_is_actual_lib_iterator`,
  the `UNDEFINED`/`UNKNOWN` defaults for the 2nd/3rd parameters) so the rendered
  base name reads `Iterator<number, undefined, unknown>` — but the check is gated
  on the symbol genuinely resolving to the cloned/actual lib `Iterator`, not on
  the bare name, so a user-defined `Iterator` shadow is unaffected.
- **Empty `@extends`/`@augments` JSDoc.** In checked JS, an empty augments tag
  deliberately invalidates the structural `extends` edge for instance members so
  recovery lookups do not resurrect suppressed base properties
  (`skip_heritage_merge`).

## A worked example

```ts
class Base<T> {
  value: T;
  get tag(): string { return "base"; }
}
class Derived extends Base<number> {
  value: string;            // TS2416
  override missing() {}     // TS4113
}
```

Tracing `Derived`:

1. `state_checking/class.rs` sets `enclosing_class`, clears the type caches, and
   eventually calls `check_property_inheritance_compatibility(Derived)`.
2. The extends clause resolves `Base` to its in-arena class declaration;
   `type_args = [number]`; `TypeSubstitution::from_args` maps `T -> number`;
   `compose_ancestor_substitutions` adds nothing (no deeper chain).
3. `summarize_class_chain(Base)` returns a cached `ClassChainSummary` with
   instance members `value: T` and the accessor `tag: string`.
4. For `value: string`, `lookup("value", false, true)` finds the base entry;
   `instantiate_type(T, {T->number})` yields `number`. Not a kind mismatch, not
   `any`. `should_report_member_type_mismatch(self, string, number, name_idx)`
   runs `no_erase_generics_relation_outcome(string, number)`, which is **not**
   related, so the gateway returns `true`.
5. The loop emits `TS2416` at `value`'s name, then
   `report_type_override_incompatibility_detail` appends the elaboration.
6. For `override missing()`, `has_override` is set but
   `lookup("missing", …)` is `None`; with no close spelling suggestion the loop
   emits `TS4113` ("… cannot have an 'override' modifier because it is not
   declared in the base class 'Base<number>'") at the method name.

Every semantic decision in that trace — instantiation, the assignability
relation, property collection — was made by the solver or a solver-backed query
boundary; the class checker chose the pairing, the substitution, the diagnostic
code, and the span.
