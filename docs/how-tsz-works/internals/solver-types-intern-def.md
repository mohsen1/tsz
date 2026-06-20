# The Type Universe: TypeData, Interning, DefId/Lazy Resolution, Canonicalization

Every type the compiler ever reasons about is, at the bottom, a single 32-bit
integer: a `TypeId(u32)`. `string`, `{ x: number }`, `Promise<readonly T[]>`,
the union `"a" | "b" | undefined`, and the still-unevaluated conditional
`T extends U ? X : Y` are all handed around as four-byte handles. Two types are
the *same* type if and only if their `TypeId`s compare equal as `u32` — an O(1)
integer compare, no structural walk. This is the central bargain of the solver:
make identity cheap by paying the cost once, at construction time, through a
content-addressed **interning** table. The structural payload behind a handle is
a `TypeData` enum stored in `crates/tsz-solver/src/types.rs`; the table that maps
`TypeData -> TypeId` is the `TypeInterner` in
`crates/tsz-solver/src/intern/core/interner.rs`.

This document covers the *type universe* itself: the `TypeData` variants and the
identity decisions baked into them, how the sharded interner assigns and reuses
`TypeId`s, the thread-local fast-path caches, how a named type reference
(`TypeData::Lazy(DefId)`) is resolved on demand through a `TypeEnvironment`, and
how the `Canonicalizer` collapses alpha-equivalent and recursive types — and
drops cosmetic value-names — so that structurally identical types share one
identity. Sibling internals docs cover the things that *consume* this universe:
relations ([solver-relations](solver-relations.md)), evaluation
([solver-evaluation](solver-evaluation.md)), instantiation
([solver-instantiation](solver-instantiation.md)), inference
([solver-inference](solver-inference.md)), narrowing
([solver-narrowing](solver-narrowing.md)), the broader cache landscape
([solver-caches-objects-contextual-compat](solver-caches-objects-contextual-compat.md)),
and the checker side that asks the solver to build types
([checker-context-and-state](checker-context-and-state.md),
[checker-declarations-modules](checker-declarations-modules.md)).

## Owns / Must not own

**Owns:**

- The canonical `TypeData` enum and every interned shape struct (`ObjectShape`,
  `CallableShape`, `FunctionShape`, `PropertyInfo`, `IndexSignature`,
  `TupleElement`, `ConditionalType`, `MappedType`, `TypeParamInfo`, `ParamInfo`).
- The `TypeInterner` content-addressed table: structural hashing, shard
  allocation, the `TypeData <-> TypeId` bijection, and the family of side
  interners for type-lists, tuples, templates, and value shapes.
- The intrinsic / sentinel `TypeId` constants (`ERROR`, `NEVER`, `UNKNOWN`,
  `ANY`, `VOID`, the boxed primitives) and the local/global `TypeId` partition.
- The `DefId` definition-identity universe (`DefinitionStore`) and the
  `TypeEnvironment` resolver that turns `DefId -> TypeId` lazily.
- Canonicalization: De Bruijn rewriting of recursion and bound parameters, plus
  the comparison-only erasure of cosmetic value-names.
- The thread-local lookup/intern/string caches and the per-`TypeId` structural
  predicate memo (`predicate_cache`).

**Must not own:**

- *Relation* decisions. Whether two distinct `TypeId`s are subtypes or
  assignable is the relation engine's job ([solver-relations](solver-relations.md));
  the interner's `PartialEq` is *identity*, never *subtyping*.
- *Evaluation*. Reducing `Conditional`/`Mapped`/`IndexAccess`/`Application` to a
  concrete shape belongs to the evaluator ([solver-evaluation](solver-evaluation.md));
  the interner only stores the unevaluated node.
- *Diagnostics and printing*. The interner stores display provenance
  (`display_alias`, `display_properties`, `display_union_origin`) but never reads
  rendered output; the printer reads types, types never read the printer.
- *Source orchestration*. The checker decides *which* types to build and *when*;
  it must not construct raw `TypeKey`s or pattern-match interner internals.

## Where it lives

| Path | Role |
|------|------|
| `crates/tsz-solver/src/types.rs` | `TypeData`, `TypeId`, all shape structs, intrinsic constants, local/global partition. |
| `crates/tsz-solver/src/types/shape_identity.rs` | Hand-written `PartialEq`/`Eq`/`Hash` for `PropertyInfo`/`IndexSignature`/`ObjectShape`/`CallableShape` — the per-field identity decisions. |
| `crates/tsz-solver/src/intern/core/interner.rs` | `TypeInterner` struct, `intern`/`lookup` hot paths, intrinsic shortcut, circuit breaker, predicate memo. |
| `crates/tsz-solver/src/intern/core/interner/storage.rs` | `TypeShard`, `ConcurrentSliceInterner`, `ConcurrentValueInterner`, the arrival-order-immune `write_id_slot`. |
| `crates/tsz-solver/src/intern/core/interner/cache.rs` | Thread-local direct-mapped lookup/intern/string caches scoped by `instance_id`. |
| `crates/tsz-solver/src/intern/core/constructors.rs` | Public type-construction surface (`union`, `array`, `lazy`, `recursive`, `bound_parameter`, ...). |
| `crates/tsz-solver/src/def/core.rs` | `DefId`, `DefKind`, `DefinitionInfo`, `DefinitionStore`. |
| `crates/tsz-solver/src/def/resolver.rs` | `TypeResolver` trait + `TypeEnvironment` (`DefId -> TypeId` resolution). |
| `crates/tsz-solver/src/canonicalize/mod.rs` | `Canonicalizer`: De Bruijn rewriting + cosmetic-name erasure. |
| `crates/tsz-common/src/id/mod.rs` | `define_id!`, the single source of truth for every `u32` handle newtype. |
| `crates/tsz-common/src/interner/mod.rs` | `Atom`/`AstAtom` string handles and the `ShardedInterner`. |
| `crates/tsz-solver/src/caches/db.rs` | `TypeDatabase` trait — the query boundary the checker calls through. |

