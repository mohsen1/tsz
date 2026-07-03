//! Index Signature Resolution
//!
//! This module provides a unified interface for querying and resolving
//! index signatures on object types. Index signatures allow objects to be
//! accessed using string or numeric keys (e.g., `{ [key: string]: number }`).
//!
//! ## Key Types
//!
//! - **`IndexKind`**: Distinguishes between string and numeric index signatures
//! - **`IndexSignatureResolver`**: Main resolver for index signature queries
//!
//! ## Usage
//!
//! ```text
//! use crate::objects::index_signatures::IndexSignatureResolver;
//!
//! let resolver = IndexSignatureResolver::new(db);
//!
//! // Get string index signature type
//! if let Some(value_type) = resolver.resolve_string_index(obj_type) {
//!     // Object has string index signature
//! }
//!
//! // Check if index signature is readonly
//! if resolver.is_readonly(obj_type, IndexKind::String) {
//!     // Index signature is readonly
//! }
//! ```

use crate::TypeId;
use crate::construction::TypeDatabase;
use crate::relations::subtype::{NoopResolver, TypeResolver};
use crate::types::{
    CallableShapeId, IndexInfo, IndexSignature, MappedTypeId, ObjectShapeId, TypeData, TypeListId,
};
use crate::utils;
use crate::visitor::TypeVisitor;

/// Distinguishes between string and numeric index signatures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexKind {
    /// String index signature: `{ [key: string]: T }`
    String,
    /// Numeric index signature: `{ [key: number]: T }`
    Number,
}

fn merge_union_index_signatures(
    db: &dyn TypeDatabase,
    member_count: usize,
    signatures: &[IndexSignature],
    key_type: TypeId,
) -> Option<IndexSignature> {
    (member_count != 0 && signatures.len() == member_count).then(|| IndexSignature {
        key_type,
        value_type: db.union(signatures.iter().map(|sig| sig.value_type).collect()),
        readonly: signatures.iter().all(|sig| sig.readonly),
        param_name: None,
    })
}

fn merge_intersection_index_signature(
    db: &dyn TypeDatabase,
    slot: &mut Option<IndexSignature>,
    incoming: Option<IndexSignature>,
) {
    let Some(index) = incoming else { return };
    if let Some(existing) = slot {
        existing.value_type = db.intersect_types_raw2(existing.value_type, index.value_type);
        existing.readonly &= index.readonly;
        existing.param_name = None;
    } else {
        *slot = Some(IndexSignature {
            param_name: None,
            ..index
        });
    }
}

fn merge_intersection_index_value(
    db: &dyn TypeDatabase,
    slot: &mut Option<TypeId>,
    incoming: Option<TypeId>,
) {
    let Some(value_type) = incoming else { return };
    *slot = Some(match *slot {
        Some(existing) => db.intersect_types_raw2(existing, value_type),
        None => value_type,
    });
}

// =============================================================================
// Visitor Implementations for Index Signature Resolution
// =============================================================================

/// Visitor for resolving string index signatures.
struct StringIndexResolver<'a, R: TypeResolver> {
    db: &'a dyn TypeDatabase,
    resolver: &'a R,
}

