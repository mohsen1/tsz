# Building Class Instance and Constructor Types

This doc fills a gap left by the wave-1 internals set. [Class Checking:
Declarations, Inheritance, Implements, and Members](checker-classes.md)
describes how a `ClassDeclaration` becomes class-specific *diagnostics*
(`TS2415`/`TS2416`/`TS2420`/`TS4112`/`TS2564`/...). It treats the *type* of the
class — the instance shape, the constructor (static-side) shape, and the
`Lazy(DefId)` handle that names them — as a boundary it consumes. This doc goes
inside that boundary: where the instance `ObjectShape` and the constructor
`CallableShape` are physically assembled, in what phase order, under which cycle
and fuel guards, and how the assembled `TypeId` is published into the solver's
`DefinitionStore` and `TypeEnvironment` so that every later `Lazy(DefId)`
resolves back to it.

The two assemblers live in `crates/tsz-checker/src/types/class_type`. They are
checker code (they own AST orchestration, member ordering, source-context-driven
decisions, and diagnostic emission), but the *types they emit* are built only
through the solver's type factory (`self.ctx.types.factory()`), and the result
is registered through the `DefId` query boundary. The checker never hand-rolls a
relation, instantiation, or evaluation kernel here; it calls
`instantiate_type` / `object_shape_for_type` / `callable_shape_for_type` through
`crate::query_boundaries`, and it asks the solver factory to intern every shape.
For the relation/instantiation/evaluation kernels themselves, see
[Solver: Relations](solver-relations.md),
[Solver: Instantiation](solver-instantiation.md), and
[Solver: Evaluation](solver-evaluation.md). For the `DefId` universe and
interning, see [Solver: Types, Interning, and DefId](solver-types-intern-def.md)
and [Checker: Type of Symbol and Symbol Types](checker-type-of-symbol-and-symbol-types.md).

## Owns / Must not own

| Owns | Must not own |
|------|--------------|
| Walking class members in source order, deferring methods/accessors, building the instance `ObjectShape` | Deciding assignability/subtyping between two class types (that is the relation kernel) |
| Constructing the constructor `CallableShape` (static props, construct signatures, `prototype`, `is_abstract`) | Running instantiation: it *calls* `instantiate_type` through the query boundary, it does not re-implement substitution |
| Cycle/fuel/depth guards that break recursive heritage and self-reference | Defining `Lazy(DefId)` resolution semantics (that lives in `TypeEnvironment::get_def` / `resolve_lazy`) |
| Heritage substitution policy: which type args fill which base type params, default/constraint fallback, truncation | Choosing how the relation kernel canonicalizes alias-forwarded `DefId`s |
| Publishing the final/partial shapes into `DefinitionStore` + both `TypeEnvironment`s | Reading rendered printer output as a predicate; constructing raw `TypeKey` |
| The `__private_brand_*` nominal property and the late-bound/index-signature policy | Emit: the `.d.ts`/JS form of the class (that is the emitter) |

## Where it lives

| Path | Role |
|------|------|
| `types/class_type/mod.rs` | Module root; re-exports `can_skip_base_instantiation` |
| `types/class_type/entry.rs` | Public instance entrypoints; `class_instance_type_cache` read/write, in-progress/prescan short-circuit |
| `types/class_type/core.rs` | `get_class_instance_type_inner`: thin orchestrator, guards, drives the phase helpers in order |
| `types/class_type/instance.rs` | `ClassInstanceBuilder` + Phase 0/1/2 + accessor/finalize phases |
| `types/class_type/instance_merge.rs` | Base-member merge, class/interface merge, `class_instance_build_final_type` |
| `types/class_type/prescan.rs` | `inherited_prescan_this_base_type` (heritage surface for the Phase-0 `this`) |
| `types/class_type/heritage_identity.rs` | `record_heritage_extends`, `refresh_constructor_instance_return_if_stale` |
| `types/class_type/helpers.rs` | Guards (`exceeds_class_inheritance_depth_limit`), cache-admissibility, `register_final_class_instance_type`, brand/merge helpers |
| `types/class_type/js_class_properties.rs` | `collect_js_constructor_this_properties`, `quick_prescan_class_members` (JS/checkJs + cycle fallback) |
| `types/class_type/constructor.rs` | `get_class_constructor_type_*`: static-side assembler, cycle window, `DefId` companion registration |
| `types/class_type/constructor_parts/` | `StaticMemberBuildData`, rough partials, `build_partial_static_constructor_type`, `deferred_constructor_companion_lazy`, member aggregates |
| `query_boundaries/class_type` | `object_shape_for_type`, `callable_shape_for_type`, `construct_signatures_for_type` (read shapes back without touching solver internals) |
| `crates/tsz-solver/src/def/core.rs` | `DefinitionStore`, `DefinitionInfo`, `DefKind`, `register_type_to_def`, `register_class_instance_type`, `get_constructor_def`, `set_body` |
| `crates/tsz-solver/src/def/resolver.rs` | `TypeEnvironment`: `insert_class_instance_type`, `insert_def_with_params`, `get_def`, `get_class_instance_type`; `TypeResolver::resolve_lazy` trait |
| `context/def_mapping.rs` | Checker bridge: `get_or_create_def_id`, `create_lazy_type_ref`, `register_class_instance_in_envs`, `register_resolved_type` |
| `context/resolver.rs` | `resolve_lazy_lookup_only` / `resolve_lazy`: the reverse path `Lazy(DefId) -> TypeId` consumers see |

