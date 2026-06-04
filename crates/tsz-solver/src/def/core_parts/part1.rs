

pub use content_addressed::ContentAddressedDefIds;

#[cfg(test)]
use crate::types::ObjectFlags;

use crate::types::{ObjectShape, PropertyInfo, TypeId, TypeParamInfo};

use dashmap::{DashMap, DashSet};

use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};

use std::hash::{Hash, Hasher};

use std::sync::Arc;

use std::sync::Mutex;

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use tracing::trace;

use tsz_common::interner::Atom;

/// Global counter for assigning unique instance IDs to `DefinitionStore` instances.
/// Used for debugging `DefId` collision issues.
static NEXT_INSTANCE_ID: AtomicU64 = AtomicU64::new(1);

type CrossFileQueryCacheKey = (u8, u32, u32, u32, u64);

type CrossFileQueryCacheValue = (TypeId, Arc<Vec<TypeParamInfo>>);

type DefDashMap<K, V> = DashMap<K, V, FxBuildHasher>;

type DefDashSet<K> = DashSet<K, FxBuildHasher>;

type SymbolMappingsSnapshot = Arc<[(u32, DefId)]>;

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
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct DefId(pub u32);

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

    /// Reverse map: `TypeId` -> `DefId` for named types.
    ///
    /// When a class/interface instance type is computed, the checker registers it here
    /// so the `TypeFormatter` can display the class/interface name instead of expanding
    /// the structural form (e.g., show "A" instead of "{ a: string }").
    type_to_def: DefDashMap<TypeId, DefId>,

    /// Forward map: `DefId` -> `TypeId` for type-parameter declarations.
    ///
    /// Lets the checker reuse the canonical `TypeId` allocated for a
    /// type-parameter declaration across reprocessings of the same
    /// signature. Cross-declaration distinctness is still guaranteed
    /// by `intern_fresh` because lookups key on the declaration's
    /// `DefId`, not on `TypeParamInfo` content. See
    /// `CheckerState::intern_type_param_for_decl` for the rationale.
    type_param_for_def: DefDashMap<DefId, TypeId>,

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

    /// Generation-keyed immutable snapshot of `symbol_only_index`.
    ///
    /// Project checking warms many per-file checker contexts from the same shared
    /// store. Caching this snapshot avoids collecting the same `DashMap` into a
    /// fresh `Vec` for every checker while preserving generation-based invalidation.
    symbol_mappings_snapshot: Mutex<Option<(u64, SymbolMappingsSnapshot)>>,

    /// Reverse index: body `TypeId` -> `DefId` for non-generic type aliases.
    ///
    /// Populated by `set_body` when the definition is a `TypeAlias` with no type
    /// parameters. Enables O(1) lookup in `find_type_alias_by_body`, replacing an
    /// O(N) linear scan over all definitions. This is used by the `TypeFormatter`
    /// and error reporters to display alias names (e.g., "Color") instead of
    /// structural expansions (e.g., "{ r: number; g: number; b: number }").
    body_to_alias: DefDashMap<TypeId, DefId>,

    /// Set of body `TypeId`s that were produced by type-level computation
    /// (intersection reduction, conditional evaluation) and should NOT be
    /// used to display alias names. tsc does not preserve alias names for
    /// such computed types (e.g., `type T2 = T1 & ("a"|"b")` evaluates to
    /// `"a"|"b"` but tsc shows the expanded union, not `T2`).
    computed_alias_bodies: DefDashSet<TypeId>,

    /// Set of type-alias `DefId`s whose instantiation is unconditionally
    /// infinite (e.g. `type A<T> = T extends infer X ? A<X & B> : never`).
    /// The checker records these when it emits TS2589 at the alias definition;
    /// the evaluator then resolves every `Alias<...>` application of a poisoned
    /// def to the error type so use sites do not cascade into spurious TS2322,
    /// matching tsc's collapse of the alias to the error type.
    depth_poisoned_defs: DefDashSet<DefId>,

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

    /// Thread-safe cache for cross-file checker queries (interface lowering,
    /// class instance type, interface member simple types, symbol type),
    /// keyed by `(kind, file_idx, primary, secondary, args_hash)`. The
    /// `SYMBOL_TYPE` bucket replaces the previous standalone
    /// `resolved_symbol_types` map.
    resolved_cross_file_queries: DefDashMap<CrossFileQueryCacheKey, CrossFileQueryCacheValue>,

    /// Program-local scope mixed into source-file symbol-type query keys.
    /// Batch drivers stamp this from `ProgramContext` so reused shared stores
    /// cannot read stale entries from an earlier virtual program.
    source_file_symbol_type_cache_scope: AtomicU64,

    /// Per-file mutual exclusion locks for cross-file type delegation.
    /// Prevents concurrent delegation to the same target file.
    file_delegation_locks: DefDashMap<usize, Arc<Mutex<()>>>,

    /// Flag indicating that cross-batch heritage resolution and DefId population
    /// have already been completed. When `true`, `apply_to` skips the expensive
    /// `pre_populate_def_ids_from_all_binders()` and `resolve_cross_batch_heritage()`
    /// calls. Set by `mark_fully_populated()` after the first complete population pass.
    ///
    /// This prevents O(files * `total_defs`) work when checking many files in parallel,
    /// which was the root cause of hangs on large type libraries like ts-toolbelt.
    fully_populated: std::sync::atomic::AtomicBool,

    /// Set of `DefId`s detected as circular type aliases (shared across checkers).
    circular_def_ids: DefDashSet<DefId>,
}