impl<R: TypeResolver> TypeVisitor for StringIndexResolver<'_, R> {
    type Output = Option<TypeId>;

    fn visit_intrinsic(&mut self, _kind: crate::types::IntrinsicKind) -> Self::Output {
        None
    }

    fn visit_literal(&mut self, _value: &crate::LiteralValue) -> Self::Output {
        None
    }

    fn visit_object_with_index(&mut self, shape_id: u32) -> Self::Output {
        let shape = self.db.object_shape(ObjectShapeId(shape_id));
        shape
            .string_index
            .as_ref()
            .filter(|idx| idx.key_type != TypeId::SYMBOL)
            .map(|idx| idx.value_type)
    }

    fn visit_callable(&mut self, shape_id: u32) -> Self::Output {
        let shape = self.db.callable_shape(CallableShapeId(shape_id));
        shape
            .string_index
            .as_ref()
            .filter(|idx| idx.key_type != TypeId::SYMBOL)
            .map(|idx| idx.value_type)
    }

    fn visit_array(&mut self, element_type: TypeId) -> Self::Output {
        // Array/tuple types have readonly numeric index (which also supports string)
        Some(element_type)
    }

    fn visit_tuple(&mut self, _list_id: u32) -> Self::Output {
        // Would need union of all elements, return UNKNOWN for simplicity
        Some(TypeId::UNKNOWN)
    }

    fn visit_union(&mut self, list_id: u32) -> Self::Output {
        let types = self.db.type_list(TypeListId(list_id));
        if types.is_empty() {
            return None;
        }

        let mut values = Vec::with_capacity(types.len());
        for &ty in types.iter() {
            values.push(self.visit_type(self.db, ty)?);
        }
        Some(self.db.union(values))
    }

    fn visit_intersection(&mut self, list_id: u32) -> Self::Output {
        let types = self.db.type_list(TypeListId(list_id));
        let mut value_type = None;
        for &ty in types.iter() {
            let incoming = self.visit_type(self.db, ty);
            merge_intersection_index_value(self.db, &mut value_type, incoming);
        }
        value_type
    }

    fn visit_readonly_type(&mut self, inner_type: TypeId) -> Self::Output {
        self.visit_type(self.db, inner_type)
    }

    fn visit_application_type(&mut self, type_id: TypeId, _app_id: u32) -> Self::Output {
        let evaluated = crate::evaluation::evaluate::evaluate_type_with_resolver(
            self.db,
            self.resolver,
            type_id,
        );
        (evaluated != type_id)
            .then(|| self.visit_type(self.db, evaluated))
            .flatten()
    }

    fn visit_type(&mut self, types: &dyn TypeDatabase, type_id: TypeId) -> Self::Output {
        match types.lookup(type_id) {
            Some(TypeData::Application(app_id)) => self.visit_application_type(type_id, app_id.0),
            Some(ref type_key) => self.visit_type_key(types, type_key),
            None => Self::default_output(),
        }
    }

    fn visit_mapped(&mut self, mapped_id: u32) -> Self::Output {
        let type_id = self.db.mapped(self.db.get_mapped(MappedTypeId(mapped_id)));
        let evaluated = crate::evaluation::evaluate::evaluate_type(self.db, type_id);
        (evaluated != type_id)
            .then(|| self.visit_type(self.db, evaluated))
            .flatten()
    }

    fn default_output() -> Self::Output {
        None
    }
}

/// Visitor for resolving `symbol` index signatures (`{ [key: symbol]: T }`).
///
/// A `symbol` index is stored either in [`ObjectShape::symbol_index`] or,
/// historically, in `string_index` with a `key_type` of [`TypeId::SYMBOL`].
/// This resolver is the mirror of [`StringIndexResolver`] that reads *only* the
/// symbol-keyed signature, so callers can distinguish an object that genuinely
/// accepts any `symbol` key from one whose `symbol` access merely falls through
/// to a `string` index signature (which `tsc` rejects with TS2538).
struct SymbolIndexResolver<'a, R: TypeResolver> {
    db: &'a dyn TypeDatabase,
    resolver: &'a R,
}

