//! Definition identifiers and storage for the solver.
//!
//! This module provides a Solver-owned definition identifier (`DefId`) that
//! replaces `SymbolRef` in types, enabling:
//!
//! - **Decoupling**: Solver is independent of Binder's symbol representation
//! - **Testing**: Types can be created and tested without a full Binder
//! - **Caching**: `DefId` provides a stable key for Salsa memoization
//!
//! ## Migration Path
//!
//! The transition from `SymbolRef` to `DefId` happens incrementally:
//!
//! 1. `TypeData::Ref(SymbolRef)` remains for backward compatibility
//! 2. New `TypeData::Lazy(DefId)` is added for migrated code
//! 3. Eventually, `Ref(SymbolRef)` is removed entirely
//!
//! ## `DefId` Allocation Strategies
//!
//! | Mode | Strategy | Use Case |
//! |------|----------|----------|
//! | CLI  | Sequential allocation | Fresh start each compilation |
//! | LSP  | Content-addressed hash | Stable IDs across edits |

use crate::types::{ObjectFlags, ObjectShape, PropertyInfo, TypeId, TypeParamInfo};
use dashmap::DashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use tracing::trace;
use tsz_common::interner::Atom;

/// Global counter for assigning unique instance IDs to `DefinitionStore` instances.
/// Used for debugging `DefId` collision issues.
static NEXT_INSTANCE_ID: AtomicU64 = AtomicU64::new(1);

// =============================================================================
// DefId - Solver-Owned Definition Identifier
// =============================================================================

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

impl DefinitionInfo {
    /// Create a new type alias definition.
    pub const fn type_alias(name: Atom, type_params: Vec<TypeParamInfo>, body: TypeId) -> Self {
        Self {
            kind: DefKind::TypeAlias,
            name,
            type_params,
            body: Some(body),
            instance_shape: None,
            static_shape: None,
            extends: None,
            implements: Vec::new(),
            enum_members: Vec::new(),
            exports: Vec::new(),
            file_id: None,
            span: None,
            symbol_id: None,
            heritage_names: Vec::new(),
            is_abstract: false,
            is_const: false,
            is_exported: false,
            is_global_augmentation: false,
            is_declare: false,
        }
    }

    /// Returns `true` if this definition represents a class constructor (static side).
    pub const fn is_class_constructor(&self) -> bool {
        matches!(self.kind, DefKind::ClassConstructor)
    }

    /// Create a new interface definition.
    pub fn interface(
        name: Atom,
        type_params: Vec<TypeParamInfo>,
        properties: Vec<PropertyInfo>,
    ) -> Self {
        let shape = ObjectShape {
            flags: ObjectFlags::empty(),
            properties,
            string_index: None,
            number_index: None,
            symbol: None,
        };
        Self {
            kind: DefKind::Interface,
            name,
            type_params,
            body: None, // Body computed on demand
            instance_shape: Some(Arc::new(shape)),
            static_shape: None,
            extends: None,
            implements: Vec::new(),
            enum_members: Vec::new(),
            exports: Vec::new(),
            file_id: None,
            span: None,
            symbol_id: None,
            heritage_names: Vec::new(),
            is_abstract: false,
            is_const: false,
            is_exported: false,
            is_global_augmentation: false,
            is_declare: false,
        }
    }

    /// Create a new class definition.
    pub fn class(
        name: Atom,
        type_params: Vec<TypeParamInfo>,
        instance_properties: Vec<PropertyInfo>,
        static_properties: Vec<PropertyInfo>,
    ) -> Self {
        let instance_shape = ObjectShape {
            flags: ObjectFlags::empty(),
            properties: instance_properties,
            string_index: None,
            number_index: None,
            symbol: None,
        };
        let static_shape = ObjectShape {
            flags: ObjectFlags::empty(),
            properties: static_properties,
            string_index: None,
            number_index: None,
            symbol: None,
        };
        Self {
            kind: DefKind::Class,
            name,
            type_params,
            body: None,
            instance_shape: Some(Arc::new(instance_shape)),
            static_shape: Some(Arc::new(static_shape)),
            extends: None,
            implements: Vec::new(),
            enum_members: Vec::new(),
            exports: Vec::new(),
            file_id: None,
            span: None,
            symbol_id: None,
            heritage_names: Vec::new(),
            is_abstract: false,
            is_const: false,
            is_exported: false,
            is_global_augmentation: false,
            is_declare: false,
        }
    }

