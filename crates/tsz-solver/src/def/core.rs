//! Definition identifiers and storage for the solver.
//!
//! This module provides a Solver-owned definition identifier (`DefId`) that
//! replaces `SymbolRef` in types, enabling:
//!
//! - **Decoupling**: Solver is independent of Binder's symbol representation
//! - **Testing**: Types can be created and tested without a full Binder
//! - **Caching**: `DefId` provides a stable key for Salsa memoization
//!
//! ## `DefId` Allocation Strategies
//!
//! | Mode | Strategy | Use Case |
//! |------|----------|----------|
//! | CLI  | Sequential allocation | Fresh start each compilation |
//! | LSP  | Content-addressed hash | Stable IDs across edits |
mod augmentation_symbols;
mod body_dependencies;
mod campaign_channels;
mod content_addressed;
mod cross_file_cache;
mod decl_identity;
mod definition_info;
mod observability;
mod semantic_construction;
mod state_flags;
mod symbol_registration;

pub(crate) use augmentation_symbols::module_augmentation_symbol_edge_enabled;
pub use content_addressed::ContentAddressedDefIds;
use cross_file_cache::CrossFileQueryCache;
use decl_identity::DeclSiteKey;
pub use observability::StoreStatistics;
use state_flags::DefStateFlags;

use super::publication_census;
use crate::construction::TypeDatabase;
#[cfg(test)]
use crate::types::ObjectFlags;
use crate::types::{ObjectShape, SymbolRef, TypeData, TypeId, TypeParamInfo};
use crate::utils::MutexExt;
use dashmap::{DashMap, DashSet};
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use tracing::trace;
use tsz_common::define_id;
use tsz_common::interner::Atom;

/// `TSZ_TYPEOF_URI_SELFLOOP=1` activates the program-wide value-space literal
/// substitution for a self-looping `typeof X` query in
/// `TypeEnvironment::resolve_type_query` (issue #14345). Default-OFF, so
/// flag-OFF is byte-parity with the historical per-arena-only behavior (the
/// self-looping `TypeQuery(symbol)` is returned unchanged and stays deferred).
///
/// When ON, a `typeof X` whose `SymbolRef`/`DefId` body resolves back to a
/// self-referential `TypeQuery(symbol)` (the fp-ts `const URI = "Array"; type
/// URI = typeof URI` higher-kinded-type tag idiom) is resolved to the concrete
/// value literal published in `DefinitionStore::typeof_value_to_literal`, when
/// one exists. Substitution is gated on a registered concrete literal, so an
/// abstract URI / literal-less `typeof` still self-loops/defers (sound). Both
/// the write-through (`register_typeof_value_literal_if_enabled`) and the read
/// (`typeof_self_loop_literal`) are gated on this flag.
pub(crate) fn typeof_uri_selfloop_enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("TSZ_TYPEOF_URI_SELFLOOP").is_ok_and(|v| v == "1"))
}

/// `TSZ_AUGMENTED_BODY_SYMBOL_REDIRECT=1` activates the home-symbol →
/// home-`DefId` redirect for index reduction against a frozen *pre-merge* empty
/// snapshot of a cross-file augmented interface (issue #14344 / #14345).
/// Default-OFF, so flag-OFF is byte-parity with the historical behavior (the
/// empty snapshot indexes to `undefined`).
///
/// When ON, both the producer write-through
/// (`register_augmented_base_body_def_if_enabled`, called at the augmentation
/// merge site) and the consumer read (the index-reduction redirect in
/// `evaluate_rules::index_access`) are active: a frozen empty `Object` carrying
/// `shape.symbol = <home interface symbol>` is re-indexed against the merged
/// body published under the home `DefId`, instead of falling to `undefined`.
/// The redirect is gated structurally on channel membership (no name/file
/// string), so flag-ON only affects symbols a checker actually published.
pub fn augmented_body_symbol_redirect_enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("TSZ_AUGMENTED_BODY_SYMBOL_REDIRECT").is_ok_and(|v| v == "1"))
}

/// Global counter for assigning unique instance IDs to `DefinitionStore` instances.
/// Used for debugging `DefId` collision issues.
static NEXT_INSTANCE_ID: AtomicU64 = AtomicU64::new(1);

/// Whether monotone lib-interface body publication (#13862) is active.
///
/// Default-on; `TSZ_DISABLE_LIB_DEF_MONOTONE=1` is the kill switch. When on, a
/// def whose finalized lib-interface body has been published rejects later
/// non-finalize different-body overwrites (heritage-thin re-derivations from
/// sibling fresh per-file checkers), so the shared store keeps the
/// heritage-complete form. See `set_body_with_params_impl`.
fn lib_def_monotone_publish_enabled() -> bool {
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| !std::env::var("TSZ_DISABLE_LIB_DEF_MONOTONE").is_ok_and(|v| v == "1"))
}

type CrossFileQueryCacheKey = (u8, u32, u32, u32, u64);
type CrossFileQueryCacheValue = (TypeId, Arc<Vec<TypeParamInfo>>);
type DefDashMap<K, V> = DashMap<K, V, FxBuildHasher>;
type DefDashSet<K> = DashSet<K, FxBuildHasher>;
type SymbolMappingsSnapshot = Arc<[(u32, DefId)]>;

/// Rough per-entry overhead for a `DashMap`/`DashSet` bucket (key + value +
/// shard bookkeeping), used by the store's `estimated_size_bytes` reporting.
/// Shared by the store core and its sub-store size estimators.
pub(super) const DASHMAP_ENTRY_OVERHEAD: usize = 64;

// =============================================================================
// DefId - Solver-Owned Definition Identifier
// =============================================================================

define_id! {
/// Solver-owned definition identifier.
///
/// Unlike `SymbolRef` which references Binder symbols, `DefId` is owned by
/// the Solver and can be created without Binder context.
///
/// ## Comparison with `SymbolRef`
///
/// | Aspect | SymbolRef | DefId |
/// |--------|-----------|-------|
/// | Owner | Binder | Solver |
/// | Stable across edits | No | Yes (with content-hash) |
/// | Requires Binder | Yes | No |
/// | Supports testing | Limited | Full |
pub struct DefId; }

impl DefId {
    /// Sentinel value for invalid `DefId`.
    pub const INVALID: Self = Self(0);

    /// First valid `DefId`.
    pub const FIRST_VALID: u32 = 1;

    /// Check if this `DefId` is valid.
    pub const fn is_valid(self) -> bool {
        self.0 >= Self::FIRST_VALID
    }
}

// =============================================================================
// DefKind - Definition Kind
// =============================================================================

/// Kind of type definition.
///
/// Affects evaluation and subtype checking behavior:
///
/// | Kind | Expansion | Nominal | Example |
/// |------|-----------|---------|---------|
/// | TypeAlias | Always expand | No | `type Foo = number` |
/// | Interface | Lazy expand | No | `interface Point { x: number }` |
/// | Class | Lazy expand | Yes (with brand) | `class Foo {}` |
/// | Enum | Special handling | Yes | `enum Color { Red, Green }` |
/// | Namespace | Export lookup | No | `namespace NS { export type T = number }` |
/// | Function | Value-space | No | `function foo(): void {}` |
/// | Variable | Value-space | No | `const x: number = 1` |
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum DefKind {
    /// Type alias: always expand (transparent).
    /// `type Foo<T> = T | null`
    TypeAlias,

    /// Interface: keep opaque until needed.
    /// `interface Point { x: number; y: number }`
    Interface,

    /// Class: opaque with nominal brand.
    /// `class User { constructor(public name: string) {} }`
    Class,

    /// Enum: special handling for member access.
    /// `enum Direction { Up, Down, Left, Right }`
    Enum,

    /// Namespace/Module: container for exported types and values.
    /// `namespace NS { export type T = number }`
    Namespace,

    /// Class constructor (static side): displayed as `typeof ClassName`.
    /// Distinguishes the constructor/static type from the instance type (`DefKind::Class`).
    ClassConstructor,

    /// Function declaration: value-space callable.
    /// `function foo(x: number): string { ... }`
    Function,

    /// Variable declaration: value-space binding.
    /// `const x: number = 42` or `let y = "hello"`
    Variable,
}

// =============================================================================
// Definition Info - Stored Definition Data
// =============================================================================

/// Complete information about a type definition.
///
/// This is stored in `DefinitionStore` and retrieved by `DefId`.
#[derive(Clone, Debug)]
pub struct DefinitionInfo {
    /// Kind of definition (affects evaluation strategy)
    pub kind: DefKind,

    /// Name of the definition (for diagnostics)
    pub name: Atom,

    /// Type parameters for generic definitions
    pub type_params: Vec<TypeParamInfo>,

    /// The body `TypeId` (structural representation)
    /// For lazy definitions, this may be computed on demand
    pub body: Option<TypeId>,

    /// For classes: the instance type's structural shape
    pub instance_shape: Option<Arc<ObjectShape>>,

    /// For classes: the static type's structural shape
    pub static_shape: Option<Arc<ObjectShape>>,

    /// For classes: parent class `DefId` (if extends)
    pub extends: Option<DefId>,

    /// For classes/interfaces: implemented interfaces
    pub implements: Vec<DefId>,

    /// For enums: member names and values
    pub enum_members: Vec<(Atom, EnumMemberValue)>,

    /// For namespaces/modules: exported members
    /// Maps export name to the `DefId` of the exported type
    pub exports: Vec<(Atom, DefId)>,

    /// Optional file identifier for debugging
    pub file_id: Option<u32>,

    /// Optional span for source location
    pub span: Option<(u32, u32)>,

    /// The binder `SymbolId` that this `DefId` was created from.
    /// Used for cross-context cycle detection: the same interface may get
    /// different `DefIds` in different checker contexts, but the `SymbolId`
    /// stays the same. This enables coinductive cycle detection for recursive
    /// generic interfaces (e.g., `Promise<T>` vs `PromiseLike<T>`).
    pub symbol_id: Option<u32>,

    /// Heritage clause names for cross-batch resolution.
    /// E.g., `class Foo extends Bar` stores `["Bar"]` so that
    /// `resolve_heritage` can look up the `Bar` DefId by name.
    pub heritage_names: Vec<String>,

    /// Whether this is an `abstract class` declaration.
    /// Propagated from binder `SemanticDefEntry` during pre-population.
    pub is_abstract: bool,

    /// Whether this is a `const enum` declaration.
    /// Propagated from binder `SemanticDefEntry` during pre-population.
    pub is_const: bool,

