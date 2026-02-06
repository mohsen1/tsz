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
//! ```rust
//! use crate::index_signatures::IndexSignatureResolver;
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

use crate::types::{IndexInfo, IndexSignature, ObjectShapeId};
use crate::visitor::TypeVisitor;
use crate::{TypeDatabase, TypeId};

/// Distinguishes between string and numeric index signatures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexKind {
    /// String index signature: `{ [key: string]: T }`
    String,
    /// Numeric index signature: `{ [key: number]: T }`
    Number,
}

// =============================================================================
// Visitor Implementations for Index Signature Resolution
// =============================================================================

/// Visitor for resolving string index signatures.
struct StringIndexResolver<'a> {
    db: &'a dyn TypeDatabase,
}

impl<'a> TypeVisitor for StringIndexResolver<'a> {
    type Output = Option<TypeId>;

    fn visit_intrinsic(&mut self, _kind: crate::types::IntrinsicKind) -> Self::Output {
        None
    }

    fn visit_literal(&mut self, _value: &crate::LiteralValue) -> Self::Output {
        None
    }

    fn visit_object_with_index(&mut self, shape_id: u32) -> Self::Output {
        let shape = self.db.object_shape(ObjectShapeId(shape_id));
        shape.string_index.as_ref().map(|idx| idx.value_type)
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
        let types = self.db.type_list(crate::types::TypeListId(list_id));
        types.iter().find_map(|&t| self.visit_type(self.db, t))
    }

    fn visit_intersection(&mut self, list_id: u32) -> Self::Output {
        let types = self.db.type_list(crate::types::TypeListId(list_id));
        // For intersection, return the first one found
        types.first().and_then(|&t| self.visit_type(self.db, t))
    }

    fn visit_readonly_type(&mut self, inner_type: TypeId) -> Self::Output {
        self.visit_type(self.db, inner_type)
    }

    fn default_output() -> Self::Output {
        None
    }
}

/// Visitor for resolving number index signatures.
struct NumberIndexResolver<'a> {
    db: &'a dyn TypeDatabase,
}

impl<'a> TypeVisitor for NumberIndexResolver<'a> {
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

    fn visit_array(&mut self, element_type: TypeId) -> Self::Output {
        Some(element_type)
    }

    fn visit_tuple(&mut self, _list_id: u32) -> Self::Output {
        Some(TypeId::UNKNOWN)
    }

    fn visit_union(&mut self, list_id: u32) -> Self::Output {
        let types = self.db.type_list(crate::types::TypeListId(list_id));
        types.iter().find_map(|&t| self.visit_type(self.db, t))
    }

    fn visit_intersection(&mut self, list_id: u32) -> Self::Output {
        let types = self.db.type_list(crate::types::TypeListId(list_id));
        types.first().and_then(|&t| self.visit_type(self.db, t))
    }

    fn visit_readonly_type(&mut self, inner_type: TypeId) -> Self::Output {
        self.visit_type(self.db, inner_type)
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
    type Output = bool;

    fn visit_intrinsic(&mut self, _kind: crate::types::IntrinsicKind) -> Self::Output {
        false
    }

    fn visit_literal(&mut self, _value: &crate::LiteralValue) -> Self::Output {
        false
    }

    fn visit_array(&mut self, _element_type: TypeId) -> Self::Output {
        false // Arrays are mutable by default
    }

    fn visit_tuple(&mut self, _list_id: u32) -> Self::Output {
        false // Tuples are mutable by default
    }

    fn visit_object_with_index(&mut self, shape_id: u32) -> Self::Output {
        let shape = self.db.object_shape(ObjectShapeId(shape_id));
        match self.kind {
            IndexKind::String => shape.string_index.as_ref().is_some_and(|idx| idx.readonly),
            IndexKind::Number => shape.number_index.as_ref().is_some_and(|idx| idx.readonly),
        }
    }

    fn visit_union(&mut self, list_id: u32) -> Self::Output {
        let types = self.db.type_list(crate::types::TypeListId(list_id));
        // Union: any member being readonly makes it readonly
        types.iter().any(|&t| self.visit_type(self.db, t))
    }

    fn visit_intersection(&mut self, list_id: u32) -> Self::Output {
        let types = self.db.type_list(crate::types::TypeListId(list_id));
        // Intersection: all must be readonly
        types.iter().all(|&t| self.visit_type(self.db, t))
    }

    fn visit_readonly_type(&mut self, inner_type: TypeId) -> Self::Output {
        self.visit_type(self.db, inner_type)
    }

    fn visit_lazy(&mut self, def_id: u32) -> Self::Output {
        // Resolve lazy types (interfaces, classes, type aliases) before checking readonly
        let resolved = crate::evaluate::evaluate_type(self.db, TypeId(def_id));
        self.visit_type(self.db, resolved)
    }

    fn default_output() -> Self::Output {
        false
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
        }
    }

    fn visit_literal(&mut self, _value: &crate::LiteralValue) -> Self::Output {
        IndexInfo {
            string_index: None,
            number_index: None,
        }
    }

    fn visit_object_with_index(&mut self, shape_id: u32) -> Self::Output {
        let shape = self.db.object_shape(ObjectShapeId(shape_id));
        IndexInfo {
            string_index: shape.string_index.clone(),
            number_index: shape.number_index.clone(),
        }
    }

