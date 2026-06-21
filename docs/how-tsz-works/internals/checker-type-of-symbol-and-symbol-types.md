# Computing the Type of a Symbol: get_type_of_symbol and the type_resolution Engine

## Orientation

Every semantic question the checker asks eventually bottoms out in one
operation: *what is the type of this symbol?* A variable annotation, a class
heritage clause, an `import("./m").Foo`, a `keyof T`, an enum member, a
`typeof X` — all of them reduce to turning a `SymbolId(u32)` into a
`TypeId(u32)`. This document is about that kernel: `get_type_of_symbol` /
`compute_type_of_symbol` in `state/type_analysis`, the `state/type_resolution`
module that orchestrates *type-position* resolution around it, and the
constructor / heritage / cross-file / `import` / computed-property publication
machinery that surrounds it.

[checker-context-and-state](checker-context-and-state.md) names this function at
its call site — `build_type_environment` "drives each [symbol] through
`get_type_of_symbol -> compute_type_of_symbol -> register_def_in_envs`" — and
lists the caches (`symbol_types`, `type_resolution_fuel`,
`MAX_SYMBOL_RESOLUTION_DEPTH`) by name. This doc goes *inside* that arrow: the
re-entrancy placeholders, the circular-reference protocol, the cross-arena
delegation epoch, the dual-environment registration, the type-reference
dispatch in `type_resolution/core.rs`, and the concrete `tsc`-parity edge cases
each guard exists to satisfy. It deliberately does **not** re-derive how a class
*shape* is built ([checker-class-shape-construction](checker-class-shape-construction.md)),
how a `Lazy(DefId)` is later *evaluated*
([solver-evaluation](solver-evaluation.md)), or how the
`DefId`/`TypeEnvironment` universe is laid out
([solver-types-intern-def](solver-types-intern-def.md)) — it links to those.

The reader should already know the canonical handles: `TypeId`, `SymbolId`,
`DefId`, `Atom`, and that semantic references are `TypeData::Lazy(DefId)` where
the checker *stabilizes* a `DefId` and the `TypeEnvironment` *resolves*
`DefId -> TypeId`.

## Owns / Must not own

| Owns (this kernel) | Must not own (delegated) |
| --- | --- |
| Symbol -> `TypeId` resolution, caching, re-entrancy and cycle protocol | Building the class instance shape ([checker-class-shape-construction](checker-class-shape-construction.md)) |
| `DefId` minting (`get_or_create_def_id`) and dual-env publication | Relation/subtype kernels, variance ([solver-relations](solver-relations.md)) |
| Type-position dispatch (`get_type_from_type_reference`) | Evaluating `Application`/`Lazy`/conditional bodies ([solver-evaluation](solver-evaluation.md)) |
| Cross-file/cross-arena delegation orchestration | Interning `TypeData`, `DefinitionStore` storage ([solver-types-intern-def](solver-types-intern-def.md)) |
| Constructor/heritage *publication* into envs and `DefinitionStore` | Instantiating generics ([solver-instantiation](solver-instantiation.md)) |
| Computed-property-name pre-resolution for lowering | Lowering AST type nodes to `TypeData` (`tsz_lowering::TypeLowering`) |
| Fuel/depth/stack guards on resolution | Inference of un-annotated initializers ([solver-inference](solver-inference.md)) |

The hard architecture rule holds: this kernel *asks* the solver for semantic
answers (instantiation, evaluation, relations) through the
`query_boundaries` gateway and `TypeLowering`; it does not run those kernels
itself, does not construct raw `TypeKey`, and does not read printer output as a
predicate.

## Module map

`state/type_resolution/` (16 KLOC across 30 files) is the *type-position*
orchestration layer; the value-position / dispatch entry points live in
`state/type_analysis/`. The split matters: `type_resolution` answers "what does
this name *mean as a type*", `type_analysis::get_type_of_symbol` answers "what
is the symbol's canonical type" regardless of position.

| File | Role |
| --- | --- |
| `state/type_analysis/core.rs` (`get_type_of_symbol`) | The cache + re-entrancy + dual-env publication kernel |
| `state/type_analysis/computed/mod.rs` (`compute_type_of_symbol`) | Per-symbol-kind dispatch: alias, enum, namespace, accessor, method, function, class |
| `state/type_analysis/computed_helpers_binding.rs` (`compute_class_symbol_type`) | Class -> constructor type + instance type publication |
| `type_resolution/core.rs` | `get_type_from_type_reference`: the type-node dispatch (bare/qualified/import/array/generic) |
| `type_resolution/symbol_types.rs` | `type_reference_symbol_type[_with_params]`: type-position symbol meaning |
| `type_resolution/symbol_types_depth.rs` | `TypeReferenceResolutionDepthGuard` (alias-forwarding cap, 350) |
| `type_resolution/symbol_types_lazy.rs` | `resolve_symbol_as_lazy_type[_named]`: materialize body then return `Lazy(DefId)` |
| `type_resolution/constructors.rs` (+ `constructors/`) | Applying type args to constructor types; base-instance merge |
| `type_resolution/cross_file_constructors.rs` | Cross-arena JS constructor-function instance synthesis |
| `type_resolution/cross_file_export.rs` | Mode-aware cross-file export lookup kernel |
| `type_resolution/heritage_publication.rs` | Publish/consume heritage-merged interface bodies across files |
| `type_resolution/computed_property_names.rs` | Pre-resolve `[k]` computed keys before lowering |
| `type_resolution/import_type.rs` | `check_import_type_and_resolve`: `import("./m").Foo` |
| `type_resolution/array_heritage.rs` | `extends Array<T>` base-instance specialization |
| `type_resolution/module.rs`, `module/` | Re-exports, namespace exports, CJS/ESM interop |
| `context/def_mapping.rs` | `get_or_create_def_id`, `register_def_*_in_envs` (the dual-env writers) |