impl<R: TypeResolver> TypeVisitor for SymbolIndexResolver<'_, R> {
    type Output = Option<TypeId>;

    fn visit_intrinsic(&mut self, _kind: crate::types::IntrinsicKind) -> Self::Output {
        None
    }

    fn visit_literal(&mut self, _value: &crate::LiteralValue) -> Self::Output {
        None
    }

    fn visit_object_with_index(&mut self, shape_id: u32) -> Self::Output {
        let shape = self.db.object_shape(ObjectShapeId(shape_id));
        shape.symbol_index_signature().map(|idx| idx.value_type)
    }

    fn visit_callable(&mut self, shape_id: u32) -> Self::Output {
        // `CallableShape` has no dedicated `symbol_index` slot, so only the
        // historical `string_index`-with-`symbol`-key representation applies.
        let shape = self.db.callable_shape(CallableShapeId(shape_id));
        shape
            .string_index
            .as_ref()
            .filter(|idx| idx.key_type == TypeId::SYMBOL)
            .map(|idx| idx.value_type)
    }

    fn visit_array(&mut self, _element_type: TypeId) -> Self::Output {
        None
    }

    fn visit_tuple(&mut self, _list_id: u32) -> Self::Output {
        None
    }

    fn visit_union(&mut self, list_id: u32) -> Self::Output {
        let types = self.db.type_list(TypeListId(list_id));
        if types.is_empty() {
            return None;
        }

        let mut values = Vec::with_capacity(types.len());
        for &ty in types.iter() {
            values.push(self.visit_type(self.db, ty)?);
        }
        Some(self.db.union(values))
    }

    fn visit_intersection(&mut self, list_id: u32) -> Self::Output {
        let types = self.db.type_list(TypeListId(list_id));
        let mut value_type = None;
        for &ty in types.iter() {
            let incoming = self.visit_type(self.db, ty);
            merge_intersection_index_value(self.db, &mut value_type, incoming);
        }
        value_type
    }

    fn visit_readonly_type(&mut self, inner_type: TypeId) -> Self::Output {
        self.visit_type(self.db, inner_type)
    }

    fn visit_application_type(&mut self, type_id: TypeId, _app_id: u32) -> Self::Output {
        let evaluated = crate::evaluation::evaluate::evaluate_type_with_resolver(
            self.db,
            self.resolver,
            type_id,
        );
        (evaluated != type_id)
            .then(|| self.visit_type(self.db, evaluated))
            .flatten()
    }

    fn visit_type(&mut self, types: &dyn TypeDatabase, type_id: TypeId) -> Self::Output {
        match types.lookup(type_id) {
            Some(TypeData::Application(app_id)) => self.visit_application_type(type_id, app_id.0),
            Some(ref type_key) => self.visit_type_key(types, type_key),
            None => Self::default_output(),
        }
    }

    fn visit_mapped(&mut self, mapped_id: u32) -> Self::Output {
        let type_id = self.db.mapped(self.db.get_mapped(MappedTypeId(mapped_id)));
        let evaluated = crate::evaluation::evaluate::evaluate_type(self.db, type_id);
        (evaluated != type_id)
            .then(|| self.visit_type(self.db, evaluated))
            .flatten()
    }

    fn default_output() -> Self::Output {
        None
    }
}

/// Visitor for resolving number index signatures.
struct NumberIndexResolver<'a, R: TypeResolver> {
    db: &'a dyn TypeDatabase,
    resolver: &'a R,
}