    /// Whether this declaration is exported.
    /// Propagated from binder `SemanticDefEntry` during pre-population.
    pub is_exported: bool,

    /// Whether this declaration is from a `declare global { }` block.
    /// Propagated from binder `SemanticDefEntry` during pre-population.
    /// Global augmentations merge with lib.d.ts symbols at type resolution time.
    pub is_global_augmentation: bool,

    /// Whether this declaration has the `declare` modifier or is in an ambient
    /// context (`.d.ts` file).
    ///
    /// Propagated from binder `SemanticDefEntry` during pre-population.
    /// Ambient declarations have no runtime representation; the checker uses
    /// this to suppress certain diagnostics (e.g., TS1183 requires ambient
    /// classes to have no body on methods) and to gate emit behaviour.
    pub is_declare: bool,
}

/// Enum member value.
#[derive(Clone, Debug, PartialEq)]
pub enum EnumMemberValue {
    /// Numeric enum member
    Number(f64),
    /// String enum member
    String(Atom),
    /// Computed (not yet evaluated)
    Computed,
}

// =============================================================================
// DefinitionStore - Storage for Definitions
// =============================================================================

/// Thread-safe storage for type definitions.
///
/// Uses `DashMap` for concurrent access from multiple checking threads.
///
/// ## Usage
///
/// ```text
/// let store = DefinitionStore::new();
///
/// // Register a type alias
/// let def_id = store.register(DefinitionInfo::type_alias(
///     interner.intern_string("Foo"),
///     vec![],
///     TypeId::NUMBER,
/// ));
///
/// // Look up later
/// let info = store.get(def_id).expect("definition exists");
/// ```
#[derive(Debug)]
pub struct DefinitionStore {
    /// Unique instance ID for debugging (tracks which store instance this is)
    instance_id: u64,

    /// `DefId` -> `DefinitionInfo` mapping
    definitions: DefDashMap<DefId, DefinitionInfo>,

    /// Next available `DefId`
    next_id: AtomicU32,

    /// Monotonic revision for resolver-visible definition-store mutations.
    generation: AtomicU64,

    /// Import-alias `DefId` -> declaring (target) `DefId` forwarding.
    ///
    /// Type annotations lower the *alias name*, so `Lazy`/`Application` bases
    /// in the importing file carry the alias's `DefId` while the declaring
    /// module's own references carry the target's. Both denote the same
    /// definition; relation logic (same-definition application families,
    /// variance fast paths) canonicalizes through this map so the two keys
    /// never degrade into a structural mismatch between an expanded shape
    /// and an opaque application.
    alias_forwards: DefDashMap<DefId, DefId>,

    /// Reverse map: `TypeId` -> `DefId` for named types.
    ///
    /// When a class/interface instance type is computed, the checker registers it here
    /// so the `TypeFormatter` can display the class/interface name instead of expanding
    /// the structural form (e.g., show "A" instead of "{ a: string }").
    type_to_def: DefDashMap<TypeId, DefId>,

    /// Body dependency graph captured as `DefId` edges by the publishing interner.
    body_dependency_defs: DefDashMap<DefId, Arc<[DefId]>>,

    /// #14351 lazy-reference relation: instantiated heritage edges.
    ///
    /// Maps a derived `DefId` to the list of `(parent DefId, instantiated base
    /// TypeId)` for each direct `extends` clause, where the base `TypeId` is the
    /// parent reference *as written in the derived type's own scope* (e.g.
    /// `Functor1<F>` from `interface Apply1<F> extends Functor1<F>`, lowered with
    /// `Apply1`'s type parameter `F`). Both the `InheritanceGraph` (SymbolId
    /// edges) and `DefinitionInfo::extends`/`implements` (`DefId` only) are
    /// type-argument-erased, so this is the only place the heritage edge's
    /// argument expression survives for the variance fast path to relate
    /// `Apply1<A>` to `Functor1<B>` without materializing members. Populated at
    /// lowering (`class_inheritance.rs`); read only by the flag-gated
    /// lazy-reference relation branch, so an empty/unread map is behavior-neutral.
    heritage_instantiations: DefDashMap<DefId, Vec<(DefId, TypeId)>>,

    /// Shared `(file, type-parameter name node, TypeParamInfo)` -> `TypeId`
    /// canonical identity map for type-parameter declarations that have no
    /// `DefId` registration (class, method, and interface type parameters
    /// are not emitted into `semantic_defs` by the binder).
    ///
    /// The file component is the interned file-name `Atom` of the arena
    /// owning the declaration, which makes the arena-local `NodeIndex`
    /// globally unambiguous. Because this store is shared across parent and
    /// child checkers (cross-arena delegation), every checker that pushes
    /// the same declaration converges on one `TypeId`; a per-checker cache
    /// would let child checkers mint their own fresh ids for the same
    /// declaration and defeat identity-based relation fast paths
    /// (`ExpressionBuilder<DB, TB>` vs itself, #13044).
    type_param_for_decl_node: DefDashMap<(Atom, u32, TypeParamInfo), TypeId>,

    /// Authoritative `(SymbolId, file_idx)` -> `DefId` index.
    ///
    /// This replaces the per-context `symbol_to_def` cache as the single source of
    /// truth for SymbolId→DefId mappings. The composite key `(symbol_id, file_idx)`
    /// naturally disambiguates the same raw `SymbolId(u32)` across different binders
    /// (each binder has a unique `file_idx`), eliminating the need for expensive
    /// post-hoc name/file validation on every cache hit.
    ///
    /// The per-context `symbol_to_def` map is retained as a thin local cache for
    /// backward compatibility and to avoid `DashMap` overhead on repeated lookups
    /// within the same context.
    symbol_def_index: DefDashMap<(u32, u32), DefId>,

    /// Reverse index: `SymbolId` (raw u32) -> `DefId` (file-agnostic).
    ///
    /// Unlike `symbol_def_index` which uses the composite `(symbol_id, file_idx)` key,
    /// this index is keyed by `symbol_id` alone. It maps to the *first* `DefId`
    /// registered for that symbol. This serves the `TypeFormatter` use case where
    /// only a `SymbolRef` (raw u32) is available and we need *any* matching `DefId`
    /// to look up the definition name and type parameters.
    ///
    /// Replaces the O(N) linear scan in the previous `find_def_by_symbol`.
    symbol_only_index: DefDashMap<u32, DefId>,

    decl_site_to_def: DefDashMap<DeclSiteKey, DefId>,

    /// Generation-keyed immutable snapshot of `symbol_only_index`.
    ///
    /// Project checking warms many per-file checker contexts from the same shared
    /// store. Caching this snapshot avoids collecting the same `DashMap` into a
    /// fresh `Vec` for every checker while preserving generation-based invalidation.
    symbol_mappings_snapshot: Mutex<Option<(u64, SymbolMappingsSnapshot)>>,

    /// Append-only insertion log mirroring `symbol_only_index`.
    ///
    /// `symbol_only_index` is first-wins (`entry().or_insert()`), so the
    /// sequence of successful inserts reproduces the map's content exactly.
    /// `all_symbol_mappings_snapshot` snapshots this log with one `memcpy`
    /// instead of re-iterating the whole `DashMap` every time the store
    /// generation changes (which happens between every checked file).
    symbol_mappings_log: Mutex<Vec<(u32, DefId)>>,

    /// Length-keyed snapshot of `symbol_mappings_log`.
    symbol_mappings_log_snapshot: Mutex<Option<(usize, SymbolMappingsSnapshot)>>,

    /// Set when `symbol_only_index` diverges from the insert log (entry
    /// removal or `clear`). Once set, `all_symbol_mappings_snapshot` falls
    /// back to the legacy generation-keyed `DashMap` collection permanently
    /// for this store.
    symbol_mappings_log_invalid: std::sync::atomic::AtomicBool,

    /// Reverse index: body `TypeId` -> `DefId` for non-generic type aliases.
    ///
    /// Populated by `set_body` when the definition is a `TypeAlias` with no type
    /// parameters. Enables O(1) lookup in `find_type_alias_by_body`, replacing an
    /// O(N) linear scan over all definitions. This is used by the `TypeFormatter`
    /// and error reporters to display alias names (e.g., "Color") instead of
    /// structural expansions (e.g., "{ r: number; g: number; b: number }").
    body_to_alias: DefDashMap<TypeId, DefId>,

    /// Cross-checker per-definition flag sets (poison / circular /
    /// publish-isolation / alias-body). See [`DefStateFlags`] for the
    /// per-set invalidation contract.
    state_flags: DefStateFlags,

    /// Reverse index: `file_id` -> `Vec<DefId>` for per-file definition lookups.
    ///
    /// Populated during `register()` when the `DefinitionInfo` has a `file_id`.
    /// Enables O(1) lookup of all definitions originating from a given file,
    /// which is the foundation for incremental invalidation: when a file changes,
    /// we can instantly find all `DefId`s that need to be refreshed without
    /// scanning the entire definition store.
    file_to_defs: DefDashMap<u32, Vec<DefId>>,

    /// Reverse index: `ObjectShape` hash -> `DefId` for shape-based lookups.
    ///
    /// Populated when `instance_shape` is set (via `register()` or
    /// `set_instance_shape()`). Enables O(1) lookup in `find_def_by_shape`,
    /// replacing an O(N) linear scan over all definitions. Used by the
    /// `TypeFormatter` to display interface/class names instead of structural
    /// expansions in diagnostic messages.
    ///
    /// Keyed by a 64-bit `FxHash` of the `ObjectShape`. Hash collisions are
    /// theoretically possible but astronomically unlikely with `FxHash`, and the
    /// formatter use case is best-effort diagnostic naming.
    shape_to_def: DefDashMap<u64, DefId>,

    /// Reverse index: Class `DefId` -> `ClassConstructor` `DefId`.
    ///
    /// Populated during pre-population when a `DefKind::Class` definition is
    /// registered alongside its companion `DefKind::ClassConstructor` identity.
    /// Enables O(1) lookup of the constructor companion for a class, so the
    /// checker can reuse the pre-populated identity instead of creating a new
    /// `DefId` on demand during type checking.
    class_to_constructor: DefDashMap<DefId, DefId>,