## The two shapes and the handle that names them

A class declaration produces **two** distinct solver types:

```text
class Box<T> { value: T; static of<U>(u: U): Box<U> { ... } }

instance type  (DefKind::Class)             constructor type (DefKind::ClassConstructor)
  ObjectShape {                               CallableShape {
    value: T                                    construct_signatures: [ new <T>(): Box<T> ]
    + __private_brand_*  (if nominal)           properties: { of: <U>(u:U)=>Box<U>, prototype: Box<any> }
  }                                             is_abstract, string/number static index
  named by Lazy(DefId(Box))                   named by Lazy(DefId(Box::constructor companion))
```

`get_type_of_symbol` on a class **symbol** returns the *instance* type;
`typeof Box` and the value-position binding return the *constructor* type
(`types/queries/class.rs`, see `class_member_this_type`: static `this` resolves
through `get_class_constructor_type`, instance `this` through
`get_class_instance_type`). Both are interned `TypeId`s, and both are given a
stable name by registering a `TypeId -> DefId` reverse mapping
(`DefinitionStore::register_type_to_def`) so the `TypeFormatter` prints `Box` /
`typeof Box` instead of expanding the structural shape.

`DefKind` (in `def/core.rs`) tags the difference and drives expansion/nominal
behavior:

| `DefKind` | Expansion | Nominal | Used for |
|-----------|-----------|---------|----------|
| `Class` | lazy expand | yes (brand) | the instance type |
| `ClassConstructor` | displayed as `typeof ClassName` | — | the static side |
| `Interface` | lazy expand | no | merged interface decls |
| `TypeAlias` | always expand | no | (contrast) |

## Instance type: entry, caches, short-circuits

The public surface is `get_class_instance_type` (and the
`_without_module_augmentations` variant) in `entry.rs`. Both funnel through
`get_class_instance_type_with_mode`, which is where the **instance cache** is
consulted before any work begins:

- `ctx.class_instance_type_cache` is a `RefCell<FxHashMap<NodeIndex, TypeId>>`
  keyed by the class declaration node. A hit returns immediately.
- `ctx.class_instance_resolution_set` is an `FxHashSet<SymbolId>` of classes
  whose instance type is *currently being built*. If the class symbol is in this
  set, `in_progress_class_instance_result` (`helpers.rs`) returns the cached
  *partial* value (or `TypeId::ERROR` if no partial exists yet) instead of
  recursing — this is the in-flight short-circuit that lets a member reference
  the class it belongs to.

Only the `apply_module_augmentations == true` path writes the final result back
to `class_instance_type_cache`; the non-augmenting variant computes without
caching so that a one-off "no augmentation" query never poisons the canonical
cache entry.

`get_class_instance_type_inner` (`core.rs`) is a deliberately thin orchestrator.
Its job before any member work is the **guard stack** (every early return must
clean up whatever it inserted):

1. Insert `current_sym` into `class_instance_resolution_set`. If already present,
   return `TypeId::ERROR` (recursion broken, no diagnostic).
2. Insert `sym_id` into a local `visited: FxHashSet<SymbolId>` (same-file cycles)
   and `class_idx` into `visited_nodes: FxHashSet<NodeIndex>` (cross-file cycles
   with `@Filename` fixtures). A re-insert returns `TypeId::ERROR`.
3. `exceeds_class_inheritance_depth_limit(visited_nodes.len())` — depth `> 256`
   returns `TypeId::ERROR` (`helpers.rs`; the constant is intentionally far above
   realistic hierarchies).
4. `self.ctx.consume_fuel()` — global fuel guard against pathological inheritance;
   exhaustion returns `TypeId::ERROR`.