impl<R: TypeResolver> TypeVisitor for NumberIndexResolver<'_, R> {
    type Output = Option<TypeId>;

    fn visit_intrinsic(&mut self, _kind: crate::types::IntrinsicKind) -> Self::Output {
        None
    }

    fn visit_literal(&mut self, _value: &crate::LiteralValue) -> Self::Output {
        None
    }

    fn visit_object_with_index(&mut self, shape_id: u32) -> Self::Output {
        let shape = self.db.object_shape(ObjectShapeId(shape_id));
        shape.number_index.as_ref().map(|idx| idx.value_type)
    }

    fn visit_callable(&mut self, shape_id: u32) -> Self::Output {
        let shape = self.db.callable_shape(CallableShapeId(shape_id));
        shape.number_index.as_ref().map(|idx| idx.value_type)
    }

    fn visit_array(&mut self, element_type: TypeId) -> Self::Output {
        Some(element_type)
    }

    fn visit_tuple(&mut self, list_id: u32) -> Self::Output {
        let elements = self.db.tuple_list(crate::types::TupleListId(list_id));
        let element_types: Vec<TypeId> = elements.iter().map(|element| element.type_id).collect();
        Some(match element_types.as_slice() {
            [] => TypeId::UNDEFINED,
            [single] => *single,
            _ => self.db.union(element_types),
        })
    }

    fn visit_union(&mut self, list_id: u32) -> Self::Output {
        let types = self.db.type_list(TypeListId(list_id));
        if types.is_empty() {
            return None;
        }

        let mut values = Vec::with_capacity(types.len());
        for &ty in types.iter() {
            values.push(self.visit_type(self.db, ty)?);
        }
        Some(self.db.union(values))
    }

    fn visit_intersection(&mut self, list_id: u32) -> Self::Output {
        let types = self.db.type_list(TypeListId(list_id));
        let mut value_type = None;
        for &ty in types.iter() {
            let incoming = self.visit_type(self.db, ty);
            merge_intersection_index_value(self.db, &mut value_type, incoming);
        }
        value_type
    }

    fn visit_readonly_type(&mut self, inner_type: TypeId) -> Self::Output {
        self.visit_type(self.db, inner_type)
    }

    fn visit_application_type(&mut self, type_id: TypeId, _app_id: u32) -> Self::Output {
        let evaluated = crate::evaluation::evaluate::evaluate_type_with_resolver(
            self.db,
            self.resolver,
            type_id,
        );
        (evaluated != type_id)
            .then(|| self.visit_type(self.db, evaluated))
            .flatten()
    }

    fn visit_type(&mut self, types: &dyn TypeDatabase, type_id: TypeId) -> Self::Output {
        match types.lookup(type_id) {
            Some(TypeData::Application(app_id)) => self.visit_application_type(type_id, app_id.0),
            Some(ref type_key) => self.visit_type_key(types, type_key),
            None => Self::default_output(),
        }
    }

    fn visit_mapped(&mut self, mapped_id: u32) -> Self::Output {
        let type_id = self.db.mapped(self.db.get_mapped(MappedTypeId(mapped_id)));
        let evaluated = crate::evaluation::evaluate::evaluate_type(self.db, type_id);
        (evaluated != type_id)
            .then(|| self.visit_type(self.db, evaluated))
            .flatten()
    }

    fn default_output() -> Self::Output {
        None
    }
}

/// Visitor for checking if an index signature is readonly.
struct ReadonlyChecker<'a> {
    db: &'a dyn TypeDatabase,
    kind: IndexKind,
}

impl<'a> TypeVisitor for ReadonlyChecker<'a> {
    type Output = Option<bool>;

    fn visit_intrinsic(&mut self, kind: crate::types::IntrinsicKind) -> Self::Output {
        // The `string` primitive has an implicit readonly number index signature
        // (you cannot assign to individual characters: `s[0] = "x"` is an error).
        (kind == crate::types::IntrinsicKind::String && matches!(self.kind, IndexKind::Number))
            .then_some(true)
    }

    fn visit_literal(&mut self, _value: &crate::LiteralValue) -> Self::Output {
        None
    }

    fn visit_array(&mut self, _element_type: TypeId) -> Self::Output {
        matches!(self.kind, IndexKind::Number).then_some(false)
    }

    fn visit_tuple(&mut self, _list_id: u32) -> Self::Output {
        matches!(self.kind, IndexKind::Number).then_some(false)
    }

    fn visit_object_with_index(&mut self, shape_id: u32) -> Self::Output {
        let shape = self.db.object_shape(ObjectShapeId(shape_id));
        match self.kind {
            IndexKind::String => shape.string_index_signature().map(|idx| idx.readonly),
            IndexKind::Number => shape.number_index.as_ref().map(|idx| idx.readonly),
        }
    }

    fn visit_callable(&mut self, shape_id: u32) -> Self::Output {
        let shape = self.db.callable_shape(CallableShapeId(shape_id));
        match self.kind {
            IndexKind::String => shape
                .string_index
                .as_ref()
                .filter(|idx| idx.key_type != TypeId::SYMBOL)
                .map(|idx| idx.readonly),
            IndexKind::Number => shape.number_index.as_ref().map(|idx| idx.readonly),
        }
    }

    fn visit_union(&mut self, list_id: u32) -> Self::Output {
        let types = self.db.type_list(TypeListId(list_id));
        if types.is_empty() {
            return None;
        }

        let mut all_readonly = true;
        for &ty in types.iter() {
            all_readonly &= self.visit_type(self.db, ty)?;
        }
        Some(all_readonly)
    }