    /// Program-wide enum member `DefId` -> parent enum `DefId` map.
    ///
    /// `TypeEnvironment::enum_parents` is reset per file (it lives in the
    /// file-local evaluator/flow-analyzer env). Cross-file enum discriminant
    /// narrowing reads the member→parent edge through the *consuming* file's
    /// flow-analyzer env, by which point the producing file's local
    /// registration has been wiped. This shared map survives the per-file
    /// reset (it lives on the program-wide `DefinitionStore`), so
    /// `TypeEnvironment::get_enum_parent` can fall back to it and resolve the
    /// nominal `E.B <: E` relation at narrowing time regardless of which file
    /// declared the enum. Populated by write-through from
    /// `TypeEnvironment::register_enum_parent`.
    enum_member_to_parent: DefDashMap<DefId, DefId>,

    /// Shared cross-file instance-type cache for class `DefId`s.
    ///
    /// A class has two `TypeId`s: the constructor (value side, written into
    /// `body` for `typeof C` / value-position lookups via
    /// `TypeEnvironment::insert_def`) and the instance (type side, returned
    /// for `Lazy(class_def_id)` in type position).
    /// `TypeEnvironment::class_instance_types` holds the instance type
    /// *per checker*, which is unsuitable for cross-file resolution: a
    /// consuming checker's local cache is empty for classes declared in
    /// another file, and falling back to `body` returns the constructor.
    /// This map provides a shared instance-type slot that any checker can
    /// consult, populated by the producer's
    /// `TypeEnvironment::insert_class_instance_type` write-through. Only
    /// type-position resolvers (`resolve_lazy`) read this slot; value-
    /// position resolvers (`resolve_type_query`) still go through the
    /// SymbolRef/body path and continue to return the constructor.
    class_to_instance: DefDashMap<DefId, TypeId>,

    /// Program-wide value-space literal for a merged value+type symbol's
    /// `typeof X` query, keyed by raw `SymbolRef.0` (issue #14345).
    ///
    /// The fp-ts higher-kinded-type tag idiom (`const URI = "Array"; type URI =
    /// typeof URI`) produces a self-referential type-space alias body: resolving
    /// `typeof URI` through the `DefId`/`SymbolRef` body re-yields
    /// `TypeQuery(URI)` and self-loops, so `URItoKind[URI]` never reduces to
    /// `Kind<"Array", A>`. The checker computes the genuine value-space literal
    /// (`"Array"`) per-arena into `TypeEnvironment::typeof_value_types`, but that
    /// registration is gated on a syntactic `typeof <symbol>` node reached while
    /// checking THIS file; a cross-arena `Kind<URI, A>` body never registers it
    /// in the consuming arena, so the per-arena map misses. This shared slot is
    /// the program-wide publish of that literal: producer checkers write-through
    /// here from `TypeEnvironment::insert_typeof_value_type`, and any arena's
    /// `resolve_type_query` consults it as the sound substitute for the
    /// self-loop. Only populated with a concrete value literal (the producer
    /// rejects unknown/error/any), so an abstract URI / literal-less `typeof`
    /// stays deferred.
    typeof_value_to_literal: DefDashMap<u32, TypeId>,
    /// Empty registry `DefId` -> merged module-augmentation body and source files.
    module_augmented_bodies: DefDashMap<DefId, (TypeId, Vec<u32>)>,

    /// Program-wide redirect from a HOME interface `SymbolId` (raw u32) to the
    /// HOME `DefId` whose `get_body` holds its fully-merged augmented body
    /// (issue #14344 / #14345).
    ///
    /// The fp-ts higher-kinded-type registry idiom publishes a per-module
    /// augmented `interface URItoKindN { [URI]: Kind<...> }` whose merged body
    /// (all augmentation blocks folded together) is materialized under the home
    /// interface's own `DefId`. A frozen *pre-merge* snapshot of that interface
    /// — a bare `Object(ObjectShapeId)` with EMPTY properties and NO `DefId`,
    /// but still carrying `shape.symbol = <home interface symbol>` — can reach
    /// the index-reduction consumer (`URItoKind<A>[URI]`). That consumer holds
    /// only the home symbol, and the file-agnostic `symbol_only_index` was never
    /// written for the home symbol (the home def was registered without a
    /// `symbol_id`, and `set_body_with_params_impl` publishes the merged body
    /// without touching any symbol index), so `find_def_by_symbol(home_symbol)`
    /// misses and the consumer falls to `undefined`.
    ///
    /// This dedicated slot is the producer-published edge: the augmentation
    /// merge site records `home_symbol -> home_def_id` once the merged body is
    /// assembled, so the consumer can map its frozen `shape.symbol` back to the
    /// populated home def and re-index that body for the URI literal key.
    /// First-wins (the home def's identity is stable). Keyed and consulted
    /// structurally on the raw `SymbolId`; no name/file-string drives it.
    augmented_base_body_def_for_symbol: DefDashMap<u32, DefId>,

    /// Reverse index: `Atom` (name) -> `Vec<DefId>` for name-based lookups.
    ///
    /// Populated during `register()` for every definition. Enables O(1) lookup
    /// of all definitions sharing a given name, which is the foundation for
    /// cross-batch heritage resolution: when a user class says
    /// `class Foo extends Array`, the name "Array" can be looked up to find the
    /// lib definition's `DefId` without knowing its file or symbol ID.
    ///
    /// Multiple definitions may share the same name (e.g., interface merging,
    /// or same-named types in different files), so the value is a `Vec<DefId>`.
    name_to_defs: DefDashMap<Atom, Vec<DefId>>,

    /// Cross-file checker query memo, its scope stamp, and per-file delegation
    /// locks. See [`CrossFileQueryCache`].
    cross_file_cache: CrossFileQueryCache,

    /// Flag indicating that cross-batch heritage resolution and DefId population
    /// have already been completed. When `true`, `apply_to` skips the expensive
    /// `pre_populate_def_ids_from_all_binders()` and `resolve_cross_batch_heritage()`
    /// calls. Set by `mark_fully_populated()` after the first complete population pass.
    ///
    /// This prevents O(files * `total_defs`) work when checking many files in parallel,
    /// which was the root cause of hangs on large type libraries like ts-toolbelt.
    fully_populated: std::sync::atomic::AtomicBool,
}

impl Default for DefinitionStore {
    fn default() -> Self {
        Self::new()
    }
}

impl DefinitionStore {
    /// `decl_file_idx` carried by `tsz_binder` symbols that have no program
    /// declaration file — every lib-binder (ambient) symbol. See
    /// [`Self::def_is_non_program`].
    pub const NON_PROGRAM_FILE_SENTINEL: u32 = u32::MAX;

    /// Create a new definition store.
    pub fn new() -> Self {
        Self::with_capacities(0, 0)
    }

