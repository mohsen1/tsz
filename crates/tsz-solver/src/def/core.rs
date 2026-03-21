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
use std::sync::Arc;
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
        }
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
        if info.kind == DefKind::TypeAlias && info.type_params.is_empty() {
            if let Some(body) = info.body {
                self.body_to_alias.entry(body).or_insert(id);
            }
        }

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
        self.symbol_def_index.get(&(symbol_id, file_idx)).map(|r| *r)
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
    /// This method synchronizes them into the `DefinitionInfo` so that
    /// the `TypeFormatter` can display generic types with their type
    /// parameter names (e.g., `MyClass<T>` instead of just `MyClass`).
    pub fn set_type_params(&self, id: DefId, params: Vec<TypeParamInfo>) {
        if let Some(mut entry) = self.definitions.get_mut(&id) {
            entry.type_params = params;
        }
    }

    /// Update the instance shape for a type definition.
    ///
    /// This is used by checker code when a concrete object-like shape is computed
    /// for an interface/class definition and should be recorded for diagnostics.
    pub fn set_instance_shape(&self, id: DefId, shape: Arc<ObjectShape>) {
        if let Some(mut entry) = self.definitions.get_mut(&id) {
            entry.instance_shape = Some(shape);
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

    /// Find a `DefId` by its instance shape.
    ///
    /// This is used by the `TypeFormatter` to preserve interface names in error messages.
    /// When an Object type matches an interface's instance shape, we use the interface name
    /// instead of expanding the object literal.
    pub fn find_def_by_shape(&self, shape: &ObjectShape) -> Option<DefId> {
        self.definitions
            .iter()
            .find(|entry| {
                entry
                    .value()
                    .instance_shape
                    .as_ref()
                    .map(std::convert::AsRef::as_ref)
                    == Some(shape)
            })
            .map(|entry| *entry.key())
    }

    /// Check if a type name is ambiguous (appears in multiple files).
    ///
    /// Used by the `TypeFormatter` to decide whether to import-qualify a type.
    /// tsc only shows `import("specifier").TypeName` when there are multiple types
    /// with the same name in different files. If the name is unique, tsc shows
    /// just `TypeName` even for cross-file types.
    pub fn has_ambiguous_name(
        &self,
        name: &str,
        file_id: u32,
        interner: &dyn crate::TypeDatabase,
    ) -> bool {
        self.definitions.iter().any(|entry| {
            let def = entry.value();
            let def_name = interner.resolve_atom_ref(def.name);
            def_name.as_ref() == name && def.file_id.is_some_and(|fid| fid != file_id)
        })
    }

    /// Find a `DefId` by its associated `SymbolId` (raw u32).
    ///
    /// Used by the `TypeFormatter` to look up whether a symbol corresponds to a
    /// generic definition, enabling display of type parameters in error messages
    /// (e.g., `S18<unknown, unknown, unknown>` instead of just `S18`).
    ///
    /// O(1) via `symbol_only_index`; falls back to linear scan for DefIds
    /// registered before the index was populated (should not happen in practice).
    pub fn find_def_by_symbol(&self, symbol_id: u32) -> Option<DefId> {
        // Fast path: O(1) index lookup.
        if let Some(def_id) = self.symbol_only_index.get(&symbol_id) {
            return Some(*def_id);
        }

        // Fallback: linear scan for backward compatibility with definitions
        // registered without symbol_id in the index (e.g., test code).
        self.definitions
            .iter()
            .find(|entry| entry.value().symbol_id == Some(symbol_id))
            .map(|entry| *entry.key())
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
    /// O(1) via `body_to_alias` index; falls back to linear scan for bodies
    /// set before the index was populated (should not happen in practice).
    pub fn find_type_alias_by_body(&self, type_id: TypeId) -> Option<DefId> {
        // Fast path: O(1) index lookup.
        if let Some(def_id) = self.body_to_alias.get(&type_id) {
            return Some(*def_id);
        }

        // Fallback: linear scan for backward compatibility.
        self.definitions
            .iter()
            .find(|entry| {
                let def = entry.value();
                def.kind == DefKind::TypeAlias
                    && def.type_params.is_empty()
                    && def.body == Some(type_id)
            })
            .map(|entry| *entry.key())
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