## Two entry points, two questions

There are two distinct top-level questions and they take different paths.

```
                  value position                 type position
            (let x = C; typeof C)           (let a: C; extends C; keyof C)
                     │                                 │
                     ▼                                 ▼
          get_type_of_symbol(sym)          type_reference_symbol_type(sym)
       (type_analysis/core.rs)             (type_resolution/symbol_types.rs)
                     │                                 │
                     │   ┌─────────────────────────────┤ class? -> instance type
                     │   │                              │ interface? -> merge decls -> Lazy
                     ▼   ▼                              │ alias? -> body (evaluated)
          compute_type_of_symbol  ◄────────────────────┤ alias-symbol? -> resolve target
       (type_analysis/computed/mod.rs)                 │
            per-kind dispatch                          └─ falls back to get_type_of_symbol
```

- `get_type_of_symbol(sym_id)` is the *value-meaning* canonical answer: a class
  resolves to its **constructor** type (`typeof C`), an enum to
  `TypeData::Enum(def, members)`, a function to its callable/overload set.
- `type_reference_symbol_type(sym_id)` is the *type-meaning* answer used when a
  name appears in type position: a class resolves to its **instance** type, an
  interface to a `Lazy(DefId)` over the merged structural body, a type alias to
  its (often eagerly evaluated) body. It calls `get_type_of_symbol` only as a
  fallback for kinds that have no separate type-meaning.

Both are reached from `get_type_from_type_reference` in `type_resolution/core.rs`,
which is the dispatcher for an actual *type node* (a `TypeReference` AST node);
that function resolves the name to a `SymbolId`, decides the spelling shape
(bare / qualified / import-type / array / generic application), and then calls
into the two functions above.

## `get_type_of_symbol`: the cache + re-entrancy kernel

`get_type_of_symbol` (`type_analysis/core.rs`, line 1042) is a thin wrapper that
installs the stack guards and then calls `get_type_of_symbol_inner`. The wrapper
matters for deep recursion:

- A **hard stack breaker**: `crate::checkers_domain::stack_overflow_tripped()`.
  Once a previous deep recursion tripped the breaker, every subsequent call
  caches `TypeId::ERROR` for `sym_id` and returns immediately.
- A **stack probe**: `should_probe_stack()` + `headroom_below(1024 * 1024)` trips
  the breaker proactively when under 1 MiB of stack remains.
- **`stacker::maybe_grow(256 KiB, 2 MiB, …)`**: dynamically grows the native
  stack for legitimately deep symbol chains before recursing into the inner
  function.

`get_type_of_symbol_inner` then runs a fixed sequence. The ordering is
load-bearing — each step exists to make a specific cycle or cross-file case
behave like `tsc`.

### Step 1 — record dependency, decide ownership

`record_symbol_dependency(sym_id)` feeds incremental tracking (see
[driver-incremental-and-watch](driver-incremental-and-watch.md)). Then it
decides whether to use *local* symbol state or treat the symbol as owned by a
foreign file:

```
cross_file_owner_idx = resolve_symbol_file_index(sym)
    .filter(|f| f != current_file_idx)
    .filter(|f| !value_variable_owned_by_current_file_not_foreign(sym, f))
use_local_symbol_state = cross_file_owner_idx.is_none()
```

The second filter is a real `tsc`-parity guard: a plain `export const` genuinely
declared in the current file must resolve locally even when an `export *` cycle
(`internal.ts` does `export * from "./common"`, `common.ts` imports back) makes
the cross-file overlay claim a foreign owner. Delegating to the re-exporting
file — which has no concrete declaration — would collapse the const to `any`
(false `TS7053` on `obj[K]`, masked real `TS2322`).

### Step 2 — cache lookup with stale-placeholder eviction