    /// Create a new enum definition.
    pub const fn enumeration(name: Atom, members: Vec<(Atom, EnumMemberValue)>) -> Self {
        Self {
            kind: DefKind::Enum,
            name,
            type_params: Vec::new(),
            body: None,
            instance_shape: None,
            static_shape: None,
            extends: None,
            implements: Vec::new(),
            enum_members: members,
            exports: Vec::new(),
            file_id: None,
            span: None,
            symbol_id: None,
            heritage_names: Vec::new(),
            is_abstract: false,
            is_const: false,
            is_exported: false,
            is_global_augmentation: false,
            is_declare: false,
        }
    }

    /// Create a new namespace definition.
    pub const fn namespace(name: Atom, exports: Vec<(Atom, DefId)>) -> Self {
        Self {
            kind: DefKind::Namespace,
            name,
            type_params: Vec::new(),
            body: None,
            instance_shape: None,
            static_shape: None,
            extends: None,
            implements: Vec::new(),
            enum_members: Vec::new(),
            exports,
            file_id: None,
            span: None,
            symbol_id: None,
            heritage_names: Vec::new(),
            is_abstract: false,
            is_const: false,
            is_exported: false,
            is_global_augmentation: false,
            is_declare: false,
        }
    }

    /// Set the extends parent for a class.
    pub const fn with_extends(mut self, parent: DefId) -> Self {
        self.extends = Some(parent);
        self
    }

    /// Add an export to the namespace/module.
    pub fn add_export(&mut self, name: Atom, def_id: DefId) {
        self.exports.push((name, def_id));
    }

    /// Set file ID for debugging.
    pub const fn with_file_id(mut self, file_id: u32) -> Self {
        self.file_id = Some(file_id);
        self
    }

    /// Set source span.
    pub const fn with_span(mut self, start: u32, end: u32) -> Self {
        self.span = Some((start, end));
        self
    }
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
    definitions: DashMap<DefId, DefinitionInfo>,

    /// Next available `DefId`
    next_id: AtomicU32,

    /// Reverse map: `TypeId` -> `DefId` for named types.
    ///
    /// When a class/interface instance type is computed, the checker registers it here
    /// so the `TypeFormatter` can display the class/interface name instead of expanding
    /// the structural form (e.g., show "A" instead of "{ a: string }").
    type_to_def: DashMap<TypeId, DefId>,

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
    symbol_def_index: DashMap<(u32, u32), DefId>,

    /// Reverse index: `SymbolId` (raw u32) -> `DefId` (file-agnostic).
    ///
    /// Unlike `symbol_def_index` which uses the composite `(symbol_id, file_idx)` key,
    /// this index is keyed by `symbol_id` alone. It maps to the *first* `DefId`
    /// registered for that symbol. This serves the `TypeFormatter` use case where
    /// only a `SymbolRef` (raw u32) is available and we need *any* matching `DefId`
    /// to look up the definition name and type parameters.
    ///
    /// Replaces the O(N) linear scan in the previous `find_def_by_symbol`.
    symbol_only_index: DashMap<u32, DefId>,

    /// Reverse index: body `TypeId` -> `DefId` for non-generic type aliases.
    ///
    /// Populated by `set_body` when the definition is a `TypeAlias` with no type
    /// parameters. Enables O(1) lookup in `find_type_alias_by_body`, replacing an
    /// O(N) linear scan over all definitions. This is used by the `TypeFormatter`
    /// and error reporters to display alias names (e.g., "Color") instead of
    /// structural expansions (e.g., "{ r: number; g: number; b: number }").
    body_to_alias: DashMap<TypeId, DefId>,

    /// Reverse index: `file_id` -> `Vec<DefId>` for per-file definition lookups.
    ///
    /// Populated during `register()` when the `DefinitionInfo` has a `file_id`.
    /// Enables O(1) lookup of all definitions originating from a given file,
    /// which is the foundation for incremental invalidation: when a file changes,
    /// we can instantly find all `DefId`s that need to be refreshed without
    /// scanning the entire definition store.
    file_to_defs: DashMap<u32, Vec<DefId>>,

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
    shape_to_def: DashMap<u64, DefId>,

