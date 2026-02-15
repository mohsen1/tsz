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
        }
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
        }
    }

    /// Set the extends parent for a class.
    pub const fn with_extends(mut self, parent: DefId) -> Self {
        self.extends = Some(parent);
        self
    }

    /// Set implemented interfaces.
    pub fn with_implements(mut self, interfaces: Vec<DefId>) -> Self {
        self.implements = interfaces;
        self
    }

    /// Set exports for a namespace/module.
    pub fn with_exports(mut self, exports: Vec<(Atom, DefId)>) -> Self {
        self.exports = exports;
        self
    }

    /// Add an export to the namespace/module.
    pub fn add_export(&mut self, name: Atom, def_id: DefId) {
        self.exports.push((name, def_id));
    }

    /// Look up an export by name.
    pub fn get_export(&self, name: Atom) -> Option<DefId> {
        self.exports
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, d)| *d)
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
/// ```ignore
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
pub struct DefinitionStore {
    /// Unique instance ID for debugging (tracks which store instance this is)
    instance_id: u64,

    /// `DefId` -> `DefinitionInfo` mapping
    definitions: DashMap<DefId, DefinitionInfo>,

    /// Next available `DefId`
    next_id: AtomicU32,
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
        self.definitions.insert(id, info);
        id
    }

    /// Get definition info by `DefId`.
    pub fn get(&self, id: DefId) -> Option<DefinitionInfo> {
        self.definitions.get(&id).map(|r| r.clone())
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

    /// Get the instance shape for a class/interface.
    pub fn get_instance_shape(&self, id: DefId) -> Option<Arc<ObjectShape>> {
        self.definitions
            .get(&id)
            .and_then(|r| r.instance_shape.clone())
    }

    /// Get the static shape for a class.
    pub fn get_static_shape(&self, id: DefId) -> Option<Arc<ObjectShape>> {
        self.definitions
            .get(&id)
            .and_then(|r| r.static_shape.clone())
    }

    /// Get parent class `DefId` for a class.
    pub fn get_extends(&self, id: DefId) -> Option<DefId> {
        self.definitions.get(&id).and_then(|r| r.extends)
    }

    /// Get implemented interfaces for a class/interface.
    pub fn get_implements(&self, id: DefId) -> Option<Vec<DefId>> {
        self.definitions.get(&id).map(|r| r.implements.clone())
    }

    /// Update the body `TypeId` for a definition (for lazy evaluation).
    pub fn set_body(&self, id: DefId, body: TypeId) {
        if let Some(mut entry) = self.definitions.get_mut(&id) {
            entry.body = Some(body);
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
        self.next_id.store(DefId::FIRST_VALID, Ordering::SeqCst);
    }

    /// Get exports for a namespace/module `DefId`.
    pub fn get_exports(&self, id: DefId) -> Option<Vec<(Atom, DefId)>> {
        self.definitions.get(&id).map(|r| r.exports.clone())
    }

    /// Get enum members for an enum `DefId`.
    pub fn get_enum_members(&self, id: DefId) -> Option<Vec<(Atom, EnumMemberValue)>> {
        self.definitions.get(&id).map(|r| r.enum_members.clone())
    }

    /// Get the name of a definition.
    pub fn get_name(&self, id: DefId) -> Option<Atom> {
        self.definitions.get(&id).map(|r| r.name)
    }

    /// Update exports for a definition (for lazy population).
    pub fn set_exports(&self, id: DefId, exports: Vec<(Atom, DefId)>) {
        if let Some(mut entry) = self.definitions.get_mut(&id) {
            entry.exports = exports;
        }
    }

    /// Add an export to an existing definition.
    pub fn add_export(&self, id: DefId, name: Atom, export_def: DefId) {
        if let Some(mut entry) = self.definitions.get_mut(&id) {
            entry.add_export(name, export_def);
        }
    }

    /// Update enum members for a definition (for lazy population).
    pub fn set_enum_members(&self, id: DefId, members: Vec<(Atom, EnumMemberValue)>) {
        if let Some(mut entry) = self.definitions.get_mut(&id) {
            entry.enum_members = members;
        }
    }

    /// Get all `DefIds` (for debugging/testing).
    pub fn all_ids(&self) -> Vec<DefId> {
        self.definitions.iter().map(|r| *r.key()).collect()
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
#[path = "../tests/def_tests.rs"]
mod tests;