Only after the guards does it `push_type_parameters(&class.type_parameters)`
(JS files fall back to `push_jsdoc_class_template_type_params` for `@template`),
construct the `ClassInstanceBuilder`, and drive the phases.

## ClassInstanceBuilder and the phase pipeline

`ClassInstanceBuilder` (`instance.rs`) is the cross-phase accumulator: the
`properties`/`methods`/`accessors` maps (keyed by interned-name `Atom`), the
`string_index`/`number_index` `IndexSignature`s, `deferred_methods` /
`deferred_accessors`, the collected `class_type_params`, and a packed-`u8`
`ClassInstanceFlags` (bits for `DID_INSERT_INTO_GLOBAL_SET`,
`HAS_NOMINAL_MEMBERS`, `HAS_LATE_BOUND_MEMBERS`, `PUSHED_PRESCAN_THIS`). Phase
helpers extend the builder; the final phase consumes it. The methodical ordering
matters because **member bodies need a `this` type while the `this` type is
still being built** — the pipeline solves that with a sequence of progressively
richer partial `this` types pushed onto `ctx.this_type_stack`.

```text
get_class_instance_type_inner
  └─ class_instance_phase0_prescan_this        push partial `this` from ANNOTATED members
  └─ class_instance_phase1_non_method_members  properties, ctor param-props, index sigs; defer methods/accessors
  └─ class_instance_setup_deferred_enclosing    pop prescan `this`; set ctx.enclosing_class
  └─ class_instance_phase2_deferred_methods     push richer partial `this`; infer method bodies
  └─ class_instance_process_deferred_accessors  push partial `this` w/ methods; infer get/set
  └─ class_instance_finalize_members            accessors+methods -> properties; add private brand
  └─ class_instance_merge_base_members          merge `extends` base (derived wins) | early-return on cycle
  └─ class_instance_merge_interface_decls       class/interface declaration merging
  └─ class_instance_build_final_type            sort by declaration_order; intern ObjectShape; register DefId
```

### Phase 0 — prescan `this`

`class_instance_phase0_prescan_this` does a single pass over members collecting
only *annotated* property/method/accessor types and constructor parameter
properties into a provisional `ObjectShape`. The reason: the type builder runs
*before* `ctx.enclosing_class` is set, so an initializer like `n = this.s` would
otherwise resolve `this` to `any`. This partial type is:

- pushed onto `ctx.this_type_stack`,
- written into `class_instance_type_cache` for `class_idx` (so re-entrant lookups
  see a partial, not `ERROR`),
- and — critically for the reverse handoff — **registered early** under the
  class `DefId` via `register_class_instance_in_envs(def_id, prescan_type)` plus
  `register_resolved_type(sym_id, prescan_type, ...)`. This is what makes a
  `Lazy(DefId(Self))` resolve to *something* during Phase-2 body checking instead
  of `None` (the comment cites `f.x` on `f: Vec2<(a:A)=>B>` failing `TS2349`
  without it). The final registration in the build phase overwrites it.