    fn visit_intersection(&mut self, list_id: u32) -> Self::Output {
        let types = self.db.type_list(TypeListId(list_id));
        let mut readonly = None;
        for &ty in types.iter() {
            if let Some(incoming) = self.visit_type(self.db, ty) {
                readonly = Some(readonly.unwrap_or(true) && incoming);
            }
        }
        readonly
    }

    fn visit_readonly_type(&mut self, inner_type: TypeId) -> Self::Output {
        self.visit_type(self.db, inner_type).map(|_| true)
    }

    fn visit_lazy(&mut self, def_id: u32) -> Self::Output {
        // Resolve lazy types (interfaces, classes, type aliases) before checking readonly
        let resolved = crate::evaluation::evaluate::evaluate_type(self.db, TypeId(def_id));
        self.visit_type(self.db, resolved)
    }

    fn default_output() -> Self::Output {
        None
    }
}

/// Visitor for collecting index signature information.
struct IndexInfoCollector<'a> {
    db: &'a dyn TypeDatabase,
}

impl<'a> TypeVisitor for IndexInfoCollector<'a> {
    type Output = IndexInfo;

    fn visit_intrinsic(&mut self, _kind: crate::types::IntrinsicKind) -> Self::Output {
        IndexInfo {
            string_index: None,
            number_index: None,
            symbol_index: None,
        }
    }

    fn visit_literal(&mut self, _value: &crate::LiteralValue) -> Self::Output {
        IndexInfo {
            string_index: None,
            number_index: None,
            symbol_index: None,
        }
    }

    fn visit_object_with_index(&mut self, shape_id: u32) -> Self::Output {
        let shape = self.db.object_shape(ObjectShapeId(shape_id));
        IndexInfo {
            string_index: shape.string_index_signature().copied(),
            number_index: shape.number_index,
            symbol_index: shape.symbol_index_signature().copied(),
        }
    }

    fn visit_callable(&mut self, shape_id: u32) -> Self::Output {
        let shape = self.db.callable_shape(CallableShapeId(shape_id));
        IndexInfo {
            string_index: shape
                .string_index
                .filter(|idx| idx.key_type != TypeId::SYMBOL),
            number_index: shape.number_index,
            symbol_index: shape
                .string_index
                .filter(|idx| idx.key_type == TypeId::SYMBOL),
        }
    }

    fn visit_array(&mut self, elem: TypeId) -> Self::Output {
        IndexInfo {
            string_index: None,
            number_index: Some(IndexSignature {
                key_type: TypeId::NUMBER,
                value_type: elem,
                readonly: false,
                param_name: None,
            }),
            symbol_index: None,
        }
    }

    fn visit_tuple(&mut self, list_id: u32) -> Self::Output {
        let elements = self.db.tuple_list(crate::types::TupleListId(list_id));
        let element_types: Vec<TypeId> = elements.iter().map(|element| element.type_id).collect();
        let value_type = match element_types.as_slice() {
            [] => TypeId::UNDEFINED,
            [single] => *single,
            _ => self.db.union(element_types),
        };
        IndexInfo {
            string_index: None,
            number_index: Some(IndexSignature {
                key_type: TypeId::NUMBER,
                value_type,
                readonly: false,
                param_name: None,
            }),
            symbol_index: None,
        }
    }

    fn visit_readonly_type(&mut self, inner_type: TypeId) -> Self::Output {
        let mut info = self.visit_type(self.db, inner_type);
        // Mark all signatures as readonly
        if let Some(idx) = &mut info.string_index {
            idx.readonly = true;
        }
        if let Some(idx) = &mut info.number_index {
            idx.readonly = true;
        }
        if let Some(idx) = &mut info.symbol_index {
            idx.readonly = true;
        }
        info
    }