## The handle family: `define_id!`

`TypeId`, `DefId`, `Atom`, and a dozen interned-shape ids (`ObjectShapeId`,
`TypeListId`, `FunctionShapeId`, `CallableShapeId`, `TypeApplicationId`,
`TemplateLiteralId`, `ConditionalTypeId`, `MappedTypeId`, `TupleListId`,
`SymbolRef`) are all `pub struct N(pub u32)` newtypes minted by one macro,
`define_id!` (`crates/tsz-common/src/id/mod.rs`). The macro emits the shared
derive cluster `Copy, Clone, Debug, PartialEq, Eq, Hash` and an optional
sentinel block parameterized by null convention: `sentinel: zero` (interned-atom
convention, `NONE = Self(0)`) or `sentinel: max` (binder/parser arena-index
convention, `NONE = Self(u32::MAX)`). Keeping the skeleton in one macro means the
derive surface cannot drift handle-to-handle; type-specific constants like
`TypeId::ERROR` or `DefId::INVALID` live in adjacent inherent `impl` blocks.

The distinct types are load-bearing: an `Atom` minted by the program-wide solver
interner and an `AstAtom` minted by a per-file scanner interner use *incompatible
encodings* for the same string, so the type system makes cross-namespace use a
compile error. The fix is "resolve the string and re-intern," never "copy the raw
`u32`."

## `TypeId`: the layout and its sentinels

`TypeId` is defined in `types.rs` with a fixed block of intrinsic constants in
`0..FIRST_USER` (`FIRST_USER = 100`):

| `TypeId` | Value | Meaning |
|----------|-------|---------|
| `NONE` | 0 | internal placeholder, no valid type |
| `ERROR` | 1 | type resolution failed; contagious through operations |
| `NEVER` | 2 | bottom type |
| `UNKNOWN` | 3 | TypeScript `unknown` |
| `ANY` | 4 | TypeScript `any`; opts out of checking |
| `VOID` / `UNDEFINED` / `NULL` | 5 / 6 / 7 | |
| `BOOLEAN` / `NUMBER` / `STRING` / `BIGINT` / `SYMBOL` / `OBJECT` | 8..13 | primitives |
| `BOOLEAN_TRUE` / `BOOLEAN_FALSE` | 14 / 15 | the two boolean literal types |
| `FUNCTION` | 16 | the `Function` intrinsic |
| `PROMISE_BASE` | 17 | synthetic Promise base for `await` extraction |
| `STRICT_ANY` | 19 | strict-mode `any` that does *not* silence structural mismatches |

`TypeId::is_intrinsic()` is a free `self.0 < FIRST_USER` range check, used all
over the codebase as a fast-path guard (e.g. the `Canonicalizer` returns
intrinsics unchanged before touching its cache). The four sentinels carry
behavior, not just identity: `ERROR` propagates through property access and
operations to prevent cascading errors; `ANY` succeeds permissively; `UNKNOWN`
yields the checker's TS2571 path; `NEVER` is the exhaustive-narrowing remainder.

`TypeId` is also partitioned by its most-significant bit (`LOCAL_MASK =
0x80000000`). Global ids (MSB clear) are minted by the long-lived `TypeInterner`;
local ids (MSB set) belong to an ephemeral `ScopedTypeInterner` and are freed
when it drops. The interner's `make_id` asserts `id.is_global()` in debug builds
so a global allocation can never collide with the local half.

## `TypeData`: the structural universe

`TypeData` (`types.rs`, `pub enum TypeData`) is the structural "shape" used as the
interner key: structurally identical types produce equal `TypeData` and therefore
the same `TypeId`. It is `Copy` — every variant is either a small scalar or a
`u32` handle into a *side* interner, never an owned `Vec`. That is what keeps the
`key_to_index` map cheap and the `lookup` cache `Copy`.

The variants split into a few families:

- **Leaves**: `Intrinsic(IntrinsicKind)`, `Literal(LiteralValue)`, `ThisType`,
  `Error`, `UnresolvedTypeName(Atom)`. `LiteralValue` is `String(Atom)`,
  `Number(OrderedFloat)`, `BigInt(Atom)`, or `Boolean(bool)`; `OrderedFloat`
  hashes and compares on `f64::to_bits` so `0.0` and `-0.0` (and `NaN`) intern
  deterministically.
