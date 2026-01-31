//! Definition identifiers and storage for the solver.
//!
//! This module provides a Solver-owned definition identifier (`DefId`) that
//! replaces `SymbolRef` in types, enabling:
//!
//! - **Decoupling**: Solver is independent of Binder's symbol representation
//! - **Testing**: Types can be created and tested without a full Binder
//! - **Caching**: DefId provides a stable key for Salsa memoization
//!
//! ## Migration Path
//!
//! The transition from `SymbolRef` to `DefId` happens incrementally:
//!
//! 1. `TypeKey::Ref(SymbolRef)` remains for backward compatibility
//! 2. New `TypeKey::Lazy(DefId)` is added for migrated code
//! 3. Eventually, `Ref(SymbolRef)` is removed entirely
//!
//! ## DefId Allocation Strategies
//!
//! | Mode | Strategy | Use Case |
//! |------|----------|----------|
//! | CLI  | Sequential allocation | Fresh start each compilation |
//! | LSP  | Content-addressed hash | Stable IDs across edits |

use crate::interner::Atom;
use crate::solver::types::{ObjectShape, PropertyInfo, TypeId, TypeParamInfo};
use dashmap::DashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

// =============================================================================
// DefId - Solver-Owned Definition Identifier
// =============================================================================

/// Solver-owned definition identifier.
///
/// Unlike `SymbolRef` which references Binder symbols, `DefId` is owned by
/// the Solver and can be created without Binder context.
///
/// ## Comparison with SymbolRef
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
    /// Sentinel value for invalid DefId.
    pub const INVALID: DefId = DefId(0);

    /// First valid DefId.
    pub const FIRST_VALID: u32 = 1;

    /// Check if this DefId is valid.
    pub fn is_valid(self) -> bool {
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
/// This is stored in `DefinitionStore` and retrieved by DefId.
#[derive(Clone, Debug)]
pub struct DefinitionInfo {
    /// Kind of definition (affects evaluation strategy)
    pub kind: DefKind,

    /// Name of the definition (for diagnostics)
    pub name: Atom,

    /// Type parameters for generic definitions
    pub type_params: Vec<TypeParamInfo>,

    /// The body TypeId (structural representation)
    /// For lazy definitions, this may be computed on demand
    pub body: Option<TypeId>,

    /// For classes: the instance type's structural shape
    pub instance_shape: Option<Arc<ObjectShape>>,

    /// For classes: the static type's structural shape
    pub static_shape: Option<Arc<ObjectShape>>,

    /// For classes: parent class DefId (if extends)
    pub extends: Option<DefId>,

    /// For classes/interfaces: implemented interfaces
    pub implements: Vec<DefId>,

    /// For enums: member names and values
    pub enum_members: Vec<(Atom, EnumMemberValue)>,

    /// For namespaces/modules: exported members
    /// Maps export name to the DefId of the exported type
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
    pub fn type_alias(name: Atom, type_params: Vec<TypeParamInfo>, body: TypeId) -> Self {
        DefinitionInfo {
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
            properties,
            string_index: None,
            number_index: None,
        };
        DefinitionInfo {
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
            properties: instance_properties,
            string_index: None,
            number_index: None,
        };
        let static_shape = ObjectShape {
            properties: static_properties,
            string_index: None,
            number_index: None,
        };
        DefinitionInfo {
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
    pub fn enumeration(name: Atom, members: Vec<(Atom, EnumMemberValue)>) -> Self {
        DefinitionInfo {
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
    pub fn namespace(name: Atom, exports: Vec<(Atom, DefId)>) -> Self {
        DefinitionInfo {
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
    pub fn with_extends(mut self, parent: DefId) -> Self {
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
    pub fn with_file_id(mut self, file_id: u32) -> Self {
        self.file_id = Some(file_id);
        self
    }

    /// Set source span.
    pub fn with_span(mut self, start: u32, end: u32) -> Self {
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
    /// DefId -> DefinitionInfo mapping
    definitions: DashMap<DefId, DefinitionInfo>,

    /// Next available DefId
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
        DefinitionStore {
            definitions: DashMap::new(),
            next_id: AtomicU32::new(DefId::FIRST_VALID),
        }
    }

    /// Allocate a fresh DefId.
    fn allocate(&self) -> DefId {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        DefId(id)
    }

    /// Register a new definition and return its DefId.
    pub fn register(&self, info: DefinitionInfo) -> DefId {
        let id = self.allocate();
        self.definitions.insert(id, info);
        id
    }

    /// Get definition info by DefId.
    pub fn get(&self, id: DefId) -> Option<DefinitionInfo> {
        self.definitions.get(&id).map(|r| r.clone())
    }

    /// Check if a DefId exists.
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

    /// Get the body TypeId for a definition.
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

    /// Get parent class DefId for a class.
    pub fn get_extends(&self, id: DefId) -> Option<DefId> {
        self.definitions.get(&id).and_then(|r| r.extends)
    }

    /// Get implemented interfaces for a class/interface.
    pub fn get_implements(&self, id: DefId) -> Option<Vec<DefId>> {
        self.definitions.get(&id).map(|r| r.implements.clone())
    }

    /// Update the body TypeId for a definition (for lazy evaluation).
    pub fn set_body(&self, id: DefId, body: TypeId) {
        if let Some(mut entry) = self.definitions.get_mut(&id) {
            entry.body = Some(body);
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

    /// Get all DefIds (for debugging/testing).
    pub fn all_ids(&self) -> Vec<DefId> {
        self.definitions.iter().map(|r| *r.key()).collect()
    }
}

// =============================================================================
// Content-Addressed DefId (for LSP mode)
// =============================================================================

/// Content-addressed DefId generator for LSP mode.
///
/// Uses a hash of (name, file_id, span) to generate stable DefIds
/// that survive file edits without changing unrelated definitions.
pub struct ContentAddressedDefIds {
    /// Hash -> DefId mapping for deduplication
    hash_to_def: DashMap<u64, DefId>,

    /// Next DefId for new hashes
    next_id: AtomicU32,
}

impl Default for ContentAddressedDefIds {
    fn default() -> Self {
        Self::new()
    }
}

impl ContentAddressedDefIds {
    /// Create a new content-addressed DefId generator.
    pub fn new() -> Self {
        ContentAddressedDefIds {
            hash_to_def: DashMap::new(),
            next_id: AtomicU32::new(DefId::FIRST_VALID),
        }
    }

    /// Get or create a DefId for the given content hash.
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
mod tests {
    use super::*;
    use crate::solver::TypeInterner;

    fn create_test_interner() -> TypeInterner {
        TypeInterner::new()
    }

    #[test]
    fn test_def_id_validity() {
        assert!(!DefId::INVALID.is_valid());
        assert!(DefId(1).is_valid());
        assert!(DefId(100).is_valid());
    }

    #[test]
    fn test_definition_store_basic() {
        let interner = create_test_interner();
        let store = DefinitionStore::new();

        let name = interner.intern_string("Foo");
        let info = DefinitionInfo::type_alias(name, vec![], TypeId::NUMBER);

        let def_id = store.register(info);
        assert!(def_id.is_valid());
        assert!(store.contains(def_id));

        let retrieved = store.get(def_id).expect("definition exists");
        assert_eq!(retrieved.kind, DefKind::TypeAlias);
        assert_eq!(retrieved.name, name);
        assert_eq!(retrieved.body, Some(TypeId::NUMBER));
    }

    #[test]
    fn test_definition_store_interface() {
        let interner = create_test_interner();
        let store = DefinitionStore::new();

        let name = interner.intern_string("Point");
        let x_name = interner.intern_string("x");
        let y_name = interner.intern_string("y");

        let info = DefinitionInfo::interface(
            name,
            vec![],
            vec![
                PropertyInfo {
                    name: x_name,
                    type_id: TypeId::NUMBER,
                    write_type: TypeId::NUMBER,
                    optional: false,
                    readonly: false,
                    is_method: false,
                },
                PropertyInfo {
                    name: y_name,
                    type_id: TypeId::NUMBER,
                    write_type: TypeId::NUMBER,
                    optional: false,
                    readonly: false,
                    is_method: false,
                },
            ],
        );

        let def_id = store.register(info);

        let retrieved = store.get(def_id).expect("definition exists");
        assert_eq!(retrieved.kind, DefKind::Interface);

        let shape = retrieved.instance_shape.expect("has instance shape");
        assert_eq!(shape.properties.len(), 2);
    }

    #[test]
    fn test_definition_store_class_with_extends() {
        let interner = create_test_interner();
        let store = DefinitionStore::new();

        // Base class
        let base_name = interner.intern_string("Base");
        let base_info = DefinitionInfo::class(base_name, vec![], vec![], vec![]);
        let base_id = store.register(base_info);

        // Derived class
        let derived_name = interner.intern_string("Derived");
        let derived_info =
            DefinitionInfo::class(derived_name, vec![], vec![], vec![]).with_extends(base_id);
        let derived_id = store.register(derived_info);

        assert_eq!(store.get_extends(derived_id), Some(base_id));
        assert_eq!(store.get_extends(base_id), None);
    }

    #[test]
    fn test_definition_store_enum() {
        let interner = create_test_interner();
        let store = DefinitionStore::new();

        let name = interner.intern_string("Direction");
        let up = interner.intern_string("Up");
        let down = interner.intern_string("Down");

        let info = DefinitionInfo::enumeration(
            name,
            vec![
                (up, EnumMemberValue::Number(0.0)),
                (down, EnumMemberValue::Number(1.0)),
            ],
        );

        let def_id = store.register(info);

        let retrieved = store.get(def_id).expect("definition exists");
        assert_eq!(retrieved.kind, DefKind::Enum);
        assert_eq!(retrieved.enum_members.len(), 2);
    }

    #[test]
    fn test_definition_store_set_body() {
        let interner = create_test_interner();
        let store = DefinitionStore::new();

        let name = interner.intern_string("Point");
        let mut info = DefinitionInfo::interface(name, vec![], vec![]);
        info.body = None; // Start with no body

        let def_id = store.register(info);
        assert_eq!(store.get_body(def_id), None);

        // Set body later
        store.set_body(def_id, TypeId::NUMBER);
        assert_eq!(store.get_body(def_id), Some(TypeId::NUMBER));
    }

    #[test]
    fn test_content_addressed_def_ids() {
        let interner = create_test_interner();
        let generator = ContentAddressedDefIds::new();

        let name = interner.intern_string("Foo");

        // Same content -> same DefId
        let id1 = generator.get_or_create(name, 1, 100);
        let id2 = generator.get_or_create(name, 1, 100);
        assert_eq!(id1, id2);

        // Different content -> different DefId
        let id3 = generator.get_or_create(name, 1, 200);
        assert_ne!(id1, id3);

        let id4 = generator.get_or_create(name, 2, 100);
        assert_ne!(id1, id4);

        let name2 = interner.intern_string("Bar");
        let id5 = generator.get_or_create(name2, 1, 100);
        assert_ne!(id1, id5);
    }

    #[test]
    fn test_definition_store_concurrent() {
        use std::thread;

        let store = std::sync::Arc::new(DefinitionStore::new());

        let handles: Vec<_> = (0..4)
            .map(|i| {
                let store = store.clone();
                thread::spawn(move || {
                    for j in 0..100 {
                        let info = DefinitionInfo {
                            kind: DefKind::TypeAlias,
                            name: crate::interner::Atom(i * 1000 + j),
                            type_params: vec![],
                            body: Some(TypeId::NUMBER),
                            instance_shape: None,
                            static_shape: None,
                            extends: None,
                            implements: Vec::new(),
                            enum_members: Vec::new(),
                            exports: Vec::new(),
                            file_id: None,
                            span: None,
                        };
                        let id = store.register(info);
                        assert!(store.contains(id));
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().expect("thread completed");
        }

        assert_eq!(store.len(), 400);
    }
}
