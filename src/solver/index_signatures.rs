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
//! use crate::solver::index_signatures::IndexSignatureResolver;
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

use crate::solver::types::{IndexInfo, IndexSignature};
use crate::solver::{TypeDatabase, TypeId, TypeKey};

/// Distinguishes between string and numeric index signatures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexKind {
    /// String index signature: `{ [key: string]: T }`
    String,
    /// Numeric index signature: `{ [key: number]: T }`
    Number,
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
        match self.db.lookup(obj) {
            Some(TypeKey::ObjectWithIndex(shape_id)) => {
                let shape = self.db.object_shape(shape_id);
                shape.string_index.as_ref().map(|idx| idx.value_type)
            }
            // Array/tuple types have readonly numeric index (which also supports string)
            Some(TypeKey::Array(elem)) => Some(elem),
            Some(TypeKey::Tuple(_)) => Some(TypeId::UNKNOWN), // Would need union of all elements
            // Readonly wrapper: unwrap and check inner type
            Some(TypeKey::ReadonlyType(inner)) => self.resolve_string_index(inner),
            // Union: any member with string index makes it valid
            Some(TypeKey::Union(types)) => {
                let types = self.db.type_list(types);
                types.iter().find_map(|&t| self.resolve_string_index(t))
            }
            // Intersection: all members must agree
            Some(TypeKey::Intersection(types)) => {
                let types = self.db.type_list(types);
                let first = types.first().and_then(|&t| self.resolve_string_index(t));
                // For intersection, we need all members to agree
                // For simplicity, return the first one found
                first
            }
            _ => None,
        }
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
        match self.db.lookup(obj) {
            Some(TypeKey::ObjectWithIndex(shape_id)) => {
                let shape = self.db.object_shape(shape_id);
                shape.number_index.as_ref().map(|idx| idx.value_type)
            }
            // Array/tuple types have readonly numeric index
            Some(TypeKey::Array(elem)) => Some(elem),
            Some(TypeKey::Tuple(_)) => Some(TypeId::UNKNOWN), // Would need union of all elements
            // Readonly wrapper: unwrap and check inner type
            Some(TypeKey::ReadonlyType(inner)) => self.resolve_number_index(inner),
            // Union: any member with number index makes it valid
            Some(TypeKey::Union(types)) => {
                let types = self.db.type_list(types);
                types.iter().find_map(|&t| self.resolve_number_index(t))
            }
            // Intersection: all members must agree
            Some(TypeKey::Intersection(types)) => {
                let types = self.db.type_list(types);
                let first = types.first().and_then(|&t| self.resolve_number_index(t));
                // For simplicity, return the first one found
                first
            }
            _ => None,
        }
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
        match self.db.lookup(obj) {
            // Array/tuple types have readonly index
            Some(TypeKey::Array(_) | TypeKey::Tuple(_)) => {
                match kind {
                    IndexKind::Number => true,
                    IndexKind::String => true, // Arrays support string indexing too
                }
            }
            Some(TypeKey::ObjectWithIndex(shape_id)) => {
                let shape = self.db.object_shape(shape_id);
                match kind {
                    IndexKind::String => {
                        shape.string_index.as_ref().is_some_and(|idx| idx.readonly)
                    }
                    IndexKind::Number => {
                        shape.number_index.as_ref().is_some_and(|idx| idx.readonly)
                    }
                }
            }
            // Readonly wrapper: unwrap and check inner type
            Some(TypeKey::ReadonlyType(inner)) => self.is_readonly(inner, kind),
            // Union: any member being readonly makes it readonly
            Some(TypeKey::Union(types)) => {
                let types = self.db.type_list(types);
                types.iter().any(|&t| self.is_readonly(t, kind))
            }
            // Intersection: all must be readonly
            Some(TypeKey::Intersection(types)) => {
                let types = self.db.type_list(types);
                types.iter().all(|&t| self.is_readonly(t, kind))
            }
            _ => false,
        }
    }

    /// Get all index signatures from a type.
    ///
    /// Returns an `IndexInfo` struct containing both string and numeric
    /// index signatures if present.
    pub fn get_index_info(&self, obj: TypeId) -> IndexInfo {
        match self.db.lookup(obj) {
            Some(TypeKey::ObjectWithIndex(shape_id)) => {
                let shape = self.db.object_shape(shape_id);
                IndexInfo {
                    string_index: shape.string_index.clone(),
                    number_index: shape.number_index.clone(),
                }
            }
            // Array/tuple have readonly numeric index
            Some(TypeKey::Array(elem)) => IndexInfo {
                string_index: None,
                number_index: Some(IndexSignature {
                    key_type: TypeId::NUMBER,
                    value_type: elem,
                    readonly: true,
                }),
            },
            Some(TypeKey::Tuple(_)) => {
                // Tuples have readonly numeric index
                // The element type would be a union of all tuple elements
                // For simplicity, return unknown
                IndexInfo {
                    string_index: None,
                    number_index: Some(IndexSignature {
                        key_type: TypeId::NUMBER,
                        value_type: TypeId::UNKNOWN,
                        readonly: true,
                    }),
                }
            }
            // Readonly wrapper: unwrap and get info
            Some(TypeKey::ReadonlyType(inner)) => {
                let mut info = self.get_index_info(inner);
                // Mark all signatures as readonly
                if let Some(idx) = &mut info.string_index {
                    idx.readonly = true;
                }
                if let Some(idx) = &mut info.number_index {
                    idx.readonly = true;
                }
                info
            }
            // Union/Intersection: complex logic, return empty for now
            Some(TypeKey::Union(_) | TypeKey::Intersection(_)) => IndexInfo {
                string_index: None,
                number_index: None,
            },
            _ => IndexInfo {
                string_index: None,
                number_index: None,
            },
        }
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
    use crate::solver::intern::TypeInterner;
    use crate::solver::types::ObjectShape;

    #[test]
    fn test_resolve_string_index() {
        let db = TypeInterner::new();

        // Object with string index
        let obj = db.object_with_index(ObjectShape {
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