- **Composites by side-handle**: `Object(ObjectShapeId)`,
  `ObjectWithIndex(ObjectShapeId)`, `Union(TypeListId)`,
  `Intersection(TypeListId)`, `Array(TypeId)`, `Tuple(TupleListId)`,
  `Function(FunctionShapeId)`, `Callable(CallableShapeId)`,
  `Application(TypeApplicationId)`, `Conditional(ConditionalTypeId)`,
  `Mapped(MappedTypeId)`, `TemplateLiteral(TemplateLiteralId)`. The big payloads
  (property lists, signature lists) live in dedicated interners so the enum stays
  `Copy` and small.
- **Operators held unevaluated**: `IndexAccess(TypeId, TypeId)`, `KeyOf(TypeId)`,
  `ReadonlyType(TypeId)`, `StringIntrinsic { kind, type_arg }`, `NoInfer(TypeId)`.
  The interner stores these *as written*; the evaluator reduces them later.
- **Generics and binders**: `TypeParameter(TypeParamInfo)`, `Infer(TypeParamInfo)`,
  `BoundParameter(u32)`, `Recursive(u32)`. The last two are De Bruijn indices the
  `Canonicalizer` produces (see below).
- **Named references**: `Lazy(DefId)` and the nominal `Enum(DefId, TypeId)` —
  where the `DefId` carries nominal identity and the `TypeId` is the structural
  member union (`0 | 1` for a numeric enum). `TypeQuery(SymbolRef)`,
  `UniqueSymbol(SymbolRef)`, and `ModuleNamespace(SymbolRef)` are the remaining
  symbol-backed deferrals.

A historical note encoded in the source: `TypeData::Ref(SymbolRef)` is removed —
the migration to `Lazy(DefId)` is complete (PHASE 4.2 comment in `types.rs`). New
references use `Lazy(DefId)`.

### Interned shapes and the identity decisions in them

`ObjectShape` is the key non-trivial shape:

```text
ObjectShape {
    flags: ObjectFlags,
    properties: Vec<PropertyInfo>,   // sorted by name for stable identity
    string_index: Option<IndexSignature>,
    number_index: Option<IndexSignature>,
    symbol: Option<SymbolId>,        // nominal identity, in PartialEq + Hash
}
```

Crucially, `PartialEq`/`Eq`/`Hash` for `ObjectShape`, `CallableShape`,
`PropertyInfo`, and `IndexSignature` are **hand-written** in
`types/shape_identity.rs`, not derived. The module forces an explicit
identity decision per field by exhaustively destructuring `Self` (no `..` rest
patterns allowed): adding a field is a compile error until you classify it. This
is what keeps cosmetic state out of structural identity. For `PropertyInfo`:

- **Identity-bearing**: `name`, `type_id`, `write_type`, `optional`, `readonly`,
  `is_method`, `visibility`, `parent_id`, `is_string_named`, `is_symbol_named`.
  `is_string_named` matters because `"100"` and `100` are different property keys
  in TypeScript.
- **Identity-exempt (cosmetic)**: `is_class_prototype` (spread-exclusion
  metadata), `declaration_order` (display/emit order), `single_quoted_name`
  (`.d.ts` quote style). So `{ 'foo': string }` and `{ "foo": string }` intern to
  the *same* `TypeId`.

`IndexSignature::param_name` (the source `[k: string]` vs `[key: string]` name) is
exempt at the `IndexSignature` level but **re-added** at the *shape* level via
`index_signature_display_eq` / `hash_index_signature_display`, because diagnostics
must print the source parameter name. Similarly, `declaration_order` is normally
exempt but becomes identity-bearing on an `ObjectShape` carrying
`ObjectFlags::PRESERVE_DECLARATION_ORDER` — an anonymous object whose member order
is semantically irrelevant but must stay a distinct `TypeId` so post-widening
diagnostics print source order.

The `symbol` field is the *nominal brand*: it is in both `PartialEq` and `Hash`,
so `class A {}` and `class B {}` with identical members get different
`ObjectShapeId`s and thus different `TypeId`s. The header comment is emphatic:
"Structural subtyping is computed explicitly in the Solver, not via `PartialEq`."
The interner's equality answers *"is this the same interned type?"*, never *"do
these two types relate?"*.

`ObjectFlags` is a `bitflags` set on the shape (e.g. `FRESH_LITERAL` for
excess-property checking, `INTERSECTION_MERGED` to keep a merged `A & B` object a
distinct id from a plain `{ a; b }`, `MAPPED_CONSTRAINT_KEYS` for non-homomorphic
mapped `keyof` semantics). The flags are part of identity, so a fresh literal and
its widened twin are distinct `TypeId`s.

## The `TypeInterner`: content-addressed identity

`TypeInterner` (`intern/core/interner.rs`) is the table that makes the
`TypeData <-> TypeId` mapping a bijection. It is lock-free-by-sharding and
lazily initialized: most of its `DashMap`/`OnceLock` fields allocate only on first
use, which keeps startup cheap.

### Sharded storage

The `TypeData` storage is split across `SHARD_COUNT = 64` shards
(`SHARD_BITS = 6`), each a `TypeShard` (`storage.rs`):

```text
TypeShardInner {
    key_to_index: DashMap<TypeData, u32>,   // forward: structure -> local index
    index_to_key: RwLock<Vec<TypeData>>,    // reverse: local index -> structure
    alloc_order:  RwLock<Vec<u32>>,         // monotonic alloc order, for sorting
}
```