    fn visit_union(&mut self, list_id: u32) -> Self::Output {
        let types = self.db.type_list(TypeListId(list_id));
        let mut string_indices = Vec::with_capacity(types.len());
        let mut number_indices = Vec::with_capacity(types.len());
        let mut symbol_indices = Vec::with_capacity(types.len());

        for &ty in types.iter() {
            let info = self.visit_type(self.db, ty);
            if let Some(sig) = info.string_index {
                string_indices.push(sig);
            }
            if let Some(sig) = info.number_index {
                number_indices.push(sig);
            }
            if let Some(sig) = info.symbol_index {
                symbol_indices.push(sig);
            }
        }

        IndexInfo {
            string_index: merge_union_index_signatures(
                self.db,
                types.len(),
                &string_indices,
                TypeId::STRING,
            ),
            number_index: merge_union_index_signatures(
                self.db,
                types.len(),
                &number_indices,
                TypeId::NUMBER,
            ),
            symbol_index: merge_union_index_signatures(
                self.db,
                types.len(),
                &symbol_indices,
                TypeId::SYMBOL,
            ),
        }
    }

    fn visit_intersection(&mut self, list_id: u32) -> Self::Output {
        let types = self.db.type_list(TypeListId(list_id));
        let mut info = IndexInfo::default();

        for &ty in types.iter() {
            let member = self.visit_type(self.db, ty);
            merge_intersection_index_signature(
                self.db,
                &mut info.string_index,
                member.string_index,
            );
            merge_intersection_index_signature(
                self.db,
                &mut info.number_index,
                member.number_index,
            );
            merge_intersection_index_signature(
                self.db,
                &mut info.symbol_index,
                member.symbol_index,
            );
        }

        info
    }

    fn visit_lazy(&mut self, def_id: u32) -> Self::Output {
        // Resolve lazy types (interfaces, classes, type aliases) before collecting index info
        let resolved = crate::evaluation::evaluate::evaluate_type(self.db, TypeId(def_id));
        self.visit_type(self.db, resolved)
    }

    fn visit_type(&mut self, types: &dyn TypeDatabase, type_id: TypeId) -> Self::Output {
        // `Mapped` types (e.g. `{ [K in \`on${string}\`]?: V }`) evaluate to `ObjectWithIndex`
        // with a template-literal string_index key. We must evaluate them first so that
        // `get_index_info` can return the restricted key type and let the caller decide
        // whether the index expression is valid (TS7053 gating).
        match types.lookup(type_id) {
            Some(TypeData::Mapped(_)) => {
                let evaluated = crate::evaluation::evaluate::evaluate_type(types, type_id);
                if evaluated != type_id {
                    return self.visit_type(types, evaluated);
                }
                Self::default_output()
            }
            Some(ref type_key) => self.visit_type_key(types, type_key),
            None => Self::default_output(),
        }
    }

    fn default_output() -> Self::Output {
        IndexInfo {
            string_index: None,
            number_index: None,
            symbol_index: None,
        }
    }
}

/// Resolver for index signature queries on types.
///
/// This struct provides a unified interface for querying index signatures
/// across different type representations (`ObjectWithIndex`, Union, etc.).
pub struct IndexSignatureResolver<'a, R: TypeResolver = NoopResolver> {
    db: &'a dyn TypeDatabase,
    resolver: &'a R,
}

impl<'a> IndexSignatureResolver<'a, NoopResolver> {
    /// Create a new index signature resolver.
    pub fn new(db: &'a dyn TypeDatabase) -> Self {
        static NOOP: NoopResolver = NoopResolver;
        Self {
            db,
            resolver: &NOOP,
        }
    }
}

impl<'a, R: TypeResolver> IndexSignatureResolver<'a, R> {
    /// Create a new index signature resolver with a lazy/application resolver.
    pub fn with_resolver(db: &'a dyn TypeDatabase, resolver: &'a R) -> Self {
        Self { db, resolver }
    }

    /// Resolve the string index signature type from an object type.
    ///
    /// Returns `Some(value_type)` if the object has a string index signature,
    /// `None` otherwise.
    ///
    /// ## Examples
    ///
    /// - `{ [key: string]: number }` → `Some(TypeId::NUMBER)`
    /// - `{ [key: string]: string }` → `Some(TypeId::STRING)`
    /// - `{ a: number }` → `None`
    pub fn resolve_string_index(&self, obj: TypeId) -> Option<TypeId> {
        let mut visitor = StringIndexResolver {
            db: self.db,
            resolver: self.resolver,
        };
        visitor.visit_type(self.db, obj)
    }