For a foreign-owned symbol, `cached_cross_file_symbol_type(sym, file_idx)` is the
bucket (gated for declaration-file classes on `class_instance_recoverable`,
#13185). For a locally-owned symbol, `ctx.symbol_types.get(&sym_id)` is checked,
but with one subtlety: a cached `Lazy(DefId)` that points at *this* symbol's own
def and is a `TYPE_ALIAS`, while the symbol is **not** in the active resolution
set, is a *stale alias placeholder* left behind by an interrupted resolution.
It is removed and recomputed rather than returned. A genuine cache hit records
`record_compute_type_of_symbol_cache_hit()` and returns.

The cached-`ERROR`-while-resolving branch is the function's most intricate
re-entrancy handler: if the cache holds `ERROR` *and* the symbol is currently
being resolved (`symbol_resolution_set.contains`), it pre-caches `ANY` as a
sentinel and tries `provisional_circular_function_symbol_type`. This breaks the
`typeof foo<T>` appearing in `foo`'s own return type — without the `ANY`
sentinel the re-entrant call would find `ERROR`, re-detect circularity, and
recurse into `provisional` again until the stack overflows.

### Step 3 — fuel and depth guards

```
if !ctx.consume_fuel() { cache ERROR (if local); return ERROR }
```

`consume_fuel` (`context/core.rs`) decrements the per-file
`type_resolution_fuel` (`Cell<u32>`, reset to `MAX_TYPE_RESOLUTION_OPS` =
`100_000` in release, `20_000` in debug) *and* a thread-local global counter
that survives cross-arena child contexts — so delegation cannot defeat the
budget by minting fresh per-context fuel. Then a depth gate:

```
depth = symbol_resolution_depth.get()
if depth >= max_symbol_resolution_depth (= MAX_SYMBOL_RESOLUTION_DEPTH = 50) {
    cache ERROR (if local); return ERROR
}
```

`MAX_SYMBOL_RESOLUTION_DEPTH = 50` (`context/mod.rs`) is intentionally aligned
with the solver's instantiation depth and the `CheckerRecursion` profile
(`tsz_solver::recursion::RecursionProfile::CheckerRecursion`, `max_depth = 50`).

### Step 4 — circular-reference protocol

If the symbol is already in `symbol_resolution_set`, we are in a cycle. The
response is *kind-dependent*, which is the heart of how tsz tolerates legal
recursive types:

| Symbol kind in a cycle | Returned (not cached) |
| --- | --- |
| `INTERFACE` / `CLASS` / `TYPE_ALIAS` / `ENUM` / `NAMESPACE_MODULE` / `VALUE_MODULE` | `factory.lazy(def_id)` — defers evaluation |
| `CLASS` with a partial constructor available | `circular_class_partial_constructor_type(sym)` |
| `FUNCTION` (not interface) | `provisional_circular_function_symbol_type(sym)` (cached) |
| anything else | `ERROR` (cached, to stop deep recursion) |

The `Lazy` is deliberately *not* cached: when the cycle later breaks, the next
lookup must recompute the real body. This is what lets
`interface User { filtered: Filtered }` /
`type Filtered = { [K in keyof User]: ... }` resolve — `keyof Lazy(User)` defers
instead of failing.

### Step 5 — placeholder seeding, then compute

Before computing, the same kind logic seeds a **placeholder** into
`symbol_types` (named entities get `Lazy(def_id)`, functions get the provisional
type, others get `ERROR`). Re-entry mid-resolution hits the cache and returns the
placeholder immediately instead of recursing deeper — the actual mechanism that
prevents stack overflow on circular class inheritance.

Then it captures the **cross-arena bailout epoch**, pushes the dependency, and
calls the kind dispatcher:

```
bailout_epoch_before = Self::cross_arena_bailout_epoch()
push_symbol_dependency(sym, true)
(result, type_params) = self.compute_type_of_symbol(sym)
pop_symbol_dependency()
result_is_bailout_artifact =
    cross_arena_bailout_epoch() != bailout_epoch_before && result == ANY
```

The epoch is a global counter bumped whenever a cross-arena delegation is
*refused by the depth cap*. If a provisional `any` was minted because of such a
refusal during this resolution, it is a *registration-window artifact*, not the
symbol's real type (the immer `[WRITABLE]` computed-key poison, #13846). Such an
`any` is dropped from the cache so a later shallower pass recomputes the
authoritative type. `ERROR`/`UNKNOWN` are deliberate cross-file cycle markers
and are left untouched.

### Step 6 — module-augmentation fold

Before caching, an exported, non-imported, *concrete* interface body has
cross-file `declare module` augmentations folded in via
`apply_self_module_augmentations` — gated cheaply on
`program_has_module_augmentations()` first, then the `INTERFACE && !CLASS` flag,
then a per-`TypeId` `classify_for_augmentation` shape check, so augmentation-free
programs pay nothing. Doing it *here* (the canonical resolution point) means
every downstream cache — `symbol_types`, both `TypeEnvironment`s, the
`DefinitionStore` — observes the **same** augmented body (#13653, extending the
same-module #13509 fix to the cross-file path).

### Step 7 — caching, with corruption guards

Two artifacts are *dropped* (placeholder removed, not cached) for class symbols:

- `result_is_lazy_to_self`: the result is a `Lazy(DefId)` pointing at the class's
  own def — a constructor-cycle fallback that resolves to the *instance* type.
  Caching it would poison value-position lookups (`C.staticProp` inside an
  instance method -> false `TS2339`).
- `class_instance_resolution_in_flight`: a class queried in value position while
  its own instance type is still being computed yields a provisional constructor
  whose construct-signature return is the Phase-0 prescan instance shape (missing
  computed/symbol-keyed members and heritage). Detected by
  `ctor_result_embeds_inflight_instance`; caching would leak the partial instance
  into later `new C()` (false `TS7053`/`TS2739`/`TS2741`).

Otherwise the result is cached in `symbol_types` (local) or
`cache_cross_file_symbol_type` (foreign), and `cache_resolved_symbol_type_for_owner`
mirrors it for the owning file.

### Step 8 — dual-environment publication

This is the step that makes a resolved symbol *usable by the solver*. Skipped for
`ANY`/`ERROR` and for in-flight class constructors. With `type_env`
(`try_borrow_mut`, deferring on a borrow race rather than panicking), it writes
both a `SymbolRef`-keyed and a `DefId`-keyed entry:

| Symbol kind | `type_env` entries |
| --- | --- |
| class | constructor `result` under `SymbolRef`/`DefId`; **plus** `insert_class_instance_type(def, instance)`; **plus** `register_class_extends(def, parent_def)` for nominal `instanceof` narrowing |
| generic (params non-empty) | `insert_with_params` / `insert_def_with_params` |
| non-generic lib interface (`Promise`, `Array`) | params recovered from `get_def_type_params(def)` even when `compute_type_of_symbol` returned none |
| enum member | `register_enum_parent(member_def, parent_def)` |
| numeric enum | `maybe_register_numeric_enum` (Rule #7, open numeric enums) |

The same `DefId` body is then *mirrored* into the second environment
`type_environment` (the `FlowAnalyzer`'s env) via
`mirror_def_in_type_environment` / `mirror_class_instance_in_type_environment`.
The mirror **defers** on a borrow race (it is replayed at
`flush_deferred_flow_env_writes`) rather than dropping — a dropped mirror left
the two envs holding two *distinctly interned* materializations of a recursive
self-referential interface, a divergence the vacancy-only `overlay_missing_from`
cannot reconcile (#13944). The two-environment split is described in
[checker-context-and-state](checker-context-and-state.md) and
[checker-flow-and-narrowing](checker-flow-and-narrowing.md); this kernel is the
single writer that keeps them coherent.

For `TYPE_ALIAS` symbols only, a `TypeId -> DefId` reverse mapping plus a body
"provenance" record (`mark_body_as_computed` / `mark_body_as_directly_named`) is
registered so diagnostics can display the alias name instead of a structural
expansion (`ExoticAnimal` vs `CatDog | ManBearPig | Platypus`). Interfaces do
not need this — `ObjectShape.symbol` already carries the name; registering them
would mis-paint inline `A | B` types with an alias name.

## `compute_type_of_symbol`: the per-kind dispatch

`compute_type_of_symbol` (`type_analysis/computed/mod.rs`, line 784) returns
`(TypeId, Vec<TypeParamInfo>)`. The returned params **must** be the exact
`TypeId`-bearing params used when lowering the body, or substitution at
instantiation breaks. The function records `record_compute_type_of_symbol_call()`
(a multi-million-call perf counter) and dispatches in a fixed priority order.
The early returns handle imports and interop *before* the structural kinds:

```
module.exports interop alias        → binding value type
cross-arena delegation              → delegate_cross_arena_symbol_resolution(sym)
─ resolve symbol globally / cross-file ─ (else (UNKNOWN, []))
ESM/CJS default-import namespace     → namespace object type
namespace import (import * as X)     → self_namespace_import_object_type
named JS export                      → resolve_js_export_named_type
EXPORT_VALUE wrapper                 → delegate to wrapped declaration symbol
ALIAS → class+namespace merge target → get_type_of_symbol(target) (full flags)
CLASS                                → compute_class_symbol_type
ENUM                                 → TypeData::Enum(def, union-of-members)
NAMESPACE_MODULE / VALUE_MODULE      → compute_namespace_symbol_type (Lazy)
ENUM_MEMBER                          → compute_enum_member_symbol_type
GET/SET_ACCESSOR                     → annotation / inferred body type
METHOD                               → merge overloads across declaration arenas
FUNCTION (not interface)             → callable / overload set
… (variable, property, type literal, …)
```

The dispatch order is itself parity-critical and commented as such in code:

- The `ENUM` branch runs **before** `NAMESPACE_MODULE` because an enum merged
  with a namespace carries both flags; it must be handled as an enum
  (`TypeData::Enum`) not a namespace (`Lazy`).
- The `NAMESPACE_MODULE` branch is skipped when the symbol *also* has
  `FUNCTION`/`VARIABLE`/`TYPE_ALIAS`, so `type Foo = …; namespace Foo { … }`
  resolves to the alias body, not the namespace.

### Enum construction in detail

The enum branch is the one place where the kernel computes literal member types
itself (constant evaluation is described in
[checker-const-enum-and-literal-evaluation](checker-const-enum-and-literal-evaluation.md)).
It walks *all* declarations (merged `const enum` blocks contribute members from
every block), runs the auto-increment counter (start at `0`, reset to
`initializer + 1` after an explicit numeric initializer, `None` after a string),
and synthesizes `factory.literal_number(val)` for un-initialized numeric members
so a mapped type `{ [k in E]?: string }` gets discrete keys `"0"`,`"1"` not a
flat `number`. It pre-caches **each member symbol's** `Enum(member_def, type)`
into `symbol_types` and both envs (with `register_enum_parent`) so `E.Member`
access hits the cache instead of re-resolving per member. It returns
`Enum(def, structural_union)` — `Lazy(def)` would infinitely recurse in
`ensure_refs_resolved`, a bare union would lose nominal identity (`E1` vs `E2`).
It also computes and stores the enum *namespace object* type
(`merge_namespace_exports_into_object`) for `typeof E` / `keyof typeof E`.

## `type_reference_symbol_type`: type-position meaning

`type_reference_symbol_type` (`type_resolution/symbol_types.rs`, line 18) is the
type-position counterpart. It is guarded by `enter_recursion`/`leave_recursion`
(the `CheckerRecursion` `DepthCounter`, `max_depth = 50`) and branches on flags:

| Symbol kind | Type-position result |
| --- | --- |
| `CLASS` (incl. class+interface+namespace merges) | `class_instance_type_with_params_from_symbol` -> register instance in envs -> instance `TypeId` |
| `INTERFACE` | merged structural body, then `Lazy(def)` (or the structural type directly for index-signature / namespace-merged / cross-file-unknown forms) |
| `TYPE_ALIAS` | body, then `evaluate_type_with_env` unless it is a deferred `keyof`, a union/intersection, or contains free type params |
| `ALIAS` (import) | `resolve_alias_symbol` -> recurse on the target's type meaning, applying module augmentations |
| else | falls back to `get_type_of_symbol`, then `import_type_alias_types` override |

The class branch returns the **instance** type and registers it under the def
(`register_class_instance_in_envs`, `class_instance_type_to_decl`) so a later
`{ new(): Foo }` or `typeof`-paired query finds it. The interface branch is the
most defensive: it prefers the cross-arena delegated body when the symbol's home
file differs (cross-file `SymbolId` collisions are real), applies module and
self-module augmentations, caches generic params on the def, and — critically —
*skips* registering a `Lazy(def)` whose target is the def itself (a cycle-breaker
placeholder), because persisting `Lazy(X) -> X` creates a self-loop that yields
`never[]` for empty array literals.

`type_reference_symbol_type_with_params` (same file, line 1148) is the variant
that also returns the params, used where instantiation needs body+params from the
*same* `push_type_parameters` call. It is guarded by
`TypeReferenceResolutionDepthGuard` (`symbol_types_depth.rs`), a thread-local
RAII counter capped at `MAX_TYPE_REFERENCE_RESOLUTION_DEPTH = 350`. That cap
exists for one specific failure: a mutually-aliasing pair
(`Dataset` <-> `OutputDataset`) produced by a raw-`SymbolId` cross-file
collision would ping-pong until the stack overflows; past the cap it returns the
symbol's own `Lazy` reference instead of crashing (#13212). 350 is far above any
legitimate alias chain and far below stack exhaustion.

## `get_type_from_type_reference`: the type-node dispatcher

`get_type_from_type_reference(idx)` (`type_resolution/core.rs`, line 158) is the
entry from the lowering pipeline when an actual `TypeReference` AST node is seen.
It is the busiest function in the module and the routing is:

```
consume_fuel? ───no──▶ ERROR
   │ yes
   ▼
import alias in plain type position?  (import { X }; x: X)
   └─ map constructor→instance, unless target owns a declared type (alias/iface)
type_name is import("…") call / qualified import?  ─▶ check_import_type_and_resolve
type_name is QualifiedName (A.B)?
   ├─ with type args ─▶ resolve member, validate arity (TS2314/TS2344), build Application
   └─ no args        ─▶ type_reference_symbol_type + module augmentations / enum identity
bare identifier?
   ├─ type parameter?            ─▶ TS2315 if used with args
   ├─ Array/ReadonlyArray/ConcatArray + unshadowed ─▶ factory.array / readonly_type
   ├─ intrinsic (NoInfer, Uppercase, …) ─▶ lowering path
   ├─ lib global ─▶ resolve_lib_type_by_name / canonical lib DefId → Application
   └─ user symbol ─▶ type_reference_symbol_type / Application(Lazy(def), args)
```

Several decisions are pure `tsc` parity:

- **Array canonicalization.** Unshadowed `Array<T>` / `ReadonlyArray<T>` lower
  through the solver `array` / `readonly_type` factory so the generic form
  interns to the *same* `TypeId` as the shorthand `T[]` / `readonly T[]`.
  Otherwise `Array<X>` would be `Application(Lazy(GlobalArrayDef), [X])` and a
  redeclaration `var a: Array<X>; var a: X[]` would fail identity comparison
  (false `TS2403`). Shadowing is detected structurally via the resolved symbol's
  `TYPE_ALIAS` flag — `symbol_is_from_actual_lib` is unreliable because the
  binder often registers a local proxy symbol. `ConcatArray` is excluded (it is
  a distinct interface, not an alias for `T[]`).
- **Imported class in type position.** When `type_reference_symbol_type` already
  produced the class *instance* type for an imported non-generic class, it is
  returned directly. Falling through would re-mint a `Lazy(DefId)` for the
  import-alias symbol whose def has no class-instance env entry, so relation-time
  resolution lands on the class's *static* side ("Property 'prototype' is
  missing", kysely order-by-parser).
- **Eager TS2589 detection.** For a generic alias or class application, the
  function eagerly evaluates the application to detect *excessively deep*
  instantiation. It clears `take_tuple_too_large()` and `depth_exceeded` before
  probing, picks a TS2589-specific evaluator
  (`evaluate_type_for_ts2589_check`) for aliases known to diverge
  (computed-recursive, same-input-recursive-union, default-reset, default-omit),
  and emits `TYPE_INSTANTIATION_IS_EXCESSIVELY_DEEP_AND_POSSIBLY_INFINITE` (or
  the tuple-too-large variant), returning `ANY` to match `tsc`'s
  errorType-suppression of the cascade. It also detects circular *mapped-type*
  aliases (`type Circular<T> = { [P in keyof T]: Circular<T> }`) the evaluator
  cannot expand and additionally emits `TS2615`. Suppression flags
  (`def_body_involves_depth_poisoned_def`, `mark_depth_poisoned`) stop the
  cascade from re-firing on dependent aliases. Evaluation itself is owned by the
  solver ([solver-evaluation](solver-evaluation.md)); this site only *triggers*
  it and *reads* the overflow flags.

When a qualified generic *interface* reference is detected, the function takes a
fast path: it primes the member's def with `ensure_def_ready_for_lowering` and
builds `Application(Lazy(def), args)` directly, because the generic
`TypeLowering` path keys the application to a *different* `DefId` than the one
the priming registered with the declared params — leaving every parameter a free
`T` and producing a false `TS2322` on every `ns.Generic<…>` assignment.

## DefId minting and dual-env registration

The bridge between the checker's `SymbolId` world and the solver's `DefId`
world is `get_or_create_def_id` (`context/def_mapping.rs`, line 56). Its lookup
is a three-tier strategy:

1. **Local cache** `symbol_to_def` (`FxHashMap`, no lock), validated against the
   cross-file collision case and the cached name.
2. **Authoritative index** `DefinitionStore::symbol_def_index` (`DashMap` keyed
   by `(symbol_id, file_idx)`) — disambiguates the same raw `SymbolId(u32)`
   across binders without a multi-binder name scan.
3. **Create**: build `DefinitionInfo`, register in store + index.

`create_lazy_type_ref(sym)` is simply `types.lazy(get_or_create_def_id(sym))`.
`resolve_symbol_as_lazy_type(sym)` (`symbol_types_lazy.rs`) is the canonical
two-step "materialize then refer": call `type_reference_symbol_type(sym)` to
ensure the body is in `type_env`, then return the `Lazy(DefId)`.

The publication writers all live in `def_mapping.rs` and all go through
`register_in_envs` so the deferred-flow-write replay keeps both environments
consistent:

| Writer | Behavior |
| --- | --- |
| `register_def_in_envs(def, body)` | non-generic body; sweeps eval caches only on a genuine rewrite |
| `register_def_with_params_in_envs(def, body, params)` | generic body+params written atomically |
| `register_def_auto_params_in_envs(def, body, params)` | chooses the above by `params.is_empty()` |
| `register_class_instance_in_envs(def, instance)` | class instance side |
| `register_class_extends_in_envs(def, parent)` | nominal `instanceof` narrowing |
| `register_def_symbol_mapping_in_envs(def, sym)` | `DefId <-> SymbolId` bridge for `TypeQuery` |

## Constructor and heritage publication

A class's `get_type_of_symbol` returns its **constructor** type. The build order
in `compute_class_symbol_type` (`computed_helpers_binding.rs`, line 495) is:

1. Build the **instance** type first (`get_class_instance_type`) so the
   constructor's construct-signature return is the *real* instance shape, not a
   prescan approximation — needed for `static getInstance() { return new C() }`.
2. Cache instance in `symbol_instance_types`, guarding against a re-entrant
   degraded (`ANY`/`ERROR`) overwrite of a previously-valid instance.
3. Build the constructor type (`get_class_constructor_type`), detect the
   degenerate `Lazy`-to-self fallback and skip the cache overwrite.
4. Merge namespace exports (`merge_namespace_exports_into_constructor`) and
   function call signatures when the class symbol is also a function/namespace.

The actual *shape* of the instance type — heritage flattening, member synthesis,
mixin handling — is owned by [checker-class-shape-construction](checker-class-shape-construction.md)
and [checker-classes](checker-classes.md); this kernel only *sequences* the
build and *publishes* the results.

Type arguments are applied to a constructor type by
`apply_type_arguments_to_constructor_type[_for_extends]`
(`constructors.rs`), which decomposes intersections
(`T & Constructor<MyMixin>`), instantiates generic construct signatures, and
follows the `default -> constraint -> unknown` fill order for missing args via
`missing_base_type_arg_fill`. That helper treats a degraded `ERROR`
default/constraint as *absent* (using `is_genuine_error_type`, not
`is_error_type`, so a deferrable `UnresolvedTypeName` is still honored), so a
cross-arena base-class cycle cannot bake `error`/`never` into an inherited
type-argument slot (the kysely `SelectFrom<error, …>` leak family, #13484).
`cache_base_instance_result` enforces the same invariant as a cache property:
`Some(ERROR)` is sanitized to `None`.

**Cross-module heritage publication** (`heritage_publication.rs`) handles a
subtle cross-file gap. When a program-module interface `extends` a base that
also lives in a program file, the *declaring* checker can flatten the heritage
but an *importing* file cannot — `merge_interface_heritage_types` cannot read
foreign declaration arenas, and the lib-aware fallback resolves the bare base
name to the local import alias, silently dropping inherited members.

- `publish_heritage_merged_interface_body` registers the merged body in the
  shared `DefinitionStore` when this checker owns every declaration and all
  direct bases resolve to program-file symbols (not lib bases — those already
  merge through the lib-aware path on both sides, and publishing them surfaces
  the resolver-less union-normalization defect #13232).
- `try_consume_published_heritage_body` lets an importing file prefer the
  published body, but only when (a) the local merge was a no-op, (b) the
  published body *covers every locally-derived member* (a partial mid-resolution
  body would trade missing-inherited for missing-own — the msw `NetworkApi`
  family), and (c) the body is *inference-inert* (no conditional reachable
  through alias applications — `contains_conditional_through_aliases`). On
  consumption it invalidates: `clear_type_evaluation_caches_for_def` and
  `invalidate_application_eval_cache_for_def`, because expressions in this file
  may already have cached applications against the heritage-dropped local body.

`register_alias_def_forwarding` links an import-alias `DefId` to its target's
`DefId` via `set_alias_forward` so alias-keyed and declaration-keyed
applications compare as one definition in relation logic — *without* registering
a body (the importing checker's local lowering can be heritage-incomplete, and
registering it would shadow the richer cross-arena delegation paths).

## Cross-file and cross-arena delegation

When a symbol is owned by another file, resolution physically *delegates* into a
child `CheckerState` bound to the owner's arena and binder.
`compute_type_of_symbol` calls `delegate_cross_arena_symbol_resolution`
(`type_analysis/cross_file.rs`) early, but that function first *declines* to
delegate when the symbol genuinely has a same-named declaration in the current
arena (a `TYPE_ALIAS`/`CLASS` collision on raw `SymbolId`/`NodeIndex`), handling
it locally instead. `cross_file_js_constructor_instance_type`
(`cross_file_constructors.rs`) is a focused instance of this: it spins up a
`CheckerState::with_parent_cache_attributed` child (reason
`DelegateCrossArenaOther`), marks its diagnostics discarded, copies cross-file
state and symbol-file targets, sets `current_file_idx` to the owner, and calls
`synthesize_js_constructor_instance_type` for a JS constructor-function base —
all under `enter_cross_arena_delegation` + `enter_recursion` guards. The
cross-file *export* lookup that resolves a specifier+name to a `SymbolId` is
`resolve_cross_file_export_from_file_with_mode` (`cross_file_export.rs`), which
honors a `resolution-mode` override (ESM `import` vs CJS `require`), walks
`module_exports` -> module-augmentation exports -> re-export chains, and only
falls back to `file_locals` for non-external script files (returning a module's
local imports through `import * as ns` is rejected by `tsc` with `TS2339`,
#3585). Module-resolution mechanics themselves are
[module-resolution-engine](module-resolution-engine.md).

## import_type: `import("./m").Foo`

`check_import_type_and_resolve` (`import_type.rs`, line 729) handles a
`TypeReference` whose name is rooted in an `import("…")` call expression. It
extracts the module specifier and resolution-mode override, then splits on
whether there are qualified member segments:

- **Bare** `import("./m")` in type position: validated against
  `bare_import_type_refers_to_type` — it must resolve to a type-meaning
  (`export =` of a type, or an interface/alias). Otherwise it emits
  `MODULE_DOES_NOT_REFER_TO_A_TYPE_BUT_IS_USED_AS_A_TYPE_HERE_DID_YOU_MEAN_TYPEOF_I`
  (unless suppressed by a parse diagnostic or import-type options).
- **Qualified** `import("./m").Foo`: `resolve_ts_import_type_member_symbol`
  resolves `Foo` through `resolve_import_with_reexports_type_only`, then
  effective module exports (covering ambient `declare module "mod"`), then the
  target file's exports and wildcard re-export chains across binders. Type
  arguments are then applied as `Application(Lazy(def), args)` (back in
  `get_type_from_type_reference`), with arity validation (`TS2344`).

## Computed property names

When an interface (or class member set) contains computed keys (`[k]: T` where
`k` is a `const` unique-symbol variable, or `[Symbol.iterator]`),
`TypeLowering` cannot resolve the key from AST alone. `computed_property_names.rs`
pre-computes them into maps keyed by `(expression NodeIndex, arena_ptr)`:

- `precompute_computed_property_names` -> `FxHashMap<(NodeIndex, usize), Atom>`
  for resolvable literal/well-known names.
- `precompute_symbol_named_computed_property_names` -> a set of expressions whose
  key is a unique-symbol/symbol-named member (rendered `__unique_<sym>` /
  `__symbol_<…>`).
- `prewarm_member_type_reference_params` -> per-`DefId` param lists (skipped in
  declaration files, where the lowering's on-demand
  `get_def_type_params(def)` fallback is cheaper than walking giant `.d.ts`
  interface graphs).

These maps are handed to `TypeLowering` via `with_computed_name_resolver`,
`with_computed_symbol_name_resolver`, and `with_lazy_type_params_resolver`. The
local resolution path (`resolve_local_computed_property_name`) flips
`preserve_literal_types` and `checking_computed_property_name` while typing the
key expression so a `const k = "x"` yields the literal `"x"` rather than the
widened `string`. The well-known-symbol mapping (`Symbol.iterator` ->
`__@iterator`) is owned by `types_domain::computed_names`, with a `Shadowed`
case so a user-redeclared `Symbol` does not get the global name.

## Caches and invariants

| Cache | Key -> value | Population / invalidation |
| --- | --- | --- |
| `ctx.symbol_types` | `SymbolId -> TypeId` | written in `get_type_of_symbol` step 7; placeholder seeded in step 5; stale alias placeholders evicted in step 2; dropped for bailout/self-lazy/in-flight-class results |
| `ctx.symbol_instance_types` | `SymbolId -> TypeId` | class/interface instance side; guarded against degraded overwrite |
| cross-file symbol cache | `(SymbolId, file_idx) -> (TypeId, params)` | `cache_cross_file_symbol_type`; gated for declaration-file classes on `class_instance_recoverable` |
| `type_env` / `type_environment` | `SymbolRef`/`DefId` -> body, instance, extends, enum-parent | dual-write via `register_*_in_envs`; mirror defers on borrow race, replayed at `flush_deferred_flow_env_writes` |
| `DefinitionStore` (shared) | `DefId -> body/params`, `TypeId -> DefId`, alias forwards | `register_def_*`; rewrite-gated eval-cache sweep |
| `symbol_to_def` / `def_to_symbol` | `SymbolId <-> DefId` | `get_or_create_def_id`; backed by `DashMap` index |
| `base_instance_expr_cache` | heritage expr `NodeIndex -> Option<TypeId>` | `cache_base_instance_result`; `Some(ERROR)` sanitized to `None` |
| `enum_namespace_types` | `SymbolId -> TypeId` | enum branch; for `typeof E` / `keyof typeof E` |
| `import_type_alias_types` | `SymbolId -> TypeId` | `TYPE_ALIAS`+`ALIAS` merge preference in type-ref position |

Invariants the kernel preserves:

- **`ERROR` is never a `tsc` type.** It is tsz's cycle/fuel sentinel. It is kept
  out of base-instance type-argument slots, env registration, and the
  `DefinitionStore` body; cycle markers (`ERROR`/`UNKNOWN`) are deliberately not
  promoted to authoritative cross-file results.
- **A `Lazy(def)` placeholder is never persisted as `Lazy(def) -> def`.**
  Self-loops are detected (`lazy_def_id(...) == Some(def)`) and skipped.
- **Body publication is atomic with params** (`register_def_with_params_in_envs`)
  so no reader sees a generic alias with a visible body but a missing param list.
- **Both `TypeEnvironment`s converge.** Every def write goes to both, with
  deferred replay on borrow contention.
- **First-publication does not sweep.** `register_def_in_envs` skips the
  `O(env_eval_cache)` sweep on `None -> Some(body)` (the solver refuses to
  persist results computed against an unresolved def), gating it on a genuine
  `Some(old) -> Some(new)` rewrite — avoiding `O(N^2)` cost across a file of `N`
  deferred-bodied aliases.

## Edge cases and tsc parity

| Case | tsc behavior | Where tsz matches it |
| --- | --- | --- |
| `interface User { x: Filtered }` + `type Filtered = …keyof User…` | resolves via deferred evaluation | cycle returns un-cached `Lazy(def)` (step 4); `keyof Lazy(User)` defers |
| `typeof foo<T>` in `foo`'s own return type | provisional, no overflow | `ANY` sentinel + `provisional_circular_function_symbol_type` (step 2/5) |
| `var a: Array<X>; var a: X[]` | same type, no error | `Array<X>` canonicalized to `factory.array(X)` (no `TS2403`) |
| imported non-generic class in type position | instance type, has `prototype` | instance returned directly, no re-mint to import-alias `Lazy` |
| `type Deep<T> = …` excessively deep instantiation | `TS2589`, errorType `any` | eager `evaluate_type_for_ts2589_check`, returns `ANY` |
| `type Circular<T> = { [P in keyof T]: Circular<T> }` | `TS2589` + `TS2615` | circular-mapped detection emits both |
| `export const` re-imported via `export *` cycle | resolves to declared literal | `value_variable_owned_by_current_file_not_foreign` forces local resolution |
| `interface D extends Base` imported across files | inherited members visible | publish/consume heritage body (`heritage_publication.rs`) |
| `import("./m").Foo` where `m` has no type meaning | `TS1340`-family "does not refer to a type" | `bare_import_type_refers_to_type` gate |
| `import * as ns from "./self"; ns.localImport` | `TS2339` | `cross_file_export.rs` `file_locals` fallback restricted to script files |
| `declare module "x" { interface Foo { … } }` augmenting an exported interface | merged members everywhere | `apply_self_module_augmentations` at the canonical resolution point (step 6) |
| numeric enum used in a mapped type `{ [k in E]?: T }` | discrete `"0"`,`"1"` keys | per-member `literal_number` synthesis in the enum branch |
| `class C` + `interface C` + `namespace C` merge | instance carries all members | class branch in `type_reference_symbol_type` (not interface branch, which drops class members) |

## Where to go next

- The *evaluation* of the `Lazy`/`Application`/conditional types this kernel
  publishes: [solver-evaluation](solver-evaluation.md).
- The `DefId`/`TypeEnvironment`/`DefinitionStore` universe these writers target:
  [solver-types-intern-def](solver-types-intern-def.md).
- How a class *shape* (vs. its resolution sequencing here) is built:
  [checker-class-shape-construction](checker-class-shape-construction.md),
  [checker-classes](checker-classes.md).
- The instantiation that consumes the `(body, params)` pairs:
  [solver-instantiation](solver-instantiation.md).
- The flow-analyzer environment this kernel mirrors into:
  [checker-flow-and-narrowing](checker-flow-and-narrowing.md).
- The call-site that drives this per file:
  [checker-context-and-state](checker-context-and-state.md),
  [end-to-end-timeline](end-to-end-timeline.md).
- Module specifier resolution behind the cross-file paths:
  [module-resolution-engine](module-resolution-engine.md),
  [checker-declarations-modules](checker-declarations-modules.md).