The shard is chosen by the low bits of the structural hash: `shard_idx = hash &
(SHARD_COUNT - 1)`. The public `TypeId` is reconstructed from the shard index and
the in-shard local index by `make_id`:

```text
raw_val = (local_index << SHARD_BITS) | (shard_idx & SHARD_MASK)
TypeId  = FIRST_USER + raw_val
```

so `lookup_slow` inverts it: `raw = id.0 - FIRST_USER`, `shard = raw &
SHARD_MASK`, `local = raw >> SHARD_BITS`. Because the shard index is embedded in
the *low* bits, raw `TypeId` ordering is hash-dependent, not allocation order —
which is why the interner *also* records a global monotonic `alloc_counter` per
allocation (the `alloc_order` vec). Union member sorting and other deterministic
orderings consult `lookup_alloc_order` to approximate tsc's source-order id
allocation rather than comparing raw `TypeId`s.

### Side interners

Variable-length and large payloads live in their own interners (all in
`storage.rs`):

- `ConcurrentSliceInterner<TypeId>` for union/intersection member lists
  (`TypeListId`), `ConcurrentSliceInterner<TupleElement>` for tuples, and
  `ConcurrentSliceInterner<TemplateSpan>` for template-literal spans. Id `0` is
  reserved for the empty slice.
- `ConcurrentValueInterner<T>` for `ObjectShape`, `FunctionShape`,
  `CallableShape`, `ConditionalType`, `MappedType`, `TypeApplication`. Each value
  is stored behind an `Arc<T>` so `lookup`-style reads clone an `Arc`, not the
  whole shape.

So `union([a, b, c])` interns the member list into a `TypeListId`, then interns
`TypeData::Union(list_id)` into a `TypeId`. Two unions with the same canonicalized
member list share the `TypeListId` and therefore the `TypeId`.

### The append protocol: arrival-order immunity

A subtle correctness point: an index is allocated by `next_index.fetch_add`
*before* the storage `RwLock` is taken, so writers can reach the lock out of id
order. `write_id_slot` (`storage.rs`) handles this with a resize-and-place
protocol rather than a `push` loop: if the vec is shorter than `index`, it
`resize`s with a placeholder and pushes the value at `index`; if equal, it
pushes; if longer, it overwrites the slot. A naive `while push` backfill would let
an earlier-arriving higher id fill a lower id's slot, permanently misaligning ids
and data. The published `key_to_index` entry is inserted *after* the slot is
written, so a concurrent reader can never observe an id whose `index_to_key` slot
is still unwritten.

### `intern`: the hot path

`TypeInterner::intern(key: TypeData) -> TypeId` (interner.rs) walks a layered
fast-to-slow path:

1. **Intrinsic shortcut** — `get_intrinsic_id` maps `Intrinsic(_)`, `Error`, and
   the two boolean literals to their fixed constants with no hashing. (`true` and
   `false` map to `BOOLEAN_TRUE`/`BOOLEAN_FALSE` so boolean literals never
   duplicate.)
2. **Thread-local intern cache** — hash the `TypeData` with `FxHasher`, then
   `cache::intern_probe(hash, instance_id, &key)`. A 512-entry direct-mapped TLS
   table keyed by hash (with a full key compare on hit) turns the common repeat
   into one array index. The probe is scoped by `instance_id`, so a stale entry
   from a *different* `TypeInterner` on the same thread is treated as a miss.
3. **`intern_slow`** — select the shard from the hash; do a lock-free
   `key_to_index.get(&key)` read; on miss, allocate a local index and `entry()`
   into the shard's `DashMap` (the vacant arm writes the storage slot and the
   alloc-order slot under the shard `RwLock`, then publishes the index; the
   occupied arm means another thread won the race, so we reuse its id).

On a successful slow-path miss the result is written back into the TLS intern
cache so the next repeat hits step 2. The whole path increments perf counters
(`interner_intern_calls`/`_hits`/`_misses`) only when `enabled_fast()` is set, so
default runs pay just one `OnceLock<bool>` read.

`intern_fresh` is the deliberate exception: it *bypasses* `key_to_index` and the
TLS cache to mint a brand-new id for a declaration-scoped type — used by
`fresh_type_param` so two same-named, same-constraint type-parameter declarations
do **not** collapse into one semantic parameter. The data is still readable via
`lookup`; only the dedup is skipped.

### `lookup`: the reverse direction

`lookup(id) -> Option<TypeData>` is the inverse. It serves intrinsics from
`get_intrinsic_key` (no storage touch), then probes a 1024-entry direct-mapped TLS
*lookup* cache (`cache.rs`, scoped by `instance_id`), then falls to
`lookup_slow`, which takes the shard's `RwLock` read lock and indexes the
`index_to_key` vec. The TLS cache turns the common case from a `RwLock::read`
(~15-25 ns: atomic CAS on the reader count, fences) into a single array index +
compare (~1-2 ns). On a cold-path resolve the entry is written back into the TLS
cache. (An opt-in `TSZ_PROMOTE_FIRST` global "promoted tier" is a
*measurement-only* larger cache that never changes the answer.)

### Data-flow summary