    /// Resolve the `symbol` index signature type from an object type.
    ///
    /// Returns `Some(value_type)` when the object exposes a signature keyed by
    /// the bare `symbol` intrinsic (`{ [key: symbol]: T }`), `None` otherwise.
    /// A `string` (or numeric-string) index signature is *not* reported here —
    /// a `symbol` key cannot be satisfied by a `string` index, so callers can
    /// use this to tell a genuine symbol-indexable object apart from one where a
    /// `symbol` access only falls through to the `string` signature.
    ///
    /// ## Examples
    ///
    /// - `{ [key: symbol]: number }` → `Some(TypeId::NUMBER)`
    /// - `Record<PropertyKey, string>` → `Some(TypeId::STRING)`
    /// - `{ [key: string]: number }` → `None`
    pub fn resolve_symbol_index(&self, obj: TypeId) -> Option<TypeId> {
        let mut visitor = SymbolIndexResolver {
            db: self.db,
            resolver: self.resolver,
        };
        visitor.visit_type(self.db, obj)
    }

    /// Resolve the numeric index signature type from an object type.
    ///
    /// Returns `Some(value_type)` if the object has a numeric index signature,
    /// `None` otherwise.
    ///
    /// ## Examples
    ///
    /// - `{ [key: number]: string }` → `Some(TypeId::STRING)`
    /// - `{ [key: number]: number }` → `Some(TypeId::NUMBER)`
    /// - `{ a: number }` → `None`
    ///
    /// Note: Array and tuple types have implicit numeric index signatures.
    pub fn resolve_number_index(&self, obj: TypeId) -> Option<TypeId> {
        let mut visitor = NumberIndexResolver {
            db: self.db,
            resolver: self.resolver,
        };
        visitor.visit_type(self.db, obj)
    }

    /// Check if an index signature is readonly.
    ///
    /// ## Parameters
    ///
    /// - `obj`: The type to check
    /// - `kind`: Which index signature to check (string or number)
    ///
    /// ## Returns
    ///
    /// `true` if the requested index signature is readonly, `false` otherwise.
    ///
    /// ## Examples
    ///
    /// - `{ readonly [x: string]: string }` with `IndexKind::String` → `true`
    /// - `{ [x: string]: string }` with `IndexKind::String` → `false`
    pub fn is_readonly(&self, obj: TypeId, kind: IndexKind) -> bool {
        let mut visitor = ReadonlyChecker { db: self.db, kind };
        visitor.visit_type(self.db, obj).unwrap_or(false)
    }

    /// Get all index signatures from a type.
    ///
    /// Returns an `IndexInfo` struct containing both string and numeric
    /// index signatures if present.
    pub fn get_index_info(&self, obj: TypeId) -> IndexInfo {
        let mut collector = IndexInfoCollector { db: self.db };
        collector.visit_type(self.db, obj)
    }

    /// Check if a type has a specific index signature.
    ///
    /// ## Parameters
    ///
    /// - `obj`: The type to check
    /// - `kind`: Which index signature to check for (string or number)
    ///
    /// ## Returns
    ///
    /// `true` if the type has the requested index signature, `false` otherwise.
    pub fn has_index_signature(&self, obj: TypeId, kind: IndexKind) -> bool {
        match kind {
            IndexKind::String => self.resolve_string_index(obj).is_some(),
            IndexKind::Number => self.resolve_number_index(obj).is_some(),
        }
    }

    /// Check if a property name is a valid numeric index.
    ///
    /// ## Examples
    ///
    /// - `"0"` → `true`
    /// - `"42"` → `true`
    /// - `"foo"` → `false`
    /// - `"-1"` → `false`
    /// - `"NaN"` → `true`
    /// - `"Infinity"` → `true`
    pub fn is_numeric_index_name(&self, name: &str) -> bool {
        if name.starts_with('-') && name != "-Infinity" {
            return false;
        }
        utils::is_numeric_literal_name(name)
    }
}

#[cfg(test)]
#[path = "../../tests/index_signatures_tests.rs"]
mod tests;