When a property initializer is exactly `new C(...)` (the class's own name), the
prescan uses `create_lazy_type_ref(current_sym)` so the property is typed by the
class's own deferred reference rather than a structural snapshot. If any prescan
property references `this` and the class `extends`, `inherited_prescan_this_base_type`
(`prescan.rs`) folds the base instance surface in via a factory `intersection`,
so the prescan `this` already includes inherited annotated members.

### Phase 1 — non-method members

`class_instance_phase1_non_method_members` processes everything that is not a
method/accessor body:

- **Properties**: name resolved via `get_property_name_resolved`; an unresolved
  *computed* name sets `HAS_LATE_BOUND_MEMBERS` (drives index-signature fallback
  later). The declared type comes from `effective_class_property_declared_type`;
  otherwise the initializer is typed under a *refreshed* partial `this` (the
  partial is rebuilt from the properties collected so far, intersected with the
  prescan `this`). For initializer typing it sets `preserve_literal_types = true`
  and `use_declared_type_for_identifier = true` (so `D = DEFAULT` inherits the
  annotated `AB`, not a narrowed `'A'`), then widens fresh literals unless the
  property is `readonly` — mirroring tsc's
  `getWidenedLiteralLikeTypeForContextualType`.
- **Constructor parameter properties** (`public`/`private`/`protected`/`readonly`
  on a ctor param): become instance properties; `private`/`protected` set
  `HAS_NOMINAL_MEMBERS`.
- **Index signatures**: validated for key kind via `classify_index_sig_param_type`
  (string/number/symbol/template-literal/`PropertyKey`), emitting `TS1268`
  otherwise; merged into `string_index`/`number_index`.
- **Methods and accessors** are *deferred* (pushed to `deferred_methods` /
  `deferred_accessors`) so their bodies can be inferred under a `this` that
  already carries every property.
- **JS/checkJs**: `collect_js_constructor_this_properties` scans
  `this.x = ...` writes in constructor/method/arrow bodies and treats them as
  property declarations (the same path is used whether the *current* file is JS
  or the class merely *lives* in a JS file).

### Phase 2 — deferred methods

`class_instance_phase2_deferred_methods` builds a still-richer partial `this`:
all collected properties, plus the *inherited* base instance surface (so a
subclass override that does `return this.optionalProperty` sees the inherited
member), plus `any`/annotated-return placeholders for **every** deferred method
and accessor (so methods can reference each other via `this`). It pushes that
partial onto `this_type_stack` and writes it to `class_instance_type_cache`
(node-keyed only — not into `symbol_instance_types`, to avoid permanently
caching a partial in parameter positions and breaking later brand checks).

For each method it builds a `CallSignature` via `call_signature_from_method`
(or a body-skipping minimal signature when `class_constructor_resolution_set`
contains the symbol — the explicit instance/constructor cycle break, with
`return_type = ANY` as the placeholder). The fluent-`this` rewrite is here: if a
method has no return annotation and either the inferred return equals the partial
type **or** `method_body_returns_only_this` is syntactically true, the return is
replaced with `self.ctx.types.this_type()` (polymorphic `ThisType`), so
`c.foo().bar()` chains across a hierarchy. Methods aggregate into
`MethodAggregate` (separating overload signatures from the implementation
signature). `class_instance_process_deferred_accessors` mirrors this for
get/set, pairing them in `AccessorAggregate`.

### Finalize and brand

`class_instance_finalize_members` converts each `AccessorAggregate` into a
property (`read_type` from getter-or-setter, `write_type` from setter-or-getter,
`readonly` when getter-only and no setter; a setter param with no annotation is
the `UNKNOWN` sentinel and is filtered so paired accessors fall back to the
getter type), then converts each `MethodAggregate` into a callable property —
*unless* a field/accessor of the same name already exists (duplicate-member
diagnostics `TS2300`/`TS2393` are handled elsewhere; here the non-method entry
is preserved to avoid cascading `TS2322`). Finally, if `HAS_NOMINAL_MEMBERS`,
it inserts the nominal **brand**: `__private_brand_<sym>` (or
`__private_brand_node_<node>` for anonymous classes), an `UNKNOWN`-typed
readonly property that gives the class its structural-but-nominal identity so a
plain object cannot be assigned to a class with private/protected members.

### Heritage merge

`class_instance_merge_base_members` (`instance_merge.rs`) walks the `extends`
clause. Derived members win (`b.properties.entry(name).or_insert_with(...)`).
The function is dense with cycle defenses because heritage is the main recursion
vector. It returns `Some(early)` (after cleaning up the resolution set) when it
detects a cycle, which short-circuits the whole `_inner` call:

- self-referential `class C extends C` / `class D<T> extends D<T>`: `break`
  (the `TS2506` diagnostic is anchored elsewhere in class-inheritance checking);
- base in `class_instance_resolution_set` (incl. an alias-canonicalized symbol):
  merge the base's cached partial, or `quick_prescan_class_members` of the base
  if none is cached, then `break`;
- base node already in `visited_nodes`: return `Some(TypeId::ANY)` to break the
  cycle (an `ANY` base here intentionally degrades gracefully, matching tsc's
  tolerance of circular base resolution).

Type-argument substitution for a generic base uses
`can_skip_base_instantiation(base_param_count, arg_count)` (`helpers.rs`,
true only when both are zero) to skip work. When instantiation is needed it
builds a `TypeSubstitution::from_args`, fills missing args from each param's
`default`-then-`constraint`-then-`UNKNOWN` (instantiating the fallback through
`instantiate_type_preserving_meta` so a later default can see earlier args),
**truncates** extra args, then calls `instantiate_type` through the query
boundary. The base shape is read back with `object_shape_for_type` and its props
merged. `record_heritage_extends` (`heritage_identity.rs`) registers the
`child_def -> parent_def` `extends` edge into both environments so instanceof
narrowing and variance can use it (`register_class_extends_in_envs`).

### Interface merge and final build

`class_instance_merge_interface_decls` handles `class C {}` + `interface C {}`
declaration merging, including same-file interfaces, module augmentations
(gated by `apply_module_augmentations`), and lib/cross-arena interfaces (e.g. a
user `class TemplateStringsArray {}` merging with the built-in interface). It
lowers the interface declarations through `TypeLowering::with_resolvers` and
merges shape props/indexes (class members still win).

`class_instance_build_final_type` is the finish line:

1. Collect `b.properties.into_values()` into a `Vec` and **sort by
   `declaration_order`** — the `FxHashMap` order is non-deterministic, but
   diagnostics like `TS2739` ("missing the following properties: a, b, c") must
   list properties in source order. `declaration_order` packs the member
   position into the high 16 bits (`class_member_order = (pos+1) << 16`) so
   constructor parameter properties can claim the low bits, and synthesized
   members (order `0`) stay first under a stable sort.
2. Build the `ObjectShape` (with `symbol: current_sym`), marking
   `has_late_bound_members` / `no_module_augmentation_lookup` as needed, and
   intern it via `factory.object_with_index`.
3. Run the final interface-merge and module-augmentation passes.
4. Clean up: remove from `visited` / `visited_nodes` / `class_instance_resolution_set`,
   drop `instance_type` from `class_decl_miss_cache`, record
   `class_instance_type_to_decl[instance_type] = class_idx`.
5. `register_final_class_instance_type` (the reverse handoff, below).
6. `refresh_constructor_instance_return_if_stale` (heritage identity), then
   `pop_type_parameters`.

## Constructor (static-side) type

`get_class_constructor_type` and friends (`constructor_parts/helpers.rs`) wrap
`get_class_constructor_type_with_request_and_mode` (`constructor.rs`). Its cache
is `ctx.class_constructor_type_cache` (also `RefCell<FxHashMap<NodeIndex, TypeId>>`)
and its in-flight set is `ctx.class_constructor_resolution_set`. The constructor
side is more elaborate than the instance side for one reason: **static
initializers can reference the class's own static members and `new C(...)` while
the constructor type is still being built**, so the code publishes a sequence of
*partial constructor types* during the build.

The cycle handling at entry, when `class_constructor_resolution_set` already
holds the symbol, is a careful ladder (re-entry must not cache `any` cross-file):

1. a cached final constructor type wins;
2. else a `symbol_types` entry whose shape is a `Callable` (a partial built
   during static-member processing) is served;
3. else a window-scoped `window_partial_ctor_types` entry — but only when more
   than one constructor window is open (a nested foreign-class window), so a
   ctor-less subclass resolving inherited construct signatures sees real arity
   instead of `any` (false `TS2554`);
4. else **defer through the companion**: `deferred_constructor_companion_lazy`
   returns `Lazy(ctor_def)` where `ctor_def` is the class's `ClassConstructor`
   companion `DefId` (get-or-created and pinned via
   `register_constructor_companion`). The outer non-cyclic computation later
   `set_body`s the real constructor type onto that exact companion, so the cycle
   resolves correctly once the outer call finishes — mirroring the instance
   side's `Lazy(classDef)` deferral (issue #13947).

### Assembling the static shape

`get_class_constructor_type_inner` pushes class type params (JSDoc `@template`
fallback in JS), pre-computes `collect_inherited_static_properties`, then —
*before* any static initializer is typed — publishes a **rough** partial
constructor:

- `rough_self_instance_reference` (`constructor_parts/rough_partial.rs`) is the
  return type of the rough construct signatures: a bare `Lazy(DefId)` for a
  non-generic class, or `Application(Lazy(DefId), [T...])` for a generic one.
  Using the deferred reference (not a structural snapshot) preserves class
  identity, so `new C(...)` inside `C`'s own static initializers relates to an
  annotated `C<U>` return *by identity* rather than failing structurally against
  a partial member list (false `TS2739`/`TS2740`/`TS2345`).
- `early_rough_construct_signatures` derives signatures from the class's own
  constructors, or inherits the base's arity for ctor-less classes, or falls back
  to a default zero-arg construct signature.
- `build_partial_static_constructor_type` (consuming a `StaticMemberBuildData`
  bundle from `constructor_parts/build_data.rs`) interns a `CallableShape` with
  those construct signatures plus `any`-typed placeholders for every
  not-yet-processed static member name (`all_static_member_names`), so
  `Class.laterMember` inside an earlier static initializer resolves to `any`
  instead of false `TS2339`. It also adds a `prototype` property typed as
  `create_lazy_type_ref(current_sym)` (the instance type) when missing.

`publish_partial_ctor_symbol_types` writes the partial into the window map for
both the node symbol and the class-name symbol (`export default class Foo` has a
separate "default" export symbol and a `Foo` name symbol; self-referential
static initializers may use either).

Then static members are processed in order. Each static property initializer is
typed under the partial constructor pushed onto `this_type_stack` (so `this` in
a static initializer is `typeof Class`), with contextual member typing pulled
from the `TypingRequest` when the class expression itself has a contextual type.
Construct signatures come from `call_signature_from_constructor` (overloads:
only the body-less signatures; otherwise the single implementation; otherwise a
default `new (): instance_type`). Heritage `extends` contributes inherited
static properties and inherited construct signatures (remapped through the base
substitution by `remap_inherited_construct_signatures*`). The final
`CallableShape` carries `construct_signatures`, static `properties`,
static index signatures, `symbol: class_symbol`, and `is_abstract` from the
`abstract` modifier. Private/protected constructors and abstract classes are
tracked in `ctx.private_constructor_types` / `ctx.protected_constructor_types` /
`ctx.abstract_constructor_types`. A mixin base typed by a type parameter `T`
intersects the result with `T` (`factory.intersection2(base_tp, ctor_type)`) so
`T & ConstructorType <: T` holds.

The instance type is computed *after* static-member processing so a
self-referential property initializer can observe the partial constructor; the
ordering, plus `had_instance_cache` tracking, lets the function drop a
provisional instance type cached during constructor resolution and recompute it
cleanly once the constructor window closes (`apply_module_augmentations && did_insert
&& !had_instance_cache`).

### Registering constructor identity

When the result is not `ERROR`, the function attaches a `ClassConstructor`
`DefId`: it reuses the binder-pre-populated companion
(`definition_store.get_constructor_def(class_def)`) and `set_body`s the result
onto it, or registers a fresh `DefinitionInfo { kind: DefKind::ClassConstructor,
... }`. Either way it calls `register_type_to_def(result, ctor_def_id)` so the
formatter prints `typeof ClassName`.

## The Lazy(DefId) handoff (forward and reverse)

This is the boundary [Class Checking](checker-classes.md) and the rest of the
checker consume. A class is *named* in type position by `TypeData::Lazy(DefId)`
(or `Application(Lazy(DefId), args)` for generics). The checker stabilizes the
`DefId`; the solver's `TypeEnvironment` resolves `DefId -> TypeId`.

**Forward (registration), from `register_final_class_instance_type`
(`helpers.rs`), only when the symbol carries `symbol_flags::CLASS`:**

```text
register_final_class_instance_type(sym, instance_type, params)
  ├─ def_id = ctx.get_or_create_def_id(sym)                 // stable DefId (context/def_mapping.rs)
  ├─ definition_store.register_type_to_def(instance_type, def_id)   // TypeId -> DefId reverse (naming)
  ├─ register_class_instance_in_envs(def_id, instance_type)         // class_instance_types in BOTH envs
  └─ register_resolved_type(sym, instance_type, params)            // def body + SymbolRef + extends bridge
```

`register_class_instance_in_envs` defers a `DeferredFlowEnvWrite::InsertClassInstance`
into *both* `TypeEnvironment`s (the evaluator's `type_env` and the flow
analyzer's `type_environment`), which calls `TypeEnvironment::insert_class_instance_type`
(`def/resolver.rs`): it fills the per-env `class_instance_types[def] = instance`,
the reverse `instance_type_to_class[instance] = def` (so instanceof narrowing can
recover the class after the type is resolved from `Lazy` to an `Object`), and —
when a shared `DefinitionStore` is attached — `register_class_instance_type` on
the store for cross-file consumers. `register_resolved_type` writes the body
through `insert_def_with_params` (write-through to `DefinitionStore::set_body_with_params`)
and registers the `def <-> symbol` bridge and `set_body` for the formatter.

**Reverse (resolution), what a consumer of `Lazy(DefId)` runs:** the relation
and evaluation kernels call `TypeResolver::resolve_lazy(def_id, ...)`. The
checker's impl (`context/resolver.rs`) is `resolve_lazy_lookup_only` then, on a
miss, `force_def_on_miss`. `resolve_lazy_lookup_only` maps the `DefId` back to a
`SymbolId` (via the `def_symbol_identity` bridge, with a fallback that
reinterprets the raw `DefId.0` as a `SymbolId` for `interner.reference`-minted
ids), checks depth-poisoning (an unconditionally-infinite alias resolves to
`TypeId::ERROR`), and ultimately reads the published body. The plain
`TypeEnvironment::get_def` (`def/resolver.rs`) is the simplest view: local
`def_types`, falling back to `definition_store.get_body(def_id)` so cross-file
delegation results are visible without merge-back. `get_class_instance_type(def)`
serves the `class_instance_types` slot specifically.

The contract the doc-comment states explicitly: **callers must ensure
`get_type_of_symbol()` ran first to populate the cache before `resolve_lazy`** —
which is why Phase 0 registers the prescan instance early. Without that early
publish, a `Lazy(DefId(Self))` accessed during the class's own Phase-2 body
checking would resolve to `None`.

## Worked example

```ts
class Box<T> {
  value: T;
  map<U>(f: (t: T) => U): Box<U> { return new Box(f(this.value)); }
}
```

1. Some consumer needs `Box`'s instance type → `get_class_instance_type(box_idx, ...)`
   (`entry.rs`). Cache miss; not in `class_instance_resolution_set`.
2. `get_class_instance_type_inner` inserts `Box` into the resolution set,
   `visited`, `visited_nodes`; depth/fuel pass; `push_type_parameters` puts `T`
   in scope.
3. **Phase 0**: `value: T` is annotated → prescan `ObjectShape { value: T }`
   pushed onto `this_type_stack` and registered under `Lazy(DefId(Box))` so
   `this.value` resolves while `map`'s body is checked.
4. **Phase 1**: `value` becomes a real `PropertyInfo` (type `T`); `map` is
   pushed to `deferred_methods`.
5. **Phase 2**: a partial `this = { value: T, map: (...) => any }` is pushed;
   `call_signature_from_method` types `map` as `<U>(f: (t:T)=>U) => Box<U>`.
   Inside the body, `new Box(...)` resolves `Box` in value position to the
   *constructor* type (possibly the window partial if `Box` is mid-resolution),
   and `Box<U>` in the return annotation resolves through `Lazy(DefId(Box))` /
   `Application`. The return is not pure `this`, so no `ThisType` rewrite.
6. **Finalize**: `map` becomes a callable property; no nominal members → no
   brand. No heritage/interface merge.
7. **Build**: props sorted by `declaration_order` →
   `ObjectShape { value: T, map: <U>(f:(t:T)=>U)=>Box<U> }`, interned, and
   `register_final_class_instance_type` publishes it under `DefId(Box)` in the
   store and both environments. Resolution set / visited sets cleaned up;
   `T` popped.
8. Later, `let b: Box<string>` resolves `Application(Lazy(DefId(Box)), [string])`:
   `resolve_lazy(DefId(Box))` returns the published instance type, and the
   instantiation kernel (see [Solver: Instantiation](solver-instantiation.md))
   substitutes `T := string` to yield `Box<string>`'s shape.

## Caches and invariants

| Cache / set | Owner | Key | Invalidation / lifetime |
|-------------|-------|-----|-------------------------|
| `class_instance_type_cache` | `ctx` (`RefCell`) | class `NodeIndex` | Holds prescan → partial → final, overwritten as phases progress; written only on the `apply_module_augmentations` path; constructor resolution may `remove` a provisional entry to force a clean recompute |
| `class_constructor_type_cache` | `ctx` (`RefCell`) | class `NodeIndex` | Written only when `constructor_cache_admissible` and not nested in a foreign window; `refresh_constructor_instance_return_if_stale` patches stale construct-sig return types |
| `class_instance_resolution_set` | `ctx` | `SymbolId` | In-flight guard; every early-return path cleans up if `did_insert_into_global_set` |
| `class_constructor_resolution_set` | `ctx` | `SymbolId` | In-flight guard for the static side; consulted by Phase-2 to break instance↔constructor cycles |
| `window_partial_ctor_types` | `ctx` | `SymbolId` | Window-scoped partial ctor; published/`unpublish`ed around the build; value-position fallback only (type position keeps seeing the `Lazy`) |
| `class_instance_type_to_decl` | `ctx` | `TypeId` | Reverse map instance type → class node, set in the build phase |
| `DefinitionStore.type_to_def` | solver store | `TypeId` | Naming; set via `register_type_to_def`; carries an `AtomicU64` `generation` bumped on mutation |
| `TypeEnvironment.class_instance_types` | per-env | `DefId.0` | Set via `insert_class_instance_type`; mirrored to the store; bumps env `generation` |

Invariants:

- **Guard cleanup is exhaustive.** Each early return that inserted into
  `class_instance_resolution_set` removes the symbol again; `ClassInstanceFlags::
  DID_INSERT_INTO_GLOBAL_SET` records ownership so a re-entrant call that found
  the symbol already present does *not* remove it on the way out.
- **Cache admissibility is narrow.** `constructor_cache_admissible` /
  `ctor_result_embeds_inflight_instance` (`helpers.rs`) forbid caching a
  constructor type whose construct-signature return points at the class's still
  in-flight provisional instance shape — caching it would leak a partial
  (missing computed/symbol/heritage members) into every later `new C()` (false
  `TS7053`/`TS2739`/`TS2741`). Results that point at a `Lazy`/final shape stay
  cacheable so heavy self-referential classes are not recomputed per reference.
- **Two registration sites, one identity.** Phase 0 registers the prescan under
  the `DefId`; the build phase overwrites with the final type. The companion
  `ClassConstructor` `DefId` is get-or-created once and reused, so the deferred
  cycle (`Lazy(ctor_def)`) and the completing computation (`set_body(ctor_def)`)
  agree on identity.
- **Member order is `declaration_order`, not hash order.** The final sort is the
  only thing standing between deterministic `TS2739`/`TS2741` property lists and
  flaky diagnostics.

## Edge cases and tsc parity

- **`class C extends C` / `D<T> extends D<T>`**: caught before recursing in
  `class_instance_merge_base_members` (`break`); `TS2506` is anchored at the
  class name by the class-inheritance checker (matching tsc's squiggle), not
  emitted here.
- **Cross-file / forward-reference cycles**: tracked by `visited_nodes`
  (`NodeIndex`), returning `Some(TypeId::ANY)` to break recursion — tsc tolerates
  circular base resolution by degrading to `any` rather than erroring out the
  whole hierarchy.
- **Self-referential static init** (`static instance: Bar<string>` accessed
  while `Bar`'s constructor is resolving): served the window partial / companion
  `Lazy`, never `symbol_types`' instance-side `Lazy` (which would cause false
  `TS2339` on `Bar.instance`).
- **Fluent `this` return**: a body-`return this` method (type-match *or*
  syntactic `method_body_returns_only_this`) is rewritten to polymorphic
  `ThisType`, so `c.foo().bar()` chains across subclasses — the syntactic check
  exists because interning/flow can produce a `TypeId` that does not equal the
  partial even though it denotes the same instance.
- **Mutable vs readonly literal widening**: `name = ""` widens to `string`;
  `readonly tag = "x"` keeps `"x"`; `D = DEFAULT` (typed identifier, not a fresh
  literal) is not widened — mirroring `getWidenedLiteralLikeTypeForContextualType`.
- **Late-bound (unresolved computed) members**: set `HAS_LATE_BOUND_MEMBERS` /
  `has_static_late_bound_members`; the instance shape is marked
  `has_late_bound_members`, and on the static side an implicit string index is
  synthesized to suppress `TS7053` exactly as tsc does.
- **Nominal brand only when needed**: the `__private_brand_*` property is added
  only when `HAS_NOMINAL_MEMBERS` (a `private`/`protected` member or parameter
  property), giving classes structural-but-nominal identity without breaking
  plain-object assignability for purely public classes.
- **`abstract class`**: `is_abstract` flows onto the `CallableShape` and the type
  is recorded in `abstract_constructor_types`, which is how `new` on an abstract
  class becomes `TS2511` downstream.
- **JS/checkJs classes**: no syntax type params (JSDoc `@template`), `this.x = …`
  body writes become declarations, and an empty `@augments`/`@extends` tag
  suppresses structural base merging (`skip_heritage_merge`).

## See also

- [Checker: Type of Symbol and Symbol Types](checker-type-of-symbol-and-symbol-types.md) — how a class symbol resolves to its instance vs constructor type.
- [Class Checking: Declarations, Inheritance, Implements, and Members](checker-classes.md) — the diagnostic consumer of these shapes.
- [Solver: Types, Interning, and DefId](solver-types-intern-def.md) — the `DefId`/`Lazy`/`Application` universe.
- [Solver: Instantiation](solver-instantiation.md) — substitution applied to generic instance/base types.
- [Solver: Relations](solver-relations.md) and [Checker: Assignability Gateway](checker-assignability-gateway.md) — what consumes a class shape for subtyping.
- [Checker: Calls, Signatures, and Generics](checker-calls-signatures-generics.md) — how construct signatures are used at `new C(...)` sites.
- [Checker: Context and State](checker-context-and-state.md) — `this_type_stack`, `enclosing_class`, and the resolution sets.
- [Checker: Declarations and Modules](checker-declarations-modules.md) — module augmentation and class/interface merging context.
- [End-to-End Timeline](end-to-end-timeline.md) — where class type construction sits in the overall pipeline.