```text
checker / solver constructor                  side interners
        |                                  +---------------------+
   db.union([a,b])  --------------------->| TypeListId for [a,b] |
        |                                  +----------+----------+
        v                                             |
   TypeData::Union(list_id)                           |
        |                                              v
        |   intern(key)                       (Copy enum, 8-16 bytes)
        v
  +-------------------+   intrinsic?  +-----------+
  | get_intrinsic_id  |-------------->| const id  |
  +---------+---------+               +-----------+
            | no
            v
  +-------------------+   TLS hit?    +-----------+
  | FxHash + TLS probe|-------------->| cached id |
  +---------+---------+               +-----------+
            | miss
            v
  +-------------------+   shard read hit?  +----------------+
  | intern_slow:      |------------------->| existing id    |
  |  key_to_index.get |                    +----------------+
  +---------+---------+
            | miss -> alloc local_index, write slot, publish
            v
       make_id(local_index, shard) = FIRST_USER + ((local<<6)|shard)
            |
            v
        TypeId(u32)  <----  O(1) equality forever after
```

## `DefId` and `Lazy`: named-type indirection

A reference to a *named* type (`interface`, `class`, `type` alias, `enum`,
namespace) is **not** interned as its expanded structure. It is interned as
`TypeData::Lazy(DefId)`. This indirection is what breaks the infinite regress of
recursive interfaces (`interface Node { next: Node }`) and what decouples the
solver from the binder's symbol representation.

`DefId` (`def/core.rs`) is a solver-owned identity for a definition. Unlike a
binder `SymbolRef`, a `DefId` can be minted without binder context, supports
content-addressed hashing for LSP stability, and is a stable key for incremental
caching. `DefId::INVALID = 0`; valid ids start at `FIRST_VALID = 1`.

### `DefinitionStore`: the definition universe

`DefinitionStore` (`def/core.rs`) is the program-wide, thread-safe store mapping
`DefId -> DefinitionInfo` plus a battery of reverse indices. `DefinitionInfo`
carries everything the checker pre-populates from the binder: `kind: DefKind`,
`name: Atom`, `type_params`, the structural `body: Option<TypeId>`, class
`instance_shape`/`static_shape`/`extends`/`implements`, `enum_members`,
namespace `exports`, the originating `symbol_id`, heritage names, and modifier
flags (`is_abstract`, `is_const`, `is_exported`, `is_declare`,
`is_global_augmentation`).

`DefKind` drives evaluation and nominality:

| `DefKind` | Expansion | Nominal |
|-----------|-----------|---------|
| `TypeAlias` | always expand (transparent) | no |
| `Interface` | lazy expand | no |
| `Class` | lazy expand | yes (brand) |
| `Enum` | special member handling | yes |
| `Namespace` | export lookup | no |
| `ClassConstructor` | `typeof Class` static side | yes |
| `Function` / `Variable` | value-space | no |

The store is registration-driven. `DefinitionStore::register(info)` allocates a
fresh `DefId`, populates the reverse indices it can (`symbol_only_index`,
`body_to_alias` for non-generic aliases, `shape_to_def`, `file_to_defs`,
`name_to_defs`), inserts the `DefinitionInfo`, and bumps a monotonic
`generation`. The body is often filled in *later* via `set_body` /
`set_body_with_params` (a class's instance shape may not be known when the
declaration is first registered). The store carries a number of O(1) reverse
indices specifically to keep the `TypeFormatter` from doing O(N) scans: `type_to_def`,
`body_to_alias`, `shape_to_def`, `symbol_only_index`, `file_to_defs`. It also
holds `alias_forwards` (import-alias `DefId -> declaring DefId`) so a type
annotation that lowered the *alias* name and the declaring module's own reference
canonicalize to one definition rather than degrading into an
opaque-vs-expanded mismatch in relations.

### `TypeEnvironment`: resolving `DefId -> TypeId`

The checker stabilizes `DefId`s; the `TypeEnvironment` (`def/resolver.rs`)
resolves them to `TypeId`s on demand. It implements the `TypeResolver` trait,
whose key methods are:

- `resolve_lazy(def_id, interner) -> Option<TypeId>` — the `Lazy(DefId)`
  resolver. For a class it returns the **instance** type (`class_instance_types`);
  otherwise it returns the registered body via `get_def`, which falls back to the
  shared `DefinitionStore::get_body` on a local miss.
- `resolve_type_query(symbol, ...)` — the `typeof X` resolver, which must return
  the **value-space** (constructor / variable) type, not the instance type. A
  merged interface+value symbol stores its instance type under the shared `DefId`,
  so `resolve_type_query` first consults a separate `typeof_value_types` map.
- `get_lazy_type_params(def_id)` / `get_type_param_variance(def_id)` — generic
  argument expansion and declared `in`/`out` variance.
- `get_def_kind(def_id)` — the `DefKind` the `Canonicalizer` needs to decide
  structural (`TypeAlias`) vs nominal (Interface/Class/Enum) handling.