    /// Reverse index: Class `DefId` -> `ClassConstructor` `DefId`.
    ///
    /// Populated during pre-population when a `DefKind::Class` definition is
    /// registered alongside its companion `DefKind::ClassConstructor` identity.
    /// Enables O(1) lookup of the constructor companion for a class, so the
    /// checker can reuse the pre-populated identity instead of creating a new
    /// `DefId` on demand during type checking.
    class_to_constructor: DashMap<DefId, DefId>,

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
    name_to_defs: DashMap<Atom, Vec<DefId>>,

    /// Thread-safe cache of resolved symbol types for cross-file delegation.
    /// Key: `(SymbolId.0, file_idx)` -- Value: resolved `TypeId`.
    /// Prevents duplicate cross-file delegation in parallel checking.
    resolved_symbol_types: DashMap<(u32, u32), TypeId>,

    /// Per-file mutual exclusion locks for cross-file type delegation.
    /// Prevents concurrent delegation to the same target file.
    file_delegation_locks: DashMap<usize, Arc<Mutex<()>>>,

    /// Flag indicating that cross-batch heritage resolution and DefId population
    /// have already been completed. When `true`, `apply_to` skips the expensive
    /// `pre_populate_def_ids_from_all_binders()` and `resolve_cross_batch_heritage()`
    /// calls. Set by `mark_fully_populated()` after the first complete population pass.
    ///
    /// This prevents O(files * total_defs) work when checking many files in parallel,
    /// which was the root cause of hangs on large type libraries like ts-toolbelt.
    fully_populated: std::sync::atomic::AtomicBool,
}

// =============================================================================
// StoreStatistics - Observability for DefinitionStore
// =============================================================================

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