/// Snapshot of `DefinitionStore` sizes and composition.
///
/// Provides observability into the store's current state for performance
/// monitoring, capacity planning, and debugging. All counts are computed
/// at the time of the `statistics()` call and represent a consistent-ish
/// snapshot (individual `DashMap` reads are atomic but not globally synchronized).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StoreStatistics {
    /// Total number of definitions.
    pub total_definitions: usize,

    /// Number of definitions by kind.
    pub type_aliases: usize,
    /// Number of interface definitions.
    pub interfaces: usize,
    /// Number of class definitions.
    pub classes: usize,
    /// Number of class constructor definitions.
    pub class_constructors: usize,
    /// Number of enum definitions.
    pub enums: usize,
    /// Number of namespace definitions.
    pub namespaces: usize,
    /// Number of function definitions.
    pub functions: usize,
    /// Number of variable definitions.
    pub variables: usize,

    /// Number of entries in the `TypeId` -> `DefId` reverse index.
    pub type_to_def_entries: usize,
    /// Number of entries in the `(SymbolId, file_idx)` -> `DefId` index.
    pub symbol_def_index_entries: usize,
    /// Number of entries in the `SymbolId` -> `DefId` (file-agnostic) index.
    pub symbol_only_index_entries: usize,
    /// Number of entries in the body `TypeId` -> `DefId` alias index.
    pub body_to_alias_entries: usize,
    /// Number of entries in the shape hash -> `DefId` index.
    pub shape_to_def_entries: usize,
    /// Number of entries in the class -> constructor companion index.
    pub class_to_constructor_entries: usize,
    /// Number of unique names in the name -> `DefId` index.
    pub name_to_defs_entries: usize,
    /// Number of files with registered definitions.
    pub file_count: usize,

    /// Next `DefId` value (high-water mark of allocation).
    pub next_def_id: u32,

    /// Estimated heap memory footprint of the store in bytes.
    ///
    /// Populated by `DefinitionStore::statistics()` using the live
    /// `estimated_size_bytes()` method. Zero when constructed via `Default`.
    pub estimated_size_bytes: usize,
}

impl StoreStatistics {
    /// Merge another `StoreStatistics` into this one (additive).
    ///
    /// Used to aggregate per-file statistics from parallel checking,
    /// where each checker has its own `DefinitionStore`.
    pub const fn merge(&mut self, other: &StoreStatistics) {
        self.total_definitions += other.total_definitions;
        self.type_aliases += other.type_aliases;
        self.interfaces += other.interfaces;
        self.classes += other.classes;
        self.class_constructors += other.class_constructors;
        self.enums += other.enums;
        self.namespaces += other.namespaces;
        self.functions += other.functions;
        self.variables += other.variables;
        self.type_to_def_entries += other.type_to_def_entries;
        self.symbol_def_index_entries += other.symbol_def_index_entries;
        self.symbol_only_index_entries += other.symbol_only_index_entries;
        self.body_to_alias_entries += other.body_to_alias_entries;
        self.shape_to_def_entries += other.shape_to_def_entries;
        self.class_to_constructor_entries += other.class_to_constructor_entries;
        self.name_to_defs_entries += other.name_to_defs_entries;
        self.file_count += other.file_count;
        // next_def_id: take the maximum (high-water mark)
        if other.next_def_id > self.next_def_id {
            self.next_def_id = other.next_def_id;
        }
        self.estimated_size_bytes += other.estimated_size_bytes;
    }
}

impl std::fmt::Display for StoreStatistics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "DefinitionStore statistics:")?;
        writeln!(f, "  definitions: {} total", self.total_definitions)?;
        writeln!(
            f,
            "    type_aliases={}, interfaces={}, classes={}, class_constructors={}",
            self.type_aliases, self.interfaces, self.classes, self.class_constructors
        )?;
        writeln!(
            f,
            "    enums={}, namespaces={}, functions={}, variables={}",
            self.enums, self.namespaces, self.functions, self.variables
        )?;
        writeln!(f, "  indices:")?;
        writeln!(f, "    type_to_def={}", self.type_to_def_entries)?;
        writeln!(f, "    symbol_def_index={}", self.symbol_def_index_entries)?;
        writeln!(
            f,
            "    symbol_only_index={}",
            self.symbol_only_index_entries
        )?;
        writeln!(f, "    body_to_alias={}", self.body_to_alias_entries)?;
        writeln!(f, "    shape_to_def={}", self.shape_to_def_entries)?;
        writeln!(
            f,
            "    class_to_constructor={}",
            self.class_to_constructor_entries
        )?;
        writeln!(f, "    name_to_defs={}", self.name_to_defs_entries)?;
        writeln!(f, "  files: {}", self.file_count)?;
        writeln!(f, "  next_def_id: {}", self.next_def_id)?;
        write!(
            f,
            "  estimated_size: {} bytes ({:.1} KB)",
            self.estimated_size_bytes,
            self.estimated_size_bytes as f64 / 1024.0,
        )
    }
}

impl Default for DefinitionStore {
    fn default() -> Self {
        Self::new()
    }
}