`resolve_lazy` has a careful fallback for "zombie" `DefId`s — ids minted by the
legacy `interner.reference(SymbolRef(N))`, whose numeric value `N` is really a
raw `SymbolId`. `raw_symbol_fallback_def` reinterprets `N` as a `SymbolId` *only*
when the `DefinitionStore` does not already `contains(def_id)`; for a genuinely
store-registered `DefId` it returns `None` so callers defer instead of resolving a
numeric collision. The defect this guards (#13862): `HTMLDivElement` resolved to
`DefId(218)`, and with its body not yet materialized the old fallback re-read
`218` as a `SymbolId` and answered with `FileSystemEntry`, corrupting
`HTMLElementTagNameMap["div"]`.

### Resolution data-flow

```text
source: interface Box<T> { value: T }     usage: let b: Box<string>
   |                                          |
   v  (checker, binder-driven)                v  (checker lowers annotation)
DefinitionStore::register(                 TypeData::Application(
  DefinitionInfo {                            base = TypeData::Lazy(DefId(Box)),
    kind: Interface, name: "Box",             args = [STRING]
    type_params: [T], body: <shape> })     )
   |                                          |
   |  -> DefId(Box)                           |  intern -> TypeId
   v                                          v
TypeEnvironment.def_types[Box] = body    relation / evaluation asks:
TypeEnvironment.def_type_params[Box]=[T]   resolver.resolve_lazy(DefId(Box))
                                              -> body TypeId  (lazy, on demand)
                                           resolver.get_lazy_type_params(Box)
                                              -> [T]  (for Box<string> expansion)
```

The `generation` counter is the invalidation hinge between this universe and the
solver's caches. Every mutator that can change what a later `resolve_lazy` returns
(`register_def`, `register_class_instance_type`, `set_body`, alias forwarding,
...) calls `bump_generation`. Relation and narrowing caches whose results depend
on lazy resolution fold `resolver_generation()` into their cache keys, so a body
that materializes after a cached miss does not serve a stale answer. (See the
targeted `env_eval_cache` invalidation work in the checker for the per-`TypeId`
refinement of this.)

## Canonicalization: collapsing alpha-equivalence and recursion

Interning gives identity for *literally identical* `TypeData`. But TypeScript
considers many *differently spelled* types identical:

- `type F<T> = { value: T }` and `type G<U> = { value: U }` — alpha-equivalent
  generic aliases.
- `type A = { x: A }` and `type B = { x: B }` — structurally identical recursive
  aliases.
- `<T extends X>() => T` and `<U extends X>() => U` — alpha-equivalent callables.
- `(a: string) => void` and `(b: string) => void` — same type; parameter *names*
  are cosmetic.

The `Canonicalizer` (`canonicalize/mod.rs`) rewrites a `TypeId` into a *canonical*
`TypeId` such that two structurally-equivalent types produce the same handle.
This canonical form is **for comparison and hashing only, never for display.**
The relation engine uses it as an identity fast path: `relations/subtype/core.rs`
canonicalizes both sides (`Canonicalizer::canonicalize`, or
`canonicalize_with_param_scope` when comparing constraints whose parameters are
free relative to the input) and short-circuits when the canonical ids match.

### De Bruijn rewriting

The canonicalizer carries two stacks: `def_stack` (the `DefId`s currently being
expanded, for `Recursive(n)`) and `param_stack` (scopes of type-parameter names,
for `BoundParameter(n)`). Two `TypeData` variants exist solely as the output of
this rewrite:

- `BoundParameter(n)` — a type parameter by De Bruijn index. When the
  canonicalizer hits `TypeData::TypeParameter(info)` it calls `find_param_index`
  on the `param_stack`; a hit becomes `interner.bound_parameter(index)`, so
  `{ value: T }` over `<T>` and `{ value: U }` over `<U>` both become
  `Object({ value: BoundParameter(0) })`.
- `Recursive(n)` — a back-reference N levels up. `canonicalize_type_alias` checks
  `get_recursion_depth(def_id)` against the `def_stack`; if the alias is already
  being expanded it emits `interner.recursive(depth)` instead of re-expanding, so
  `type A = { x: A }` becomes `Object({ x: Recursive(0) })`.

`Recursive` is produced **only** for `DefKind::TypeAlias` (structural). Nominal
references (Interface/Class/Enum) are *preserved* as `Lazy(DefId)` so their
nominal identity survives canonicalization — the canonicalizer returns the
`Lazy` unchanged in the non-`TypeAlias` arm.

The recursion is bounded by the canonicalizer's own `cache: FxHashMap<TypeId,
TypeId>` (memoizing input id -> canonical id) and a `RecursionGuard<TypeId>`
(`RecursionProfile::SubtypeCheck`). A `Cycle`/`DepthExceeded`/`IterationExceeded`
result returns the input id unchanged rather than diverging.

### Dropping cosmetic value-names from identity

The second job — the #13609 / #14096 family — is erasing identity-irrelevant
*names* that `Eq`/`Hash` would otherwise keep. tsc's `compareSignaturesIdentical`
and `compareTypeParametersIdentical` match parameters *positionally* and compare
*types* (and constraints) only; they never compare names. But `ParamInfo`,
`TypeParamInfo`, `TupleElement`, and `TypePredicate` all derive `Eq`/`Hash` over
their name fields, so keeping the names would fragment alpha-equivalent types into
distinct canonical ids and miss the relation's reflexive fast path. The
canonicalizer therefore drops them in the comparison-only form:

| Canonical helper | Drops |
|------------------|-------|
| `canonical_params` | parameter `name` (-> `None`); keeps type, `optional`, `rest`. |
| `canonical_type_param` | `default` (-> `None`) and `is_const` (-> `false`); canonicalizes `constraint`. |
| `canonical_bound_type_param` | additionally the declared `name` (-> `Atom::NONE`), since references are already positional. |
| `canonical_type_predicate` | the `Identifier(atom)` text (`parameter_index` anchors it); keeps `This`, `asserts`, `parameter_index`. |
| `canonicalize` (tuple arm) | the tuple element label `name` (-> `None`). |
| `canonicalize_index_signature` | the index `param_name` (-> `None`). |

Object property `name`s are *not* dropped — they are part of structural identity.
Likewise `optional`/`readonly`/`is_method`/`visibility`/`parent_id` survive, and
the `ObjectShape::symbol` nominal brand is preserved. The interned (non-canonical)
form keeps every name where it is actually rendered or used at instantiation;
only this comparison/hashing form erases them.

A subtle point flagged in the source: free type-parameter and `infer` references
also route through `canonical_type_param`, so a reference whose constraint was
captured in a still-`Lazy` pre-resolution form and one whose constraint was
captured already-expanded canonicalize to *one* identity (#13609). `NoInfer<T>` is
canonicalized as a single-nested wrapper (recurse the inner, re-wrap) rather than
falling into the passthrough arm, for the same reason.

### Walk-through: `type Pair<A, B> = { first: A; second: B }` vs a renamed twin

Tracing `Canonicalizer::canonicalize` on `Pair<...>` written with `<A, B>` versus
an identical alias written with `<X, Y>`:

1. `canonicalize(type_id)` — not intrinsic, cache miss, guard `Entered`.
2. `lookup` yields `TypeData::Lazy(DefId(Pair))`; `get_def_kind` is `TypeAlias`,
   so `canonicalize_type_alias(DefId(Pair))` runs.
3. `get_recursion_depth` misses (first visit) -> push `Pair` onto `def_stack`.
4. `get_lazy_type_params` returns `[A, B]` (or `[X, Y]`); the names are pushed as
   one scope onto `param_stack`.
5. `resolve_lazy(DefId(Pair))` returns the body shape `{ first: A; second: B }`;
   the recursive `canonicalize` hits the `Object` arm.
6. `canonicalize_object` walks the properties; each `first`/`second` value is a
   `TypeParameter`, so `find_param_index` rewrites `A`/`X` -> `BoundParameter(1)`
   and `B`/`Y` -> `BoundParameter(0)`. Property *names* `first`/`second` are
   preserved.
7. Both aliases produce `Object({ first: BoundParameter(1), second:
   BoundParameter(0) })` and intern to the **same** `TypeId`.

The relation engine's reflexive short-circuit then sees equal canonical ids and
answers "identical" without a structural walk.

## Caches and invariants

| Cache / state | Lives on | Key | Invalidation |
|---------------|----------|-----|--------------|
| TLS lookup cache (1024, direct-mapped) | thread-local | `TypeId.0` + `instance_id` | per-slot evict on collision; `instance_id` mismatch = miss; `clear_thread_local_cache` between sessions. |
| TLS intern cache (512, direct-mapped) | thread-local | `FxHash(TypeData)` + `instance_id` + full key compare | as above. |
| TLS string cache (256, inline keys <=23 bytes) | thread-local | `FxHash(str)` + `instance_id` + byte compare | as above; long strings bypass. |
| `key_to_index` / `index_to_key` | per `TypeInterner` shard | `TypeData` <-> local index | append-only; never evicted within a session. |
| `predicate_cache` (packed bits) | `TypeInterner` | `TypeId` + `PredicateCacheKind` bit | none — predicates are immutable per `TypeId` (structural). |
| `union_normalize_cache` | `TypeInterner` | flattened member list | bounded by `UNION_NORMALIZE_CACHE_MAX_LEN`; long inputs bypass so the TS2590 flag is never swallowed. |
| `widen_type_cache` | `TypeInterner` | root `TypeId` | none — widening is pure over immutable structure; only the canonical `widen_type` entry uses it. |
| `def_variance_masks` | `TypeInterner` | `DefId` -> (mask, gap defs) | replay validated on read: every gap def must still fail to resolve under the consumer's resolver. |
| `DefinitionStore` indices + `generation` | program-wide store | `DefId` / `TypeId` / `SymbolId` | `bump_generation` on any resolution-affecting mutation. |
| `TypeEnvironment.generation` | per env + shared store | env-local revision | bumped by every `register_*` / `set_*`; folded into relation/narrowing cache keys via `resolver_generation`. |
| `Canonicalizer.cache` | operation-local | input `TypeId` | dropped with the `Canonicalizer`. |

**Invariants worth stating:**

- `intern(lookup(id)) == id` for any interned `id`; `lookup(intern(data))` yields
  the same `data`. The bijection is the whole point.
- TLS caches never change answers — they only change *speed*. Every entry is
  re-validated by `instance_id` (and, for intern/string, a full key/byte
  compare), so a colliding or stale slot is a miss, never a wrong id. This is why
  `clear_thread_local_cache` is mandatory between independent compilation sessions
  on a reused thread (conformance runner, batch mode): without it a recycled
  `TypeId` from a new interner could read another interner's `TypeData`.
- Interner equality (`PartialEq` on shapes) is *identity*, never *subtyping*. The
  nominal `symbol` brand and `ObjectFlags` participate in identity precisely so
  the solver can compute relations explicitly over distinct ids.
- Canonical identity is comparison-only; the printer never reads the canonical
  form, and display-provenance maps (`display_alias`, `display_union_origin`,
  `display_properties`) live separately so cosmetic erasure never leaks into
  diagnostics.

## Limits, fuel, and the circuit breaker

The interner is also the home of two pathological-input guards:

- **Type-count circuit breaker.** `MAX_INTERNED_TYPES` (8,000,000 native; 500,000
  on `wasm32`, where the 32-bit heap cannot host a multi-GB interner) bounds how
  many distinct types may be minted. When `approximate_count()` (a single atomic
  load of `alloc_counter`) exceeds it, `intern_slow` calls
  `poison_due_to_interned_type_limit`, sets the `poisoned` flag, and returns
  `TypeId::ERROR` for *new* interning. Crucially, already-interned ids stay
  readable: `lookup` and existing-key `intern` are intentionally *not* gated on
  `poisoned`, so previously computed program types and the cross-file caches that
  hold their ids survive a limit event; only new type construction degrades to
  `ERROR`. A second breaker handles `u32` index overflow within a shard
  (`local_index > u32::MAX >> SHARD_BITS`).
- **Evaluation fuel.** The interner exposes `consume_evaluation_fuel` /
  `reset_evaluation_fuel` / `is_evaluation_fuel_exhausted`, backed by the
  consolidated `crate::limits` per-thread budget. Fuel is reset per top-level
  file-check (mirroring tsc resetting `instantiationCount` per checked element); a
  cumulative budget would starve the tail files of a large program into blanket
  `ERROR`. Exhaustion bails the current evaluation with `ERROR` but, like the
  count breaker, leaves the interner readable so already-computed types do not
  collapse to opaque `Type(N)` placeholders in later diagnostics.

`UNRESOLVED`-style template expansion has its own ceiling
(`TEMPLATE_LITERAL_EXPANSION_LIMIT`, 100k native / 2k wasm) to keep template
literal cross-products from OOMing.

## Edge cases and tsc parity

- **Boolean literals never duplicate.** `Literal(Boolean(true/false))` is mapped
  to the fixed `BOOLEAN_TRUE`/`BOOLEAN_FALSE` ids in `get_intrinsic_id`, so the
  literal `true` type and the constant are one id; `boolean` is their union.
- **`"100"` vs `100` as property keys.** `PropertyInfo::is_string_named` is
  identity-bearing, so a string-keyed `"100"` and a numeric-keyed `100` property
  do not intern to the same shape — matching tsc's distinct treatment of numeric
  string keys.
- **Quote style is invisible to identity.** `single_quoted_name` is exempt from
  `PropertyInfo` identity, so `{ 'a': T }` and `{ "a": T }` are one `TypeId`; the
  flag survives only for `.d.ts` emit fidelity.
- **Index-signature parameter name is display-only but shape-bearing.**
  `[k: string]` and `[key: string]` are the same type for assignability
  (`IndexSignature` excludes `param_name`), yet the interned `ObjectShape`
  re-adds it so diagnostics print the source name. Canonicalization drops it again
  for comparison.
- **Nominal classes never collapse.** Two empty classes `class A {}` / `class B
  {}` get distinct `ObjectShapeId`s via the `symbol` brand, so `A` is not
  assignable-by-identity to `B`; structural subtyping is then computed explicitly.
  Enums use `Enum(DefId, TypeId)` for the same reason — nominal `DefId` identity
  plus a structural member union for primitive relations.
- **Merged intersection vs plain object.** `{ a } & { b }` is merged into one
  `{ a; b }` object for O(1) member lookup but flagged `INTERSECTION_MERGED` so it
  stays a distinct `TypeId` from a hand-written `{ a; b }`, letting the relation
  layer recover that a target really was an intersection (TS2322 elaboration)
  without misfiring on the plain object.
- **Recursive aliases are structural; recursive interfaces are nominal.** `type A
  = { x: A }` canonicalizes to `Object({ x: Recursive(0) })` and is interchangeable
  with any structurally identical alias, while `interface I { x: I }` keeps its
  `Lazy(DefId(I))` and is compared nominally.
- **`OrderedFloat` determinism.** Numeric literal identity hashes on
  `f64::to_bits`, so `-0.0` and `0.0` are distinct literal types and `NaN` interns
  consistently rather than failing reflexive equality.

## Cross-references

- The relation engine that *uses* canonical identity and `Lazy` resolution:
  [solver-relations](solver-relations.md).
- Reducing `Conditional`/`Mapped`/`IndexAccess`/`Application` nodes to concrete
  shapes: [solver-evaluation](solver-evaluation.md).
- Substituting `BoundParameter`/`TypeParameter` during generic application:
  [solver-instantiation](solver-instantiation.md).
- Inferring type arguments into `Infer` binders: [solver-inference](solver-inference.md).
- The wider cache/object/contextual landscape:
  [solver-caches-objects-contextual-compat](solver-caches-objects-contextual-compat.md).
- How the checker stabilizes `DefId`s and registers definitions:
  [checker-context-and-state](checker-context-and-state.md),
  [checker-declarations-modules](checker-declarations-modules.md).
- How `Atom`s and `AstAtom`s are produced upstream:
  [front-end-scanner-parser](front-end-scanner-parser.md), [binder](binder.md).