impl DefinitionStore {
    /// Create a new definition store.
    pub fn new() -> Self {
        let instance_id = NEXT_INSTANCE_ID.fetch_add(1, Ordering::SeqCst);
        trace!(instance_id, "DefinitionStore::new - creating new instance");
        Self {
            instance_id,
            definitions: DashMap::new(),
            next_id: AtomicU32::new(DefId::FIRST_VALID),
            type_to_def: DashMap::new(),
            symbol_def_index: DashMap::new(),
            symbol_only_index: DashMap::new(),
            body_to_alias: DashMap::new(),
            shape_to_def: DashMap::new(),
            file_to_defs: DashMap::new(),
            class_to_constructor: DashMap::new(),
            name_to_defs: DashMap::new(),
            resolved_symbol_types: DashMap::new(),
            file_delegation_locks: DashMap::new(),
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
            self.symbol_only_index.entry(sym_id).or_insert(id);
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

        self.definitions.insert(id, info);
        id
    }

    /// Register a `(SymbolId, file_idx)` → `DefId` mapping in the authoritative index.
    ///
    /// This should be called whenever a new `DefId` is created from a binder symbol,
    /// using the symbol's raw id and its `decl_file_idx`. The composite key ensures
    /// that the same `SymbolId(u32)` from different binders maps to different `DefIds`.
    pub fn register_symbol_mapping(&self, symbol_id: u32, file_idx: u32, def_id: DefId) {
        self.symbol_def_index.insert((symbol_id, file_idx), def_id);
        // Also maintain the file-agnostic index (keeps the first registered DefId).
        self.symbol_only_index.entry(symbol_id).or_insert(def_id);
    }

    /// Look up a `DefId` by `(SymbolId, file_idx)`.
    ///
    /// Returns `Some(def_id)` if a mapping was previously registered via
    /// `register_symbol_mapping`. This is an O(1) lookup that replaces the
    /// expensive multi-binder validation in `get_or_create_def_id`.
    pub fn lookup_by_symbol(&self, symbol_id: u32, file_idx: u32) -> Option<DefId> {
        self.symbol_def_index
            .get(&(symbol_id, file_idx))
            .map(|r| *r)
    }

    /// Get definition info by `DefId`.
    pub fn get(&self, id: DefId) -> Option<DefinitionInfo> {
        self.definitions.get(&id).as_deref().cloned()
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

    /// Get type parameters for a definition.
    pub fn get_type_params(&self, id: DefId) -> Option<Vec<TypeParamInfo>> {
        self.definitions.get(&id).map(|r| r.type_params.clone())
    }

    /// Get the body `TypeId` for a definition.
    pub fn get_body(&self, id: DefId) -> Option<TypeId> {
        self.definitions.get(&id).and_then(|r| r.body)
    }

    /// Get parent class `DefId` for a class.
    pub fn get_extends(&self, id: DefId) -> Option<DefId> {
        self.definitions.get(&id).and_then(|r| r.extends)
    }

    /// Set the heritage (extends + implements) for a definition after registration.
    ///
    /// Used for cross-batch heritage resolution: when a user class extends a lib
    /// type, the heritage is resolved by name after all pre-population batches
    /// have completed.
    pub fn set_heritage(&self, id: DefId, extends: Option<DefId>, implements: Vec<DefId>) {
        if let Some(mut entry) = self.definitions.get_mut(&id) {
            entry.extends = extends;
            entry.implements = implements;
        }
    }

    /// Update the body `TypeId` for a definition (for lazy evaluation).
    pub fn set_body(&self, id: DefId, body: TypeId) {
        if let Some(mut entry) = self.definitions.get_mut(&id) {
            entry.body = Some(body);

            // Maintain body_to_alias index for non-generic type aliases.
            if entry.kind == DefKind::TypeAlias && entry.type_params.is_empty() {
                self.body_to_alias.entry(body).or_insert(id);
            }
        }
    }

    /// Update the type parameters for a definition.
    ///
    /// Type parameters may be computed lazily after initial registration.
    /// Initialize per-file delegation locks for parallel checking.
    /// Mark the store as fully populated (all DefIds registered, heritage resolved).
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
        for i in 0..file_count {
            self.file_delegation_locks
                .entry(i)
                .or_insert_with(|| Arc::new(Mutex::new(())));
        }
    }

    /// Get the delegation lock for a target file.
    pub fn get_file_delegation_lock(&self, file_idx: usize) -> Option<Arc<Mutex<()>>> {
        self.file_delegation_locks
            .get(&file_idx)
            .map(|r| Arc::clone(r.value()))
    }

    /// Look up a previously resolved cross-file symbol type.
    pub fn get_resolved_symbol_type(&self, symbol_id: u32, file_idx: u32) -> Option<TypeId> {
        self.resolved_symbol_types
            .get(&(symbol_id, file_idx))
            .map(|r| *r)
    }

    /// Cache a resolved cross-file symbol type (first-writer-wins).
    pub fn cache_resolved_symbol_type(&self, symbol_id: u32, file_idx: u32, type_id: TypeId) {
        self.resolved_symbol_types
            .entry((symbol_id, file_idx))
            .or_insert(type_id);
    }

    /// This method synchronizes them into the `DefinitionInfo` so that
    /// the `TypeFormatter` can display generic types with their type
    /// parameter names (e.g., `MyClass<T>` instead of just `MyClass`).
    pub fn set_type_params(&self, id: DefId, params: Vec<TypeParamInfo>) {
        if let Some(mut entry) = self.definitions.get_mut(&id) {
            entry.type_params = params;
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
        self.symbol_def_index.clear();
        self.symbol_only_index.clear();
        self.body_to_alias.clear();
        self.shape_to_def.clear();
        self.file_to_defs.clear();
        self.class_to_constructor.clear();
        self.name_to_defs.clear();
        self.next_id.store(DefId::FIRST_VALID, Ordering::SeqCst);
    }

    /// Register a mapping from a `TypeId` to its defining `DefId`.
    ///
    /// Called by the checker after computing class/interface instance types
    /// so the `TypeFormatter` can display named types (e.g., "A" instead of
    /// "{ a: string }") even across file boundaries.
    pub fn register_type_to_def(&self, type_id: TypeId, def_id: DefId) {
        self.type_to_def.insert(type_id, def_id);
    }

    /// Look up the `DefId` that produced the given `TypeId`.
    ///
    /// Returns `Some(def_id)` if a class/interface was registered for this type.
    pub fn find_def_for_type(&self, type_id: TypeId) -> Option<DefId> {
        self.type_to_def.get(&type_id).map(|r| *r)
    }

    /// Register a mapping from a `Class` `DefId` to its `ClassConstructor` companion `DefId`.
    ///
    /// Called during pre-population to establish constructor identity at merge time
    /// rather than on-demand during type checking. The checker can then look up the
    /// companion with `get_constructor_def` and reuse the stable identity.
    pub fn register_constructor_companion(&self, class_def: DefId, ctor_def: DefId) {
        self.class_to_constructor.insert(class_def, ctor_def);
    }

    /// Look up the pre-populated `ClassConstructor` `DefId` for a class.
    ///
    /// Returns `Some(ctor_def_id)` if a constructor companion was registered
    /// during pre-population. Returns `None` for classes without a pre-populated
    /// companion (e.g., anonymous classes or those created on-demand).
    pub fn get_constructor_def(&self, class_def: DefId) -> Option<DefId> {
        self.class_to_constructor.get(&class_def).map(|r| *r)
    }

    /// Get exports for a namespace/module `DefId`.
    pub fn get_exports(&self, id: DefId) -> Option<Vec<(Atom, DefId)>> {
        self.definitions.get(&id).map(|r| r.exports.clone())
    }

    /// Get the name of a definition.
    pub fn get_name(&self, id: DefId) -> Option<Atom> {
        self.definitions.get(&id).map(|r| r.name)
    }

    /// Add an export to an existing definition.
    pub fn add_export(&self, id: DefId, name: Atom, export_def: DefId) {
        if let Some(mut entry) = self.definitions.get_mut(&id) {
            entry.add_export(name, export_def);
        }
    }

    /// Set the `extends` (parent class/interface) for an existing definition.
    ///
    /// Used by heritage resolution at pre-populate time to wire class/interface
    /// hierarchy from binder-owned stable identity rather than checker repair.
    pub fn set_extends(&self, id: DefId, extends: DefId) {
        if let Some(mut entry) = self.definitions.get_mut(&id) {
            entry.extends = Some(extends);
        }
    }

    /// Set the `implements` list for an existing definition.
    ///
    /// Used by heritage resolution at pre-populate time to wire interface
    /// implementations from binder-owned stable identity.
    pub fn set_implements(&self, id: DefId, implements: Vec<DefId>) {
        if let Some(mut entry) = self.definitions.get_mut(&id) {
            entry.implements = implements;
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
        self.symbol_only_index
            .iter()
            .map(|entry| (*entry.key(), *entry.value()))
            .collect()
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
        self.body_to_alias.get(&type_id).map(|r| *r)
    }

    /// Find all `DefId`s registered under the given name.
    ///
    /// Returns `None` if no definitions exist with that name. Multiple
    /// definitions may share a name (e.g., interface merging across files,
    /// or same-named types in different modules).
    ///
    /// O(1) via `name_to_defs` index.
    pub fn find_defs_by_name(&self, name: Atom) -> Option<Vec<DefId>> {
        self.name_to_defs.get(&name).map(|r| r.clone())
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

        let mut resolved = Vec::new();
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
        let def_ids = match self.file_to_defs.remove(&file_id) {
            Some((_, ids)) => ids,
            None => return 0,
        };

        let count = def_ids.len();
        for def_id in &def_ids {
            // Remove from the main store and capture the info for index cleanup.
            if let Some((_, info)) = self.definitions.remove(def_id) {
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
                    }
                }

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

    /// Compute a snapshot of store sizes and composition.
    ///
    /// This iterates all definitions once to count by `DefKind`, plus reads
    /// the length of each reverse index. Suitable for periodic logging or
    /// on-demand diagnostics; avoid calling on every type check.
    pub fn statistics(&self) -> StoreStatistics {
        let mut stats = StoreStatistics {
            total_definitions: self.definitions.len(),
            type_to_def_entries: self.type_to_def.len(),
            symbol_def_index_entries: self.symbol_def_index.len(),
            symbol_only_index_entries: self.symbol_only_index.len(),
            body_to_alias_entries: self.body_to_alias.len(),
            shape_to_def_entries: self.shape_to_def.len(),
            class_to_constructor_entries: self.class_to_constructor.len(),
            name_to_defs_entries: self.name_to_defs.len(),
            file_count: self.file_to_defs.len(),
            next_def_id: self.next_id.load(Ordering::Relaxed),
            ..Default::default()
        };

        for entry in &self.definitions {
            match entry.value().kind {
                DefKind::TypeAlias => stats.type_aliases += 1,
                DefKind::Interface => stats.interfaces += 1,
                DefKind::Class => stats.classes += 1,
                DefKind::ClassConstructor => stats.class_constructors += 1,
                DefKind::Enum => stats.enums += 1,
                DefKind::Namespace => stats.namespaces += 1,
                DefKind::Function => stats.functions += 1,
                DefKind::Variable => stats.variables += 1,
            }
        }

        stats.estimated_size_bytes = self.estimated_size_bytes();
        stats
    }

    /// Estimate the heap memory footprint of the store in bytes.
    ///
    /// Accounts for the `DashMap` overhead of each index and the `Vec`-backed
    /// fields inside `DefinitionInfo`. The result is a rough lower bound —
    /// `DashMap` shard overhead, alignment padding, and allocator metadata are
    /// not included. Useful for memory pressure tracking and telemetry.
    #[must_use]
    pub fn estimated_size_bytes(&self) -> usize {
        let mut size = std::mem::size_of::<Self>();

        // Per-entry overhead for DashMap: key + value + ~64 bytes bucket/shard overhead.
        const DASHMAP_ENTRY_OVERHEAD: usize = 64;

        // definitions: DefId -> DefinitionInfo
        for entry in &self.definitions {
            let info = entry.value();
            size += std::mem::size_of::<DefId>() + std::mem::size_of::<DefinitionInfo>();
            size += DASHMAP_ENTRY_OVERHEAD;
            // Vec fields inside DefinitionInfo
            size += info.type_params.capacity() * std::mem::size_of::<TypeParamInfo>();
            size += info.enum_members.capacity() * std::mem::size_of::<(Atom, EnumMemberValue)>();
            size += info.implements.capacity() * std::mem::size_of::<DefId>();
            size += info.exports.capacity() * std::mem::size_of::<(Atom, DefId)>();
            // Arc<ObjectShape> — count the shape itself (shared, but we include it here)
            if let Some(ref shape) = info.instance_shape {
                size += std::mem::size_of::<ObjectShape>();
                size += shape.properties.capacity() * std::mem::size_of::<PropertyInfo>();
            }
            if let Some(ref shape) = info.static_shape {
                size += std::mem::size_of::<ObjectShape>();
                size += shape.properties.capacity() * std::mem::size_of::<PropertyInfo>();
            }
        }

        // type_to_def: TypeId -> DefId
        size += self.type_to_def.len()
            * (std::mem::size_of::<TypeId>()
                + std::mem::size_of::<DefId>()
                + DASHMAP_ENTRY_OVERHEAD);

        // symbol_def_index: (u32, u32) -> DefId
        size += self.symbol_def_index.len()
            * (std::mem::size_of::<(u32, u32)>()
                + std::mem::size_of::<DefId>()
                + DASHMAP_ENTRY_OVERHEAD);

        // symbol_only_index: u32 -> DefId
        size += self.symbol_only_index.len()
            * (std::mem::size_of::<u32>() + std::mem::size_of::<DefId>() + DASHMAP_ENTRY_OVERHEAD);

        // body_to_alias: TypeId -> DefId
        size += self.body_to_alias.len()
            * (std::mem::size_of::<TypeId>()
                + std::mem::size_of::<DefId>()
                + DASHMAP_ENTRY_OVERHEAD);

        // shape_to_def: u64 -> DefId
        size += self.shape_to_def.len()
            * (std::mem::size_of::<u64>() + std::mem::size_of::<DefId>() + DASHMAP_ENTRY_OVERHEAD);

        // class_to_constructor: DefId -> DefId
        size += self.class_to_constructor.len()
            * (std::mem::size_of::<DefId>()
                + std::mem::size_of::<DefId>()
                + DASHMAP_ENTRY_OVERHEAD);

        // file_to_defs: u32 -> Vec<DefId>
        for entry in &self.file_to_defs {
            size += std::mem::size_of::<u32>() + DASHMAP_ENTRY_OVERHEAD;
            size += entry.value().capacity() * std::mem::size_of::<DefId>();
        }

        // name_to_defs: Atom -> Vec<DefId>
        for entry in &self.name_to_defs {
            size += std::mem::size_of::<Atom>() + DASHMAP_ENTRY_OVERHEAD;
            size += entry.value().capacity() * std::mem::size_of::<DefId>();
        }

        size
    }

    /// Create a pre-populated `DefinitionStore` from binder `SemanticDefEntry` data.
    ///
    /// This is the canonical factory for converting binder-owned stable identity
    /// into solver `DefId`s. It runs as a standalone function (no checker context
    /// needed), enabling identity creation at merge time or single-file
    /// construction time rather than as checker-side repair.
    ///
    /// The function performs three passes:
    /// 1. Create `DefId`s and `DefinitionInfo` for each `SemanticDefEntry`.
    /// 2. Wire namespace exports from `parent_namespace` relationships.
    /// 3. Resolve heritage names (extends/implements) to `DefId`s.
    ///
    /// The `intern_string` callback abstracts over `TypeInterner::intern_string`
    /// vs `QueryDatabase::intern_string`, so both the merge pipeline and checker
    /// constructors can use this without coupling to a specific interner type.
    pub fn from_semantic_defs(
        semantic_defs: &rustc_hash::FxHashMap<tsz_binder::SymbolId, tsz_binder::SemanticDefEntry>,
        intern_string: impl Fn(&str) -> Atom,
    ) -> Self {
        let store = Self::new();

        if semantic_defs.is_empty() {
            return store;
        }

        // Pass 1: Create DefIds and DefinitionInfo for each entry.
        for (&sym_id, entry) in semantic_defs {
            let kind = match entry.kind {
                tsz_binder::SemanticDefKind::TypeAlias => DefKind::TypeAlias,
                tsz_binder::SemanticDefKind::Interface => DefKind::Interface,
                tsz_binder::SemanticDefKind::Class => DefKind::Class,
                tsz_binder::SemanticDefKind::Enum => DefKind::Enum,
                tsz_binder::SemanticDefKind::Namespace => DefKind::Namespace,
                tsz_binder::SemanticDefKind::Function => DefKind::Function,
                tsz_binder::SemanticDefKind::Variable => DefKind::Variable,
            };

            let name = intern_string(&entry.name);

            let type_params = if entry.type_param_count > 0 {
                (0..entry.type_param_count)
                    .map(|i| {
                        let param_name = entry
                            .type_param_names
                            .get(i as usize)
                            .map(|n| intern_string(n))
                            .unwrap_or(Atom(0));
                        crate::TypeParamInfo {
                            name: param_name,
                            constraint: None,
                            default: None,
                            is_const: false,
                        }
                    })
                    .collect()
            } else {
                Vec::new()
            };

            let enum_members: Vec<(Atom, EnumMemberValue)> = entry
                .enum_member_names
                .iter()
                .map(|n| (intern_string(n), EnumMemberValue::Computed))
                .collect();

            let info = DefinitionInfo {
                kind,
                name,
                type_params,
                body: None,
                instance_shape: None,
                static_shape: None,
                extends: None,
                implements: Vec::new(),
                enum_members,
                exports: Vec::new(),
                file_id: Some(entry.file_id),
                span: Some((entry.span_start, entry.span_start)),
                symbol_id: Some(sym_id.0),
                heritage_names: entry.heritage_names(),
                is_abstract: entry.is_abstract,
                is_const: entry.is_const,
                is_exported: entry.is_exported,
                is_global_augmentation: entry.is_global_augmentation,
                is_declare: entry.is_declare,
            };

            let def_id = store.register(info);
            store.register_symbol_mapping(sym_id.0, entry.file_id, def_id);

            // For classes, create a ClassConstructor companion DefId.
            if kind == DefKind::Class {
                let ctor_info = DefinitionInfo {
                    kind: DefKind::ClassConstructor,
                    name: intern_string(&entry.name),
                    type_params: Vec::new(),
                    body: None,
                    instance_shape: None,
                    static_shape: None,
                    extends: None,
                    implements: Vec::new(),
                    enum_members: Vec::new(),
                    exports: Vec::new(),
                    file_id: Some(entry.file_id),
                    span: Some((entry.span_start, entry.span_start)),
                    symbol_id: Some(sym_id.0),
                    heritage_names: Vec::new(),
                    is_abstract: entry.is_abstract,
                    is_const: false,
                    is_exported: entry.is_exported,
                    is_global_augmentation: false,
                    is_declare: entry.is_declare,
                };
                let ctor_def_id = store.register(ctor_info);
                store.register_constructor_companion(def_id, ctor_def_id);
            }
        }

        // Pass 2: Wire namespace exports from parent_namespace relationships.
        for (&sym_id, entry) in semantic_defs {
            if let Some(parent_sym) = entry.parent_namespace {
                let child_def = store.find_def_by_symbol(sym_id.0);
                let parent_def = store.find_def_by_symbol(parent_sym.0);
                if let (Some(child_def_id), Some(parent_def_id)) = (child_def, parent_def) {
                    let name = intern_string(&entry.name);
                    store.add_export(parent_def_id, name, child_def_id);
                }
            }
        }

        // Pass 3: Resolve heritage names to DefIds.
        for (&sym_id, entry) in semantic_defs {
            let def_id = match store.find_def_by_symbol(sym_id.0) {
                Some(id) => id,
                None => continue,
            };

            // Resolve extends_names → DefinitionInfo.extends
            if !entry.extends_names.is_empty() {
                for name_str in &entry.extends_names {
                    if name_str.contains('.') {
                        continue; // property-access names resolved by checker
                    }
                    let name_atom = intern_string(name_str);
                    if let Some(candidates) = store.find_defs_by_name(name_atom) {
                        for &candidate_id in &candidates {
                            if candidate_id == def_id {
                                continue;
                            }
                            if let Some(candidate_info) = store.get(candidate_id)
                                && matches!(
                                    candidate_info.kind,
                                    DefKind::Class | DefKind::Interface
                                )
                            {
                                store.set_extends(def_id, candidate_id);
                                break;
                            }
                        }
                    }
                    break; // only first extends name for the extends field
                }
            }

            // Resolve implements_names → DefinitionInfo.implements
            if !entry.implements_names.is_empty() {
                let mut resolved_implements = Vec::new();
                for name_str in &entry.implements_names {
                    if name_str.contains('.') {
                        continue;
                    }
                    let name_atom = intern_string(name_str);
                    if let Some(candidates) = store.find_defs_by_name(name_atom) {
                        for &candidate_id in &candidates {
                            if candidate_id == def_id {
                                continue;
                            }
                            if let Some(candidate_info) = store.get(candidate_id)
                                && matches!(
                                    candidate_info.kind,
                                    DefKind::Interface | DefKind::Class
                                )
                            {
                                resolved_implements.push(candidate_id);
                                break;
                            }
                        }
                    }
                }
                if !resolved_implements.is_empty() {
                    store.set_implements(def_id, resolved_implements);
                }
            }
        }

        // Mark as fully populated so parallel checkers skip redundant population.
        store.mark_fully_populated();

        store
    }
}

// =============================================================================
// Content-Addressed DefId (for LSP mode)
// =============================================================================

/// Content-addressed `DefId` generator for LSP mode.
///
/// Uses a hash of (name, `file_id`, span) to generate stable `DefIds`
/// that survive file edits without changing unrelated definitions.
pub struct ContentAddressedDefIds {
    /// Hash -> `DefId` mapping for deduplication
    hash_to_def: DashMap<u64, DefId>,

    /// Next `DefId` for new hashes
    next_id: AtomicU32,
}

impl Default for ContentAddressedDefIds {
    fn default() -> Self {
        Self::new()
    }
}

impl ContentAddressedDefIds {
    /// Create a new content-addressed `DefId` generator.
    pub fn new() -> Self {
        Self {
            hash_to_def: DashMap::new(),
            next_id: AtomicU32::new(DefId::FIRST_VALID),
        }
    }

    /// Get or create a `DefId` for the given content hash.
    ///
    /// # Arguments
    /// - `name`: Definition name
    /// - `file_id`: File identifier
    /// - `span_start`: Start offset of definition
    pub fn get_or_create(&self, name: Atom, file_id: u32, span_start: u32) -> DefId {
        use std::hash::{Hash, Hasher};

        // Compute content hash
        let mut hasher = rustc_hash::FxHasher::default();
        name.hash(&mut hasher);
        file_id.hash(&mut hasher);
        span_start.hash(&mut hasher);
        let hash = hasher.finish();

        // Check existing
        if let Some(existing) = self.hash_to_def.get(&hash) {
            return *existing;
        }

        // Allocate new
        let id = DefId(self.next_id.fetch_add(1, Ordering::SeqCst));
        self.hash_to_def.insert(hash, id);
        id
    }

    /// Clear all mappings (for testing).
    pub fn clear(&self) {
        self.hash_to_def.clear();
        self.next_id.store(DefId::FIRST_VALID, Ordering::SeqCst);
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
#[path = "../../tests/def_tests.rs"]
mod tests;