    fn visit_array(&mut self, elem: TypeId) -> Self::Output {
        IndexInfo {
            string_index: None,
            number_index: Some(IndexSignature {
                key_type: TypeId::NUMBER,
                value_type: elem,
                readonly: false,
            }),
        }
    }

    fn visit_tuple(&mut self, _list_id: u32) -> Self::Output {
        IndexInfo {
            string_index: None,
            number_index: Some(IndexSignature {
                key_type: TypeId::NUMBER,
                value_type: TypeId::UNKNOWN,
                readonly: false,
            }),
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
        info
    }

    fn visit_union(&mut self, _list_id: u32) -> Self::Output {
        // Complex logic, return empty for now
        IndexInfo {
            string_index: None,
            number_index: None,
        }
    }

    fn visit_intersection(&mut self, _list_id: u32) -> Self::Output {
        IndexInfo {
            string_index: None,
            number_index: None,
        }
    }

    fn visit_lazy(&mut self, def_id: u32) -> Self::Output {
        // Resolve lazy types (interfaces, classes, type aliases) before collecting index info
        let resolved = crate::evaluate::evaluate_type(self.db, TypeId(def_id));
        self.visit_type(self.db, resolved)
    }

    fn default_output() -> Self::Output {
        IndexInfo {
            string_index: None,
            number_index: None,
        }
    }
}

/// Resolver for index signature queries on types.
///
/// This struct provides a unified interface for querying index signatures
/// across different type representations (ObjectWithIndex, Union, etc.).
pub struct IndexSignatureResolver<'a> {
    db: &'a dyn TypeDatabase,
}

impl<'a> IndexSignatureResolver<'a> {
    /// Create a new index signature resolver.
    pub fn new(db: &'a dyn TypeDatabase) -> Self {
        Self { db }
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
        let mut visitor = StringIndexResolver { db: self.db };
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
        let mut visitor = NumberIndexResolver { db: self.db };
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
        visitor.visit_type(self.db, obj)
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
    /// This is a simplified check that returns `true` if the name starts
    /// with a digit. For more sophisticated numeric literal checking,
    /// use `is_numeric_property_name` from the utils module.
    ///
    /// ## Examples
    ///
    /// - `"0"` → `true`
    /// - `"42"` → `true`
    /// - `"foo"` → `false`
    /// - `"-1"` → `false` (starts with minus)
    pub fn is_numeric_index_name(&self, name: &str) -> bool {
        name.as_bytes()
            .first()
            .map(|b| b.is_ascii_digit())
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intern::TypeInterner;
    use crate::types::{ObjectFlags, ObjectShape};

    #[test]
    fn test_resolve_string_index() {
        let db = TypeInterner::new();

        // Object with string index
        let obj = db.object_with_index(ObjectShape {
            symbol: None,
            flags: ObjectFlags::empty(),
            properties: vec![],
            string_index: Some(IndexSignature {
                key_type: TypeId::STRING,
                value_type: TypeId::NUMBER,
                readonly: false,
            }),
            number_index: None,
        });

        let resolver = IndexSignatureResolver::new(&db);
        assert_eq!(resolver.resolve_string_index(obj), Some(TypeId::NUMBER));
        assert_eq!(resolver.resolve_number_index(obj), None);
    }

    #[test]
    fn test_resolve_number_index() {
        let db = TypeInterner::new();

        // Object with number index
        let obj = db.object_with_index(ObjectShape {
            symbol: None,
            flags: ObjectFlags::empty(),
            properties: vec![],
            string_index: None,
            number_index: Some(IndexSignature {
                key_type: TypeId::NUMBER,
                value_type: TypeId::STRING,
                readonly: false,
            }),
        });

        let resolver = IndexSignatureResolver::new(&db);
        assert_eq!(resolver.resolve_string_index(obj), None);
        assert_eq!(resolver.resolve_number_index(obj), Some(TypeId::STRING));
    }

    #[test]
    fn test_is_readonly() {
        let db = TypeInterner::new();

        // Readonly string index
        let obj1 = db.object_with_index(ObjectShape {
            symbol: None,
            flags: ObjectFlags::empty(),
            properties: vec![],
            string_index: Some(IndexSignature {
                key_type: TypeId::STRING,
                value_type: TypeId::NUMBER,
                readonly: true,
            }),
            number_index: None,
        });

        // Mutable string index
        let obj2 = db.object_with_index(ObjectShape {
            symbol: None,
            flags: ObjectFlags::empty(),
            properties: vec![],
            string_index: Some(IndexSignature {
                key_type: TypeId::STRING,
                value_type: TypeId::NUMBER,
                readonly: false,
            }),
            number_index: None,
        });

        let resolver = IndexSignatureResolver::new(&db);
        assert!(resolver.is_readonly(obj1, IndexKind::String));
        assert!(!resolver.is_readonly(obj2, IndexKind::String));
    }

    #[test]
    fn test_is_numeric_index_name() {
        let db = TypeInterner::new();
        let resolver = IndexSignatureResolver::new(&db);

        assert!(resolver.is_numeric_index_name("0"));
        assert!(resolver.is_numeric_index_name("42"));
        assert!(resolver.is_numeric_index_name("123"));
        assert!(!resolver.is_numeric_index_name("foo"));
        assert!(!resolver.is_numeric_index_name(""));
        assert!(!resolver.is_numeric_index_name("-1")); // Starts with minus
    }
}