    /// Create a new definition store with estimated capacities for the hot indices.
    pub fn with_capacities(definition_capacity: usize, file_count: usize) -> Self {
        let instance_id = NEXT_INSTANCE_ID.fetch_add(1, Ordering::SeqCst);
        trace!(instance_id, "DefinitionStore::new - creating new instance");
        let id_capacity = definition_capacity.max(16);
        let file_capacity = file_count.max(4);
        Self {
            instance_id,
            definitions: DefDashMap::with_capacity_and_hasher(id_capacity, Default::default()),
            next_id: AtomicU32::new(DefId::FIRST_VALID),
            generation: AtomicU64::new(1),
            alias_forwards: DefDashMap::default(),
            type_to_def: DefDashMap::default(),
            body_dependency_defs: DefDashMap::default(),
            heritage_instantiations: DefDashMap::default(),
            type_param_for_decl_node: DefDashMap::default(),
            symbol_def_index: DefDashMap::with_capacity_and_hasher(id_capacity, Default::default()),
            symbol_only_index: DefDashMap::with_capacity_and_hasher(
                id_capacity,
                Default::default(),
            ),
            decl_site_to_def: DefDashMap::with_capacity_and_hasher(id_capacity, FxBuildHasher),
            symbol_mappings_snapshot: Mutex::new(None),
            symbol_mappings_log: Mutex::new(Vec::new()),
            symbol_mappings_log_snapshot: Mutex::new(None),
            symbol_mappings_log_invalid: std::sync::atomic::AtomicBool::new(false),
            body_to_alias: DefDashMap::default(),
            state_flags: DefStateFlags::default(),
            shape_to_def: DefDashMap::default(),
            file_to_defs: DefDashMap::with_capacity_and_hasher(file_capacity, Default::default()),
            class_to_constructor: DefDashMap::with_capacity_and_hasher(
                id_capacity / 2,
                Default::default(),
            ),
            class_to_instance: DefDashMap::with_capacity_and_hasher(
                id_capacity / 2,
                Default::default(),
            ),
            typeof_value_to_literal: DefDashMap::default(),
            module_augmented_bodies: DefDashMap::default(),
            augmented_base_body_def_for_symbol: DefDashMap::default(),
            enum_member_to_parent: DefDashMap::default(),
            name_to_defs: DefDashMap::with_capacity_and_hasher(id_capacity, Default::default()),
            cross_file_cache: CrossFileQueryCache::default(),
            fully_populated: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Compute a 64-bit `FxHash` fingerprint for an `ObjectShape`.
    fn hash_shape(shape: &ObjectShape) -> u64 {
        let mut hasher = rustc_hash::FxHasher::default();
        shape.hash(&mut hasher);
        hasher.finish()
    }

    /// Allocate a fresh `DefId`.
    fn allocate(&self) -> DefId {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        trace!(
            instance_id = self.instance_id,
            allocated_def_id = %id,
            next_will_be = %(id + 1),
            "DefinitionStore::allocate"
        );
        DefId(id)
    }

    fn bump_generation(&self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
    }

    /// Current resolver-visible generation for this store.
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }

    /// First-wins insert into `symbol_only_index` that mirrors successful
    /// inserts into the append-only `symbol_mappings_log`.
    fn insert_symbol_only_mapping(&self, symbol_id: u32, def_id: DefId) {
        use dashmap::mapref::entry::Entry;
        match self.symbol_only_index.entry(symbol_id) {
            Entry::Vacant(vacant) => {
                vacant.insert(def_id);
                self.symbol_mappings_log
                    .lock_unpoisoned("def.symbol_mappings_log")
                    .push((symbol_id, def_id));
            }
            Entry::Occupied(_) => {}
        }
    }

    /// Mark the symbol-mappings insert log as diverged from
    /// `symbol_only_index` (an entry was removed or the index was cleared).
    fn invalidate_symbol_mappings_log(&self) {
        self.symbol_mappings_log_invalid
            .store(true, Ordering::Relaxed);
    }

    /// Register a new definition and return its `DefId`.
    pub fn register(&self, info: DefinitionInfo) -> DefId {
        let id = self.allocate();
        trace!(
            instance_id = self.instance_id,
            def_id = %id.0,
            kind = ?info.kind,
            "DefinitionStore::register"
        );

        // Populate symbol_only_index if a symbol_id is present.
        // Uses entry API to keep the *first* registered DefId (stable identity).
        if let Some(sym_id) = info.symbol_id {
            self.insert_symbol_only_mapping(sym_id, id);
        }

        // Populate body_to_alias for non-generic type aliases with a body.
        if info.kind == DefKind::TypeAlias
            && info.type_params.is_empty()
            && let Some(body) = info.body
        {
            self.body_to_alias.entry(body).or_insert(id);
        }

        // Populate shape_to_def for definitions with an instance shape.
        if let Some(ref shape) = info.instance_shape {
            let hash = Self::hash_shape(shape);
            self.shape_to_def.entry(hash).or_insert(id);
        }

        // Populate file_to_defs index for per-file lookups.
        if let Some(file_id) = info.file_id {
            self.file_to_defs.entry(file_id).or_default().push(id);
        }

        // Populate name_to_defs index for name-based lookups.
        self.name_to_defs.entry(info.name).or_default().push(id);

        self.register_decl_site_identity(id, &info);
        self.definitions.insert(id, info);
        self.bump_generation();
        id
    }

    /// Register a `(SymbolId, file_idx)` → `DefId` mapping in the authoritative index.
    ///
    /// This should be called whenever a new `DefId` is created from a binder symbol,
    /// using the symbol's raw id and its `decl_file_idx`. The composite key ensures
    /// that the same `SymbolId(u32)` from different binders maps to different `DefIds`.
    pub fn register_symbol_mapping(&self, symbol_id: u32, file_idx: u32, def_id: DefId) {
        // Re-registering the identical mapping changes nothing a reader can
        // observe (the file-agnostic index keeps the first DefId anyway), so
        // skip the generation bump for it.
        if self.lookup_by_symbol(symbol_id, file_idx) == Some(def_id) {
            return;
        }
        self.register_symbol_file_mapping(symbol_id, file_idx, def_id);
        // Also maintain the file-agnostic index (keeps the first registered DefId).
        self.insert_symbol_only_mapping(symbol_id, def_id);
        self.bump_generation();
    }

    fn register_symbol_file_mapping(&self, symbol_id: u32, file_idx: u32, def_id: DefId) {
        self.symbol_def_index.insert((symbol_id, file_idx), def_id);
    }

    /// Look up a `DefId` by `(SymbolId, file_idx)`.
    ///
    /// Returns `Some(def_id)` if a mapping was previously registered via
    /// `register_symbol_mapping`. This is an O(1) lookup that replaces the
    /// expensive multi-binder validation in `get_or_create_def_id`.
    pub fn lookup_by_symbol(&self, symbol_id: u32, file_idx: u32) -> Option<DefId> {
        let result = self
            .symbol_def_index
            .get(&(symbol_id, file_idx))
            .map(|r| *r);
        // #14344 denominator context (measurement only — `result` is returned
        // unchanged): partitions composite-key `(symbol, file)` resolution
        // attempts into hits/misses so the wrong-decl collision count has a
        // population to normalize against.
        tsz_common::perf_counters::record_symbol_def_index_lookup(result.is_some());
        result
    }

    /// Get definition info by `DefId`.
    pub fn get(&self, id: DefId) -> Option<DefinitionInfo> {
        self.definitions.get(&id).as_deref().cloned()
    }

    /// Snapshot all definition name paths for consumers that need stable display names.
    pub fn all_definition_names(&self) -> Vec<(DefId, Vec<Atom>)> {
        let definitions: FxHashMap<_, _> = self
            .definitions
            .iter()
            .map(|entry| (*entry.key(), entry.value().clone()))
            .collect();
        let mut parents = FxHashMap::default();
        for (parent_id, parent) in &definitions {
            for &(export_name, child_id) in &parent.exports {
                parents.entry(child_id).or_insert((*parent_id, export_name));
            }
        }
        definitions
            .iter()
            .map(|(&def_id, info)| {
                let mut path = vec![info.name];
                let mut current = def_id;
                let mut seen = FxHashSet::default();
                while seen.insert(current) {
                    let Some(&(parent_id, export_name)) = parents.get(&current) else {
                        break;
                    };
                    path[0] = export_name;
                    path.insert(
                        0,
                        definitions.get(&parent_id).map_or(export_name, |p| p.name),
                    );
                    current = parent_id;
                }
                (def_id, path)
            })
            .collect()
    }

    /// Get the binder SymbolId for a `DefId`.
    ///
    /// Returns the `SymbolId` (as raw u32) that this `DefId` was created from.
    /// This is available across checker contexts because it's stored directly
    /// in the `DefinitionInfo` (which is shared via `DefinitionStore`).
    pub fn get_symbol_id(&self, id: DefId) -> Option<u32> {
        self.definitions.get(&id).and_then(|info| info.symbol_id)
    }

    /// Check if a `DefId` exists.
    pub fn contains(&self, id: DefId) -> bool {
        self.definitions.contains_key(&id)
    }

    /// Get the kind of a definition.
    pub fn get_kind(&self, id: DefId) -> Option<DefKind> {
        self.definitions.get(&id).map(|r| r.kind)
    }

    /// Get the `Copy` classification fields of a definition — `(file_id,
    /// kind, is_declare)` — in one lookup, without cloning the whole
    /// `DefinitionInfo` (whose heap fields make [`Self::get`] expensive for
    /// gate checks that only classify the def).
    pub fn get_classification(&self, id: DefId) -> Option<(Option<u32>, DefKind, bool)> {
        self.definitions
            .get(&id)
            .map(|r| (r.file_id, r.kind, r.is_declare))
    }

    /// Update the body `TypeId` for a definition (for lazy evaluation).
    ///
    /// If no entry exists for this `DefId` (e.g., it was created by
    /// `get_or_create_def_id` without a full `register` call), a minimal
    /// entry is created so that cross-file type resolution can find the
    /// body via `get_body`.
    #[track_caller]
    pub fn set_body(&self, id: DefId, body: TypeId) {
        self.set_body_with_params(id, body, None);
    }

    /// Publish a definition body — and optionally its type parameters —
    /// atomically under the entry lock.
    ///
    /// The shared `DefinitionStore` is read concurrently by sibling parallel
    /// file checkers while each fresh checker re-derives bodies for the defs
    /// it touches. Body and type parameters must be written under one entry
    /// guard: publishing the body first and the parameter list second (two
    /// separate `get_mut` windows) let a concurrent reader observe a generic
    /// alias whose body was visible but whose `type_params` were still
    /// empty/stale, mis-instantiating every application of the alias
    /// (false `TS2344` storms under parallel fresh checking; sequential
    /// checking never interleaves a reader between the two writes).
    #[track_caller]
    pub fn set_body_with_params(
        &self,
        id: DefId,
        body: TypeId,
        params: Option<Vec<TypeParamInfo>>,
    ) {
        self.set_body_with_params_impl(id, body, params, false);
    }

    /// Publish a definition body through the **finalize entry point**.
    ///
    /// Identical to [`Self::set_body_with_params`] except that it bypasses
    /// the deferred-publication drop (`deferred_publish_defs`): the finalized
    /// lib-body form must overwrite whatever earlier form the store carries.
    /// Frozen defs (`publish_once_defs`) still win — once a def's finalized
    /// body is frozen, later checkers' re-finalizations (checker-relative
    /// `TypeId`s for the byte-identical semantic form) are dropped.
    #[track_caller]
    pub fn set_body_finalized(&self, id: DefId, body: TypeId, params: Option<Vec<TypeParamInfo>>) {
        self.set_body_with_params_impl(id, body, params, true);
    }

    #[track_caller]
    fn set_body_with_params_impl(
        &self,
        id: DefId,
        body: TypeId,
        params: Option<Vec<TypeParamInfo>>,
        finalize: bool,
    ) {
        if let Some(mut entry) = self.definitions.get_mut(&id) {
            // Mutation-isolation: defs frozen after their finalized
            // materialization keep that body; a later attempt to overwrite it
            // with a *different* body form is dropped (the shared store stays
            // immutable for that def). Compute this under the same entry guard
            // that owns the write: a pre-lock body probe can race with a
            // sibling checker publication and turn a stale "no body yet"
            // observation into a different-body overwrite.
            let is_different_overwrite = entry.body.is_some_and(|prev| prev != body);
            let suppressed = is_different_overwrite && self.state_flags.is_publish_once_frozen(id);
            // Deferred-publication experiment: pre-finalize different-body
            // overwrites of marked defs are dropped; only the finalize entry
            // point may replace the first published form.
            let deferred = !suppressed
                && !finalize
                && is_different_overwrite
                && self.state_flags.is_deferred_publish(id);

            // Mutation-isolation campaign census (env-gated, see
            // `publication_census`): classify this publication against the
            // guarded pre-write entry state with caller attribution.
            if publication_census::census_enabled() {
                publication_census::record_existing_publication(
                    id,
                    &entry,
                    body,
                    params.as_deref(),
                    suppressed,
                    deferred,
                    std::panic::Location::caller(),
                );
            }
            if suppressed || deferred {
                return;
            }

            // Monotone lib-interface publication (#13862): the program-shared
            // `DefinitionStore` is read by sibling fresh per-file checkers via
            // `Lazy(DefId)` resolution. Without isolation it is last-writer-wins,
            // so a heritage-thin body re-derived by a cross-arena lowering path
            // (`resolver.rs` `insert_def_with_params`, the cross-file delegation
            // helpers) can clobber the heritage-merged body another checker
            // already finalized — the DOM `Node`/`Element`/`HTMLElement` diamond
            // (#12299) then oscillates between forms and a reader's relation sees
            // the thin one (false TS2345/TS2740/TS2322 where a derived element
            // interface is not recognized as its transitive base). Once a
            // *finalized* interface body is published (only
            // `register_finalized_lib_body` reaches the finalize entry point, and
            // only on heritage-complete resolution — see `lib_resolution`), mark
            // the def deferred so later non-finalize different-body overwrites are
            // dropped. Marking at the finalize point (rather than blanket
            // up-front, the opt-in `mark_non_program_interface_defs_deferred`
            // path) keeps the load-bearing pre-finalize forms for augmented names
            // intact — finalize re-publications (which carry the augmentation)
            // still win. Kill switch: `TSZ_DISABLE_LIB_DEF_MONOTONE=1`.
            if finalize && entry.kind == DefKind::Interface && lib_def_monotone_publish_enabled() {
                self.state_flags.mark_deferred_publish(id);
            }

            // Identical republication is a no-op: nothing a reader can
            // observe changes, so consumers keyed on `generation()` must not
            // see a bump for it.
            if entry.body == Some(body)
                && params
                    .as_ref()
                    .is_none_or(|params| &entry.type_params == params)
            {
                return;
            }
            let old_decl_site_key = params
                .as_ref()
                .and_then(|_| Self::decl_site_key_for_info(&entry));
            if let Some(params) = params {
                if entry.kind == DefKind::TypeAlias
                    && entry.type_params.is_empty()
                    && !params.is_empty()
                    && let Some(prev_body) = entry.body
                {
                    self.body_to_alias.remove(&prev_body);
                }
                entry.type_params = params;
            }
            entry.body = Some(body);
            if old_decl_site_key.is_some() {
                self.refresh_decl_site_identity(id, old_decl_site_key, &entry);
            }

            // Maintain body_to_alias index for non-generic type aliases.
            if entry.kind == DefKind::TypeAlias && entry.type_params.is_empty() {
                self.body_to_alias.entry(body).or_insert(id);
            }
            self.bump_generation();
        } else {
            if publication_census::census_enabled() {
                publication_census::record_minted_minimal_publication(
                    id,
                    body,
                    std::panic::Location::caller(),
                );
            }
            // Create a minimal entry for DefIds created via get_or_create_def_id
            // (which only populates symbol_to_def/def_to_symbol, not definitions).
            // This ensures cross-file delegation results survive child-checker
            // teardown and are visible to parent checkers via get_body().
            self.definitions.insert(
                id,
                DefinitionInfo {
                    kind: DefKind::Interface,
                    name: Atom::default(),
                    type_params: params.unwrap_or_default(),
                    body: Some(body),
                    instance_shape: None,
                    static_shape: None,
                    extends: None,
                    implements: Vec::new(),
                    enum_members: Vec::new(),
                    exports: Vec::new(),
                    file_id: None,
                    span: None,
                    symbol_id: self.get_symbol_id(id),
                    heritage_names: Vec::new(),
                    is_abstract: false,
                    is_const: false,
                    is_exported: false,
                    is_global_augmentation: false,
                    is_declare: false,
                },
            );
            self.bump_generation();
        }
    }

    /// Mutation-isolation campaign experiment
    /// (`TSZ_EXPERIMENT_LIB_DEF_DEFER_PUBLISH`): mark every **interface**
    /// definition that does not originate from a program source file (lib
    /// binder symbols carry the binder's "no declaration file" sentinel
    /// index) as deferred-publication. Pre-finalize different-body
    /// overwrites of such defs are dropped; only
    /// [`Self::set_body_finalized`] replaces the first published form (and
    /// the checker freezes the def right after). Per-file checkers continue
    /// to use their own `TypeEnvironment` bodies for in-flight refinement.
    ///
    /// Returns the number of definitions marked. Driver-invoked only when
    /// the experiment is enabled.
    pub fn mark_non_program_interface_defs_deferred(&self) -> usize {
        /// `tsz_binder` symbols without a program declaration file (every
        /// lib-binder symbol) carry `u32::MAX` as `decl_file_idx`.
        const NON_PROGRAM_FILE_SENTINEL: u32 = u32::MAX;
        let mut marked = 0usize;
        for entry in &self.definitions {
            let info = entry.value();
            if info.kind == DefKind::Interface && info.file_id == Some(NON_PROGRAM_FILE_SENTINEL) {
                self.state_flags.mark_deferred_publish(*entry.key());
                marked += 1;
            }
        }
        marked
    }

    /// Mutation-isolation campaign: freeze a single def's shared-store body
    /// **after** its current (finalized) publication, so the form just
    /// published becomes the immutable one. Later different-body
    /// publications are dropped.
    pub fn mark_publish_once(&self, id: DefId) {
        self.state_flags.mark_publish_once(id);
    }

    /// Record that `alias` is an import alias of `target` (see
    /// `alias_forwards`). No-op for self-forwards; bumps the generation only
    /// when the link is new or changed.
    pub fn set_alias_forward(&self, alias: DefId, target: DefId) {
        if alias == target || !alias.is_valid() || !target.is_valid() {
            return;
        }
        // Refuse links that would create a forwarding cycle.
        if self.canonical_def_id(target) == alias {
            return;
        }
        let prev = self.alias_forwards.insert(alias, target);
        if prev != Some(target) {
            self.bump_generation();
        }
    }

    /// Resolve a `DefId` through the import-alias forwarding chain to the
    /// declaring definition. Identity for non-alias defs. The chase is
    /// depth-bounded so a (refused, but defensively handled) cycle cannot
    /// loop.
    pub fn canonical_def_id(&self, def_id: DefId) -> DefId {
        let mut current = def_id;
        for _ in 0..8 {
            match self.alias_forwards.get(&current) {
                Some(next) if *next != current => current = *next,
                _ => break,
            }
        }
        current
    }

    /// Mark a type-alias `DefId` as having an unconditionally-infinite
    /// instantiation (TS2589). Every later application of this def resolves to
    /// the error type.
    pub fn mark_depth_poisoned(&self, id: DefId) {
        if self.state_flags.mark_depth_poisoned(id) {
            self.bump_generation();
        }
    }

    /// Whether the given `DefId` was flagged via [`mark_depth_poisoned`].
    pub fn is_depth_poisoned(&self, id: DefId) -> bool {
        self.state_flags.is_depth_poisoned(id)
    }

    /// Whether any def has been flagged via [`mark_depth_poisoned`]. Used as a
    /// cheap guard so hot evaluation paths skip per-application poison checks
    /// when nothing is poisoned (the overwhelmingly common case).
    pub fn has_any_depth_poisoned(&self) -> bool {
        self.state_flags.has_any_depth_poisoned()
    }

    /// Update the type parameters for a definition.
    ///
    /// Type parameters may be computed lazily after initial registration.
    /// Initialize per-file delegation locks for parallel checking.
    /// Mark the store as fully populated (all `DefIds` registered, heritage resolved).
    ///
    /// After this is called, `is_fully_populated()` returns `true`, allowing
    /// callers to skip redundant population passes.
    pub fn mark_fully_populated(&self) {
        self.fully_populated
            .store(true, std::sync::atomic::Ordering::Release);
    }

    /// Check if the store has been marked as fully populated.
    pub fn is_fully_populated(&self) -> bool {
        self.fully_populated
            .load(std::sync::atomic::Ordering::Acquire)
    }

    pub fn init_file_locks(&self, file_count: usize) {
        self.cross_file_cache.init_file_locks(file_count);
    }

    /// Get the delegation lock for a target file.
    pub fn get_file_delegation_lock(&self, file_idx: usize) -> Option<Arc<Mutex<()>>> {
        self.cross_file_cache.file_delegation_lock(file_idx)
    }

    pub fn source_file_symbol_type_cache_scope(&self) -> u64 {
        self.cross_file_cache.scope()
    }

    pub fn set_source_file_symbol_type_cache_scope(&self, scope: u64) {
        self.cross_file_cache.set_scope(scope);
    }

    /// Look up a previously resolved cross-file query result.
    ///
    /// Returns the shared `Arc` over the cached type-params so per-hit reads
    /// are O(1) (no `Vec<TypeParamInfo>` deep clone). Callers that need an
    /// owned `Vec` clone at their own boundary.
    pub fn get_resolved_cross_file_query(
        &self,
        kind: u8,
        file_idx: u32,
        primary: u32,
        secondary: u32,
        args_hash: u64,
    ) -> Option<(TypeId, Arc<Vec<TypeParamInfo>>)> {
        self.cross_file_cache
            .get(kind, file_idx, primary, secondary, args_hash)
    }

    /// Cache a cross-file query result. First writer wins to keep parallel
    /// checking deterministic when equivalent queries race.
    pub fn cache_resolved_cross_file_query(
        &self,
        kind: u8,
        file_idx: u32,
        primary: u32,
        secondary: u32,
        args_hash: u64,
        type_id: TypeId,
        type_params: Vec<TypeParamInfo>,
    ) {
        self.cross_file_cache.insert(
            kind,
            file_idx,
            primary,
            secondary,
            args_hash,
            type_id,
            type_params,
        );
    }

    /// Mark a DefId as participating in a circular type alias cycle.
    pub fn mark_circular_def(&self, def_id: DefId) {
        self.state_flags.mark_circular(def_id);
    }

    /// Check whether a DefId has been marked as circular by any checker.
    pub fn is_circular_def(&self, def_id: DefId) -> bool {
        self.state_flags.is_circular(def_id)
    }

    /// This method synchronizes them into the `DefinitionInfo` so that
    /// the `TypeFormatter` can display generic types with their type
    /// parameter names (e.g., `MyClass<T>` instead of just `MyClass`).
    pub fn set_type_params(&self, id: DefId, params: Vec<TypeParamInfo>) {
        if let Some(mut entry) = self.definitions.get_mut(&id) {
            let old_decl_site_key = Self::decl_site_key_for_info(&entry);
            // If this is a TypeAlias that previously had empty type_params,
            // set_body may have created a body_to_alias entry. Now that we
            // know it's generic, remove that entry to avoid incorrect alias
            // lookups (e.g., showing "B" instead of "B<string>").
            if entry.kind == DefKind::TypeAlias
                && entry.type_params.is_empty()
                && !params.is_empty()
                && let Some(body) = entry.body
            {
                self.body_to_alias.remove(&body);
            }
            entry.type_params = params;
            self.refresh_decl_site_identity(id, old_decl_site_key, &entry);
            self.bump_generation();
        }
    }

    /// Update heritage links (extends/implements) only for non-empty values.
    ///
    /// Called by the checker's `resolve_cross_batch_heritage` after all
    /// pre-population batches complete, when heritage targets from other
    /// batches become available in the name index.
    pub fn set_heritage_if_nonempty(
        &self,
        id: DefId,
        extends: Option<DefId>,
        implements: Vec<DefId>,
    ) {
        if let Some(mut entry) = self.definitions.get_mut(&id) {
            if extends.is_some() {
                entry.extends = extends;
            }
            if !implements.is_empty() {
                entry.implements = implements;
            }
            self.bump_generation();
        }
    }

    /// Update the instance shape for a type definition.
    ///
    /// This is used by checker code when a concrete object-like shape is computed
    /// for an interface/class definition and should be recorded for diagnostics.
    pub fn set_instance_shape(&self, id: DefId, shape: Arc<ObjectShape>) {
        if let Some(mut entry) = self.definitions.get_mut(&id) {
            let hash = Self::hash_shape(&shape);
            entry.instance_shape = Some(shape);
            self.shape_to_def.entry(hash).or_insert(id);
            self.bump_generation();
        }
    }

    /// Number of definitions.
    pub fn len(&self) -> usize {
        self.definitions.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.definitions.is_empty()
    }

    /// Clear all definitions (for testing).
    pub fn clear(&self) {
        self.definitions.clear();
        self.type_to_def.clear();
        self.type_param_for_decl_node.clear();
        self.symbol_def_index.clear();
        self.symbol_only_index.clear();
        self.decl_site_to_def.clear();
        self.invalidate_symbol_mappings_log();
        self.body_to_alias.clear();
        self.state_flags.clear_alias_bodies();
        self.shape_to_def.clear();
        self.file_to_defs.clear();
        self.class_to_constructor.clear();
        self.class_to_instance.clear();
        self.typeof_value_to_literal.clear();
        self.module_augmented_bodies.clear();
        self.augmented_base_body_def_for_symbol.clear();
        self.enum_member_to_parent.clear();
        self.name_to_defs.clear();
        self.next_id.store(DefId::FIRST_VALID, Ordering::SeqCst);
        self.bump_generation();
    }

    /// Register a mapping from a `TypeId` to its defining `DefId`.
    ///
    /// Called by the checker after computing class/interface instance types
    /// so the `TypeFormatter` can display named types (e.g., "A" instead of
    /// "{ a: string }") even across file boundaries.
    pub fn register_type_to_def(&self, type_id: TypeId, def_id: DefId) {
        // Intrinsic TypeIds (number, string, boolean, etc.) are universal and
        // must never be associated with a user-named def. Their canonical
        // display is the keyword (`number`, `string`, ...), provided by the
        // TypeFormatter's intrinsic short-circuit. If a checker path tries to
        // register an intrinsic type to a class/interface/alias def, that
        // mapping would later poison `find_def_for_type` lookups and cause
        // diagnostics like "Type 'FlatArray' is not assignable to type
        // 'Boolean'." for `let b: Boolean; b = 1;` (where the source is the
        // primitive `number`).  Drop the registration so the formatter falls
        // back to the intrinsic keyword.
        if type_id.is_intrinsic() {
            return;
        }
        use dashmap::mapref::entry::Entry;
        match self.type_to_def.entry(type_id) {
            Entry::Vacant(e) => {
                e.insert(def_id);
            }
            Entry::Occupied(mut e) => {
                let existing = *e.get();
                if existing == def_id {
                    return;
                }
                let existing_pos = self
                    .get(existing)
                    .and_then(|d| Some((d.file_id?, d.span?.0)));
                let new_pos = self.get(def_id).and_then(|d| Some((d.file_id?, d.span?.0)));
                match (existing_pos, new_pos) {
                    (Some((ef, ep)), Some((nf, np))) if (nf, np) < (ef, ep) => {
                        e.insert(def_id);
                    }
                    (None, Some(_)) => {
                        e.insert(def_id);
                    }
                    _ => {}
                }
            }
        }
        self.bump_generation();
    }

    /// Look up the `DefId` that produced the given `TypeId`.
    ///
    /// Returns `Some(def_id)` if a class/interface was registered for this type.
    pub fn find_def_for_type(&self, type_id: TypeId) -> Option<DefId> {
        self.type_to_def.get(&type_id).map(|r| *r)
    }

    /// Look up the canonical `TypeId` for a type-parameter declaration
    /// identified by its owning file and name-node index (declarations
    /// without a `DefId` registration).
    ///
    /// `file` is the interned file-name `Atom` of the arena owning
    /// `name_node`; together they form a globally unambiguous declaration
    /// identity that is stable across parent and child checkers.
    pub fn find_type_param_for_decl_node(
        &self,
        file: Atom,
        name_node: u32,
        info: &TypeParamInfo,
    ) -> Option<TypeId> {
        self.type_param_for_decl_node
            .get(&(file, name_node, *info))
            .map(|r| *r)
    }

    /// #14351 measurement-only: number of distinct decl-site canonical type-param
    /// entries `(file, name_node, info)`. Comparing this to the count of distinct
    /// interned `TypeParameter` ids decides whether the divergent ids are
    /// fresh-mints that BYPASS the decl-site map (map-len << distinct-ids =>
    /// tractable re-mint convergence) or genuinely-distinct decl-sites
    /// (map-len ~= distinct-ids => XL relation-level).
    pub fn type_param_decl_node_count(&self) -> usize {
        self.type_param_for_decl_node.len()
    }

    /// Register the canonical `TypeId` for a type-parameter declaration
    /// identified by `(file, name_node, info)`.
    ///
    /// First writer wins: the returned `TypeId` is the canonical one, which
    /// may differ from `type_id` when another checker registered the same
    /// declaration first (parallel file checking or cross-arena
    /// delegation). Callers must adopt the returned id.
    pub fn register_type_param_for_decl_node(
        &self,
        file: Atom,
        name_node: u32,
        info: TypeParamInfo,
        type_id: TypeId,
    ) -> TypeId {
        *self
            .type_param_for_decl_node
            .entry((file, name_node, info))
            .or_insert(type_id)
    }

    /// Register a mapping from a `Class` `DefId` to its `ClassConstructor` companion `DefId`.
    ///
    /// Called during pre-population to establish constructor identity at merge time
    /// rather than on-demand during type checking. The checker can then look up the
    /// companion with `get_constructor_def` and reuse the stable identity.
    pub fn register_constructor_companion(&self, class_def: DefId, ctor_def: DefId) {
        self.class_to_constructor.insert(class_def, ctor_def);
        self.bump_generation();
    }

    /// Look up the pre-populated `ClassConstructor` `DefId` for a class.
    ///
    /// Returns `Some(ctor_def_id)` if a constructor companion was registered
    /// during pre-population. Returns `None` for classes without a pre-populated
    /// companion (e.g., anonymous classes or those created on-demand).
    pub fn get_constructor_def(&self, class_def: DefId) -> Option<DefId> {
        self.class_to_constructor.get(&class_def).map(|r| *r)
    }

    /// Publish the resolved instance `TypeId` for a class `DefId` into the
    /// shared cross-file cache (see `class_to_instance` field doc).
    ///
    /// Producer checkers call this whenever they finalize a class's instance
    /// type. Consumer checkers in other files read it through
    /// `get_class_instance_type` when their per-checker
    /// `TypeEnvironment::class_instance_types` is cold.
    pub fn register_class_instance_type(&self, class_def: DefId, instance_type: TypeId) {
        self.class_to_instance.insert(class_def, instance_type);
        self.bump_generation();
    }

    /// Look up the shared instance `TypeId` for a class `DefId`.
    ///
    /// Returns `Some(instance_type)` if some checker has already finalized
    /// the class's instance type and published it via
    /// `register_class_instance_type`. Returns `None` when no checker has
    /// finished building the class yet.
    pub fn get_class_instance_type(&self, class_def: DefId) -> Option<TypeId> {
        self.class_to_instance.get(&class_def).map(|r| *r)
    }

    /// Publish a merged value+type symbol's value-space `typeof` literal into
    /// the program-wide slot (see the `typeof_value_to_literal` field doc).
    ///
    /// Write-through target for `TypeEnvironment::insert_typeof_value_type`. The
    /// producing arena's local `typeof_value_types` map only carries entries for
    /// symbols whose `typeof` query was syntactically reached while checking that
    /// file; this shared copy lets a consuming arena's `resolve_type_query`
    /// resolve a cross-arena self-looping `typeof X` to the concrete literal.
    pub fn register_typeof_value_literal(&self, symbol_id: u32, literal: TypeId) {
        if self
            .typeof_value_to_literal
            .insert(symbol_id, literal)
            .is_none_or(|prev| prev != literal)
        {
            self.bump_generation();
        }
    }

    /// Look up the program-wide value-space `typeof` literal for a merged
    /// value+type symbol, if some checker has published one.
    pub fn get_typeof_value_literal(&self, symbol_id: u32) -> Option<TypeId> {
        self.typeof_value_to_literal.get(&symbol_id).map(|r| *r)
    }

    /// Record the redirect from a HOME interface `SymbolId` (raw u32) to the
    /// HOME `DefId` whose `get_body` holds its fully-merged augmented body
    /// (see the `augmented_base_body_def_for_symbol` field doc).
    ///
    /// First-wins (the home def's identity is stable across re-checks), so a
    /// later re-publication of the same edge is a no-op and does not perturb the
    /// store generation. Write-through target for the checker's augmentation
    /// merge site.
    pub fn register_augmented_base_body_def(&self, symbol_id: u32, def_id: DefId) {
        use dashmap::mapref::entry::Entry;
        if let Entry::Vacant(vacant) = self.augmented_base_body_def_for_symbol.entry(symbol_id) {
            vacant.insert(def_id);
            self.bump_generation();
        }
    }

    /// Look up the HOME `DefId` whose merged augmented body should answer an
    /// index access against a frozen empty pre-merge snapshot carrying this home
    /// `SymbolId`, if some checker has published the edge.
    pub fn augmented_base_body_def_for_symbol(&self, symbol_id: u32) -> Option<DefId> {
        self.augmented_base_body_def_for_symbol
            .get(&symbol_id)
            .map(|r| *r)
    }

    /// Flag-gated write-through of a merged value+type symbol's value literal
    /// into the program-wide slot (issue #14345). Called from
    /// `TypeEnvironment::insert_typeof_value_type`; the producer only passes a
    /// concrete value type (unknown/error/any are rejected upstream in the
    /// checker), so the shared slot stays sound. Gated on
    /// `typeof_uri_selfloop_enabled()` so flag-OFF leaves the shared store (and
    /// its generation) untouched — fully byte-parity.
    pub fn register_typeof_value_literal_if_enabled(&self, symbol_id: u32, literal: TypeId) {
        if typeof_uri_selfloop_enabled() {
            self.register_typeof_value_literal(symbol_id, literal);
        }
    }

    /// Flag-gated write-through of the home-symbol → home-`DefId` redirect edge
    /// (issue #14344 / #14345). Called from the checker's augmentation merge
    /// site once the merged augmented body is published under the home `DefId`.
    /// Gated on `augmented_body_symbol_redirect_enabled()` so flag-OFF leaves
    /// the shared store (and its generation) untouched — fully byte-parity.
    pub fn register_augmented_base_body_def_if_enabled(&self, symbol_id: u32, def_id: DefId) {
        if augmented_body_symbol_redirect_enabled() {
            self.register_augmented_base_body_def(symbol_id, def_id);
        }
    }

    /// Resolve a self-looping `typeof X` query to its program-wide value literal
    /// (issue #14345). A merged value+type symbol whose type-space body is a
    /// self-referential `typeof X` (the fp-ts `const URI = "..."; type URI =
    /// typeof URI` tag idiom) self-loops in `resolve_type_query` — `candidate`
    /// re-yields `TypeQuery(symbol)`, so `URItoKind[URI]` never reduces. When the
    /// consuming arena never registered the value literal locally (its `typeof X`
    /// site lives in another arena), substitute the program-wide literal
    /// published by the producing arena. Returns `None` (leaving `candidate`
    /// unchanged) unless the flag is on, `candidate` is exactly the self-loop
    /// `TypeQuery(symbol)`, and a concrete literal was published — so an abstract
    /// URI / literal-less `typeof` stays deferred. Flag-gated; OFF is byte-parity.
    pub fn typeof_self_loop_literal(
        &self,
        symbol: SymbolRef,
        candidate: Option<TypeId>,
        interner: &dyn TypeDatabase,
    ) -> Option<TypeId> {
        if !typeof_uri_selfloop_enabled() {
            return None;
        }
        let is_self_loop = candidate.is_some_and(
            |ty| matches!(interner.lookup(ty), Some(TypeData::TypeQuery(s)) if s == symbol),
        );
        if is_self_loop {
            self.get_typeof_value_literal(symbol.0)
        } else {
            None
        }
    }

    /// Publish an enum member `DefId` -> parent enum `DefId` edge into the
    /// shared, program-wide map (see the `enum_member_to_parent` field doc).
    ///
    /// Write-through target for `TypeEnvironment::register_enum_parent`. The
    /// producing file's local `enum_parents` map is wiped on its file-session
    /// reset; this shared copy persists so a consuming file's flow-analyzer env
    /// can still resolve the member's parent during cross-file enum
    /// discriminant narrowing.
    pub fn register_enum_parent(&self, member_def: DefId, parent_def: DefId) {
        if self
            .enum_member_to_parent
            .insert(member_def, parent_def)
            .is_none()
        {
            self.bump_generation();
        }
    }

    /// Look up the parent enum `DefId` for an enum member `DefId` in the shared
    /// program-wide map. Returns `None` when no checker has registered the edge
    /// (e.g. a non-enum-member `DefId`).
    pub fn get_enum_parent(&self, member_def: DefId) -> Option<DefId> {
        self.enum_member_to_parent.get(&member_def).map(|r| *r)
    }

    /// Get exports for a namespace/module `DefId`.
    pub fn get_exports(&self, id: DefId) -> Option<Vec<(Atom, DefId)>> {
        self.definitions.get(&id).map(|r| r.exports.clone())
    }

    /// Get the name of a definition.
    pub fn get_name(&self, id: DefId) -> Option<Atom> {
        self.definitions.get(&id).map(|r| r.name)
    }

    /// The declaring file id of a definition, if known.
    ///
    /// Reads the single field directly rather than cloning the whole
    /// `DefinitionInfo` (as `get` does), matching `get_kind`/`get_name`. Lib
    /// definitions use the `u32::MAX` sentinel.
    pub fn get_file_id(&self, id: DefId) -> Option<u32> {
        self.definitions.get(&id).and_then(|r| r.file_id)
    }

    /// Whether `id` is a non-program (lib/ambient-binder) definition.
    ///
    /// `tsz_binder` symbols without a program declaration file — every
    /// lib-binder symbol — carry [`Self::NON_PROGRAM_FILE_SENTINEL`]
    /// (`u32::MAX`) as their `decl_file_idx`. This is the structural witness
    /// that a def originates in a `lib.*.d.ts` (or other ambient) file rather
    /// than user program source, independent of its name.
    pub fn def_is_non_program(&self, id: DefId) -> bool {
        self.get_file_id(id) == Some(Self::NON_PROGRAM_FILE_SENTINEL)
    }

    /// Add an export to an existing definition.
    pub fn add_export(&self, id: DefId, name: Atom, export_def: DefId) {
        if let Some(mut entry) = self.definitions.get_mut(&id) {
            entry.add_export(name, export_def);
            self.bump_generation();
        }
    }

    /// Set the `extends` (parent class/interface) for an existing definition.
    ///
    /// Used by heritage resolution at pre-populate time to wire class/interface
    /// hierarchy from binder-owned stable identity rather than checker repair.
    pub fn set_extends(&self, id: DefId, extends: DefId) {
        if let Some(mut entry) = self.definitions.get_mut(&id) {
            entry.extends = Some(extends);
            self.bump_generation();
        }
    }

    /// Set the `implements` list for an existing definition.
    ///
    /// Used by heritage resolution at pre-populate time to wire interface
    /// implementations from binder-owned stable identity.
    pub fn set_implements(&self, id: DefId, implements: Vec<DefId>) {
        if let Some(mut entry) = self.definitions.get_mut(&id) {
            entry.implements = implements;
            self.bump_generation();
        }
    }

    /// Find a `DefId` by its instance shape.
    ///
    /// This is used by the `TypeFormatter` to preserve interface names in error messages.
    /// When an Object type matches an interface's instance shape, we use the interface name
    /// instead of expanding the object literal.
    ///
    /// O(1) via `shape_to_def` index. The index is populated by both `register()`
    /// (when `DefinitionInfo::instance_shape` is set) and `set_instance_shape()`,
    /// covering all registration paths.
    pub fn find_def_by_shape(&self, shape: &ObjectShape) -> Option<DefId> {
        let hash = Self::hash_shape(shape);
        self.shape_to_def.get(&hash).map(|r| *r)
    }

    /// Find a `DefId` by its associated `SymbolId` (raw u32).
    ///
    /// Used by the `TypeFormatter` to look up whether a symbol corresponds to a
    /// generic definition, enabling display of type parameters in error messages
    /// (e.g., `S18<unknown, unknown, unknown>` instead of just `S18`).
    ///
    /// O(1) via `symbol_only_index`. The index is populated by both `register()`
    /// (when `DefinitionInfo::symbol_id` is set) and `register_symbol_mapping()`,
    /// covering all registration paths.
    pub fn find_def_by_symbol(&self, symbol_id: u32) -> Option<DefId> {
        self.symbol_only_index.get(&symbol_id).map(|r| *r)
    }

    /// Return all `(raw_symbol_id, DefId)` pairs from the symbol-only index.
    ///
    /// This enables the checker to warm its local `symbol_to_def` / `def_to_symbol`
    /// caches in a single pass from the shared `DefinitionStore`, avoiding the need
    /// to iterate each binder's `semantic_defs` separately. The returned pairs are
    /// collected into a `Vec` to avoid holding `DashMap` read locks across the
    /// caller's mutation of its own maps.
    pub fn all_symbol_mappings(&self) -> Vec<(u32, DefId)> {
        self.all_symbol_mappings_snapshot().to_vec()
    }

    /// Return a generation-keyed immutable snapshot of all `(raw_symbol_id, DefId)`
    /// pairs from the symbol-only index.
    ///
    /// The snapshot is rebuilt only when the store generation changes. If a writer
    /// mutates the store while we are collecting, we retry so the cached generation
    /// cannot point at a partially stale snapshot.
    pub fn all_symbol_mappings_snapshot(&self) -> SymbolMappingsSnapshot {
        // Fast path: while `symbol_only_index` has only ever grown through
        // first-wins inserts, the append-only log reproduces its content
        // exactly and a length-keyed snapshot is one `memcpy` instead of a
        // full `DashMap` iteration. The store generation changes between
        // every checked file, so the legacy generation-keyed cache below
        // misses on essentially every per-file warm.
        if !self.symbol_mappings_log_invalid.load(Ordering::Relaxed) {
            let log = self
                .symbol_mappings_log
                .lock_unpoisoned("def.symbol_mappings_log");
            let mut cached = self
                .symbol_mappings_log_snapshot
                .lock_unpoisoned("def.symbol_mappings_log_snapshot");
            if let Some((cached_len, snapshot)) = cached.as_ref()
                && *cached_len == log.len()
            {
                return Arc::clone(snapshot);
            }
            let snapshot: SymbolMappingsSnapshot = log.as_slice().into();
            *cached = Some((log.len(), Arc::clone(&snapshot)));
            return snapshot;
        }

        loop {
            let generation_before = self.generation();
            if let Some(snapshot) = self.cached_symbol_mappings_snapshot(generation_before) {
                return snapshot;
            }

            let mut mappings: Vec<_> = self
                .symbol_only_index
                .iter()
                .map(|entry| (*entry.key(), *entry.value()))
                .collect();
            mappings.sort_by_key(|&(symbol_id, def_id)| (symbol_id, def_id.0));
            let mappings: SymbolMappingsSnapshot = mappings.into();

            let generation_after = self.generation();
            if generation_before != generation_after {
                continue;
            }

            let mut cached = self
                .symbol_mappings_snapshot
                .lock_unpoisoned("def.symbol_mappings_snapshot");
            if let Some((cached_generation, snapshot)) = cached.as_ref()
                && *cached_generation == generation_after
            {
                return Arc::clone(snapshot);
            }

            *cached = Some((generation_after, Arc::clone(&mappings)));
            return mappings;
        }
    }

    fn cached_symbol_mappings_snapshot(&self, generation: u64) -> Option<SymbolMappingsSnapshot> {
        let cached = self
            .symbol_mappings_snapshot
            .lock_unpoisoned("def.symbol_mappings_snapshot");
        cached.as_ref().and_then(|(cached_generation, snapshot)| {
            (*cached_generation == generation).then(|| Arc::clone(snapshot))
        })
    }

    /// Find a type alias `DefId` whose body matches the given `TypeId`.
    ///
    /// This preserves type alias names in diagnostic messages: when the formatter
    /// encounters an Object/Union/etc. TypeId that is the body of a type alias,
    /// it can display the alias name (e.g., "Color") instead of the expansion
    /// (e.g., "{ r: number; g: number; b: number }").
    ///
    /// Only matches non-generic type aliases (no type parameters) to avoid
    /// ambiguity with instantiated generics.
    ///
    /// O(1) via `body_to_alias` index. The index is populated by both `register()`
    /// (for aliases created with a body) and `set_body()` (for lazily-evaluated aliases),
    /// covering all registration paths.
    pub fn find_type_alias_by_body(&self, type_id: TypeId) -> Option<DefId> {
        // Skip bodies that were marked as "computed" (produced by intersection
        // reduction, conditional evaluation, etc.). tsc does not preserve alias
        // names for such types. A shape that is also the constructive body of a
        // directly-written alias keeps its name ("direct wins"), so it is not
        // skipped here.
        if self.is_computed_body(type_id) {
            return None;
        }
        self.body_to_alias.get(&type_id).map(|r| *r)
    }

    /// Mark a body `TypeId` as "computed" so that `find_type_alias_by_body`
    /// skips it. Called by the checker when a type alias body is produced by
    /// intersection reduction or conditional evaluation.
    pub fn mark_body_as_computed(&self, body: TypeId) {
        self.state_flags.mark_body_computed(body);
        self.bump_generation();
    }

    /// Record `body` as the constructive body of a non-computed type alias, so
    /// it keeps its alias name even if a computed alias resolves to the same
    /// interned shape ("direct wins"). See [`Self::directly_named_alias_bodies`].
    pub fn mark_body_as_directly_named(&self, body: TypeId) {
        self.state_flags.mark_body_directly_named(body);
        self.bump_generation();
    }

    /// Check if a body `TypeId` should be displayed structurally because it was
    /// produced by a reducing operator (conditional/indexed-access/intersection)
    /// that carries no `aliasSymbol` in tsc. A shape that is also the body of a
    /// directly-written alias is excluded ("direct wins"): that alias must keep
    /// its name, and because tsz interns structurally-identical types to one
    /// `TypeId`, the shared shape cannot be reported as computed.
    pub fn is_computed_body(&self, body: TypeId) -> bool {
        self.state_flags.is_computed_body(body)
    }

    /// Mark a non-generic type alias whose declared tuple body was produced by
    /// flattening a fixed-tuple spread (`type T = [...[a, b], c]`). `tsc` does
    /// not stamp the resulting spread tuple with an `aliasSymbol`, so its
    /// diagnostics render the structural form (`[a, b, c]`) rather than `T`.
    /// Keyed per def — the flattened tuple shares its interned `TypeId` with a
    /// directly-written `type T = [a, b, c]`, which `tsc` displays by name.
    pub fn mark_tuple_spread_flattened_alias(&self, def_id: DefId) {
        self.state_flags.mark_tuple_spread_flattened_alias(def_id);
        self.bump_generation();
    }

    /// Whether `def_id` is a non-generic alias whose tuple body was
    /// spread-flattened (see [`Self::mark_tuple_spread_flattened_alias`]).
    pub fn is_tuple_spread_flattened_alias(&self, def_id: DefId) -> bool {
        self.state_flags.is_tuple_spread_flattened_alias(def_id)
    }

    /// Record a non-generic type alias whose declared body is a *bare*
    /// (argument-free) type reference resolving to the non-generic
    /// interface/class declaration `target`. `tsc` attaches no `aliasSymbol`
    /// to the declaration's shared nominal type, so diagnostics render the
    /// declaration's own name (`type IA = Iface` renders `Iface`). Keyed per
    /// alias def because the resolved body may flatten to the declaration's
    /// structural shape, which no longer records which reference produced it.
    pub fn record_bare_nominal_ref_alias(&self, alias_def: DefId, target_def: DefId) {
        self.state_flags
            .record_bare_nominal_ref_alias(alias_def, target_def);
        self.bump_generation();
    }

    /// The interface/class declaration recorded for `alias_def` via
    /// [`Self::record_bare_nominal_ref_alias`], if any.
    pub fn bare_nominal_ref_alias_target(&self, alias_def: DefId) -> Option<DefId> {
        self.state_flags.bare_nominal_ref_alias_target(alias_def)
    }

    /// Find all `DefId`s registered under the given name.
    pub fn find_defs_by_name(&self, name: Atom) -> Option<Vec<DefId>> {
        self.name_to_defs.get(&name).map(|r| r.clone())
    }

    pub fn all_type_alias_defs(&self) -> Vec<DefId> {
        self.definitions
            .iter()
            .filter_map(|entry| (entry.value().kind == DefKind::TypeAlias).then_some(*entry.key()))
            .collect()
    }

    /// Resolve heritage names to `DefId`s using an intern function for
    /// name comparison.
    ///
    /// For each name in the definition's `heritage_names`, interns the name
    /// string via `intern_fn`, looks up the `name_to_defs` index, and returns
    /// the first matching `DefId` of kind `Class` or `Interface`.
    ///
    /// This enables cross-batch heritage resolution: when a user class says
    /// `class Foo extends Array`, the lib definition for `Array` can be found
    /// by name after all batches are registered.
    ///
    /// Returns a list of `(heritage_name, resolved_def_id)` pairs.
    /// Unresolved names are silently skipped.
    pub fn resolve_heritage(
        &self,
        id: DefId,
        intern_fn: &dyn Fn(&str) -> Atom,
    ) -> Vec<(String, DefId)> {
        let heritage_names = match self.definitions.get(&id) {
            Some(info) if !info.heritage_names.is_empty() => info.heritage_names.clone(),
            _ => return Vec::new(),
        };

        let mut resolved = Vec::with_capacity(heritage_names.len());
        for name_str in &heritage_names {
            let name_atom = intern_fn(name_str);
            if let Some(candidates) = self.name_to_defs.get(&name_atom) {
                // Find the first Class or Interface that isn't self.
                for &candidate_id in candidates.value() {
                    if candidate_id == id {
                        continue;
                    }
                    if let Some(candidate_info) = self.definitions.get(&candidate_id)
                        && matches!(candidate_info.kind, DefKind::Class | DefKind::Interface)
                    {
                        resolved.push((name_str.clone(), candidate_id));
                        break;
                    }
                }
            }
        }

        resolved
    }

    /// Get all `DefId`s originating from the given file.
    ///
    /// Returns a clone of the `Vec<DefId>` for the file, or an empty `Vec` if
    /// no definitions were registered with that `file_id`. This is an O(1)
    /// lookup via the `file_to_defs` index.
    ///
    /// Used for incremental invalidation: when a file changes, the caller can
    /// find all `DefId`s that need to be refreshed.
    pub fn defs_by_file(&self, file_id: u32) -> Vec<DefId> {
        self.file_to_defs
            .get(&file_id)
            .map(|r| r.clone())
            .unwrap_or_default()
    }

    /// Check whether the store has any definitions registered for the given file.
    ///
    /// O(1) lookup via the `file_to_defs` index.
    pub fn has_file(&self, file_id: u32) -> bool {
        self.file_to_defs.contains_key(&file_id)
    }

    /// Invalidate all definitions originating from the given file.
    ///
    /// Removes each `DefId` from the main definition store and all reverse
    /// indices (`type_to_def`, `symbol_def_index`, `symbol_only_index`,
    /// `body_to_alias`, `shape_to_def`). The `file_to_defs` entry itself is
    /// also removed.
    ///
    /// After invalidation, the `DefId` values are "dangling" — any remaining
    /// references to them (e.g., in `TypeData::Lazy(DefId)`) will fail to
    /// resolve, which is the intended behavior for incremental re-checking:
    /// the caller must re-bind and re-register the changed file's definitions.
    ///
    /// Returns the number of definitions invalidated.
    pub fn invalidate_file(&self, file_id: u32) -> usize {
        self.invalidate_module_augmented_bodies_for_file(file_id);
        let def_ids = match self.file_to_defs.remove(&file_id) {
            Some((_, ids)) => ids,
            None => return 0,
        };

        let count = def_ids.len();
        for def_id in &def_ids {
            // Remove from the main store and capture the info for index cleanup.
            if let Some((_, info)) = self.definitions.remove(def_id) {
                self.remove_decl_site_identity_if_points_to(*def_id, &info);
                // Clean up symbol indices.
                if let Some(sym_id) = info.symbol_id {
                    if let Some(fid) = info.file_id {
                        self.symbol_def_index.remove(&(sym_id, fid));
                    }
                    // Only remove from symbol_only_index if it points to this DefId.
                    if let Some(entry) = self.symbol_only_index.get(&sym_id)
                        && *entry == *def_id
                    {
                        drop(entry);
                        self.symbol_only_index.remove(&sym_id);
                        self.invalidate_symbol_mappings_log();
                    }
                }

                self.module_augmented_bodies.remove(def_id);

                // Clean up type_to_def (reverse scan is expensive, but invalidation
                // is rare and bounded by per-file definition count).
                self.type_to_def.retain(|_, v| *v != *def_id);

                // Clean up body_to_alias.
                if info.kind == DefKind::TypeAlias
                    && info.type_params.is_empty()
                    && let Some(body) = info.body
                    && let Some(entry) = self.body_to_alias.get(&body)
                    && *entry == *def_id
                {
                    drop(entry);
                    self.body_to_alias.remove(&body);
                }

                // Clean up shape_to_def.
                if let Some(ref shape) = info.instance_shape {
                    let hash = Self::hash_shape(shape);
                    if let Some(entry) = self.shape_to_def.get(&hash)
                        && *entry == *def_id
                    {
                        drop(entry);
                        self.shape_to_def.remove(&hash);
                    }
                }

                // Clean up class_to_constructor (both directions).
                if info.kind == DefKind::Class {
                    self.class_to_constructor.remove(def_id);
                    self.class_to_instance.remove(def_id);
                } else if info.kind == DefKind::ClassConstructor {
                    // Remove any forward mapping that points to this constructor.
                    self.class_to_constructor.retain(|_, v| *v != *def_id);
                }

                // Clean up name_to_defs.
                if let Some(mut name_entry) = self.name_to_defs.get_mut(&info.name) {
                    name_entry.retain(|d| d != def_id);
                    if name_entry.is_empty() {
                        drop(name_entry);
                        self.name_to_defs.remove(&info.name);
                    }
                }
            }
        }

        trace!(
            instance_id = self.instance_id,
            file_id,
            invalidated_count = count,
            "DefinitionStore::invalidate_file"
        );

        count
    }

    /// Get the number of files that have definitions registered.
    ///
    /// Useful for diagnostics and testing.
    pub fn file_count(&self) -> usize {
        self.file_to_defs.len()
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
#[path = "../../tests/def_tests.rs"]
mod tests;
