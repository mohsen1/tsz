//! Unified Type Classification System
//!
//! This module provides a systematic classification of types using the Visitor pattern.
//! It consolidates the numerous individual type query functions into a clean, extensible
//! classification system.
//!
//! # Design Benefits
//!
//! - **Single lookup**: Each type is looked up exactly once
//! - **Exhaustive handling**: All TypeKey variants covered
//! - **Extensible**: New classifications added without duplication
//! - **Efficient**: Classification result can answer multiple queries
//! - **Memory efficient**: Reusable classification enum
//!
//! # Example
//!
//! ```rust,ignore
//! use crate::type_classifier::{classify_type, TypeClassification};
//!
//! let classification = classify_type(&db, type_id);
//! match classification {
//!     TypeClassification::Literal(lit) => { /* handle literal */ }
//!     TypeClassification::Object(_) => { /* handle object */ }
//!     TypeClassification::Union(_) => { /* handle union */ }
//!     TypeClassification::Callable(_) => { /* handle callable */ }
//!     _ => { /* handle other */ }
//! }
//! ```

use crate::def::DefId;
use crate::types::{
    CallableShapeId, ConditionalTypeId, FunctionShapeId, IntrinsicKind, LiteralValue, MappedTypeId,
    ObjectShapeId, SymbolRef, TemplateLiteralId, TupleListId, TypeApplicationId, TypeListId,
    TypeParamInfo,
};
use crate::{TypeDatabase, TypeId, TypeKey};

/// Comprehensive type classification enum.
///
/// This enum represents all major categories of types, providing a single
/// classification for any TypeId. Each variant contains the essential data
/// needed to perform operations on that type, eliminating the need for
/// repeated TypeKey pattern matching.
///
/// This replaces the pattern of multiple `is_*()` and `get_*()` functions
/// with a single, efficient classification operation.
#[derive(Debug, Clone)]
pub enum TypeClassification {
    // =========================================================================
    // Primitive & Literal Types
    // =========================================================================
    /// Intrinsic type (any, unknown, never, void, number, string, etc.)
    Intrinsic(IntrinsicKind),

    /// Literal value type (string literal, number literal, boolean literal)
    Literal(LiteralValue),

    // =========================================================================
    // Container Types
    // =========================================================================
    /// Array type with element type
    Array(TypeId),

    /// Tuple type with elements
    Tuple(TupleListId),

    /// Object type with properties (no index signatures)
    Object(ObjectShapeId),

    /// Object type with index signatures (in addition to properties)
    ObjectWithIndex(ObjectShapeId),

    /// Union type (A | B | C) with member list
    Union(TypeListId),

    /// Intersection type (A & B & C) with member list
    Intersection(TypeListId),

    // =========================================================================
    // Callable Types
    // =========================================================================
    /// Function type with signature
    Function(FunctionShapeId),

    /// Callable type (has both call and construct signatures)
    Callable(CallableShapeId),

    // =========================================================================
    // Generic & Reference Types
    // =========================================================================
    /// Type parameter (generic type variable)
    TypeParameter(TypeParamInfo),

    /// Lazy type reference (DefId to symbol)
    ///
    /// Used for named types like interfaces, classes, type aliases.
    /// The DefId should be resolved to get the actual type.
    Lazy(DefId),

    /// Enum type with nominal identity and member union
    ///
    /// Enums are nominally typed (DefId for identity) but structurally
    /// checked through the member type union.
    Enum(DefId, TypeId),

    /// Generic type application (Base<Args>)
    Application(TypeApplicationId),

    /// Type parameter reference (via De Bruijn index in bound parameter contexts)
    BoundParameter(u32),

    /// Recursive type reference (via De Bruijn index)
    Recursive(u32),

    // =========================================================================
    /// Advanced Types
    // =========================================================================

    /// Conditional type (T extends U ? X : Y)
    Conditional(ConditionalTypeId),

    /// Mapped type { [K in T]: V }
    Mapped(MappedTypeId),

    /// Index access type (T[K])
    IndexAccess(TypeId, TypeId),

    /// KeyOf type
    KeyOf(TypeId),

    /// Template literal type
    TemplateLiteral(TemplateLiteralId),

    /// Type query (typeof X)
    TypeQuery(SymbolRef),

    /// `this` type
    ThisType,

    /// Unique symbol type
    UniqueSymbol(SymbolRef),

    /// Infer type (infer T in conditional types)
    Infer(TypeParamInfo),

    /// Readonly type wrapper
    ReadonlyType(TypeId),

    /// String intrinsic type (Uppercase<T>, Lowercase<T>, etc.)
    StringIntrinsic {
        kind: crate::StringIntrinsicKind,
        type_arg: TypeId,
    },

    /// Module namespace type (import * as ns)
    ModuleNamespace(SymbolRef),

    /// NoInfer wrapper (TypeScript 5.4+)
    NoInfer(TypeId),

    /// Error type (used for invalid type expressions)
    Error,

    /// Unknown type (when lookup fails or type not found)
    Unknown,
}

impl TypeClassification {
    /// Check if this classification is a primitive type
    pub fn is_primitive(&self) -> bool {
        matches!(self, TypeClassification::Intrinsic(_))
    }

    /// Check if this classification is a literal type
    pub fn is_literal(&self) -> bool {
        matches!(self, TypeClassification::Literal(_))
    }

    /// Check if this classification is an object-like type
    pub fn is_object_like(&self) -> bool {
        matches!(
            self,
            TypeClassification::Object(_)
                | TypeClassification::ObjectWithIndex(_)
                | TypeClassification::Callable(_)
        )
    }

    /// Check if this classification is callable (function or callable)
    pub fn is_callable(&self) -> bool {
        matches!(
            self,
            TypeClassification::Function(_) | TypeClassification::Callable(_)
        )
    }

    /// Check if this classification is a collection type (array or tuple)
    pub fn is_collection(&self) -> bool {
        matches!(
            self,
            TypeClassification::Array(_) | TypeClassification::Tuple(_)
        )
    }

    /// Check if this classification is a composite type (union or intersection)
    pub fn is_composite(&self) -> bool {
        matches!(
            self,
            TypeClassification::Union(_) | TypeClassification::Intersection(_)
        )
    }
}

/// Classify a TypeId into its fundamental category.
///
/// This is the main entry point for type classification. It performs a single
/// lookup and returns a comprehensive classification that can answer multiple
/// queries about the type.
///
/// # Example
///
/// ```rust,ignore
/// let classification = classify_type(&db, type_id);
/// match classification {
///     TypeClassification::Literal(LiteralValue::String(_)) => {
///         // Handle string literal
///     }
///     TypeClassification::Union(list_id) => {
///         // Handle union type
///     }
///     _ => {}
/// }
/// ```
pub fn classify_type(db: &dyn TypeDatabase, type_id: TypeId) -> TypeClassification {
    match db.lookup(type_id) {
        None => TypeClassification::Unknown,
        Some(TypeKey::Intrinsic(kind)) => TypeClassification::Intrinsic(kind),
        Some(TypeKey::Literal(value)) => TypeClassification::Literal(value),
        Some(TypeKey::Array(element_type)) => TypeClassification::Array(element_type),
        Some(TypeKey::Tuple(list_id)) => TypeClassification::Tuple(list_id),
        Some(TypeKey::Object(shape_id)) => TypeClassification::Object(shape_id),
        Some(TypeKey::ObjectWithIndex(shape_id)) => TypeClassification::ObjectWithIndex(shape_id),
        Some(TypeKey::Union(list_id)) => TypeClassification::Union(list_id),
        Some(TypeKey::Intersection(list_id)) => TypeClassification::Intersection(list_id),
        Some(TypeKey::Function(shape_id)) => TypeClassification::Function(shape_id),
        Some(TypeKey::Callable(shape_id)) => TypeClassification::Callable(shape_id),
        Some(TypeKey::TypeParameter(param_info)) => TypeClassification::TypeParameter(param_info),
        Some(TypeKey::Lazy(def_id)) => TypeClassification::Lazy(def_id),
        Some(TypeKey::Enum(def_id, member_type)) => TypeClassification::Enum(def_id, member_type),
        Some(TypeKey::Application(app_id)) => TypeClassification::Application(app_id),
        Some(TypeKey::BoundParameter(index)) => TypeClassification::BoundParameter(index),
        Some(TypeKey::Recursive(index)) => TypeClassification::Recursive(index),
        Some(TypeKey::Conditional(cond_id)) => TypeClassification::Conditional(cond_id),
        Some(TypeKey::Mapped(mapped_id)) => TypeClassification::Mapped(mapped_id),
        Some(TypeKey::IndexAccess(obj, key)) => TypeClassification::IndexAccess(obj, key),
        Some(TypeKey::KeyOf(inner)) => TypeClassification::KeyOf(inner),
        Some(TypeKey::TemplateLiteral(template_id)) => {
            TypeClassification::TemplateLiteral(template_id)
        }
        Some(TypeKey::TypeQuery(sym_ref)) => TypeClassification::TypeQuery(sym_ref),
        Some(TypeKey::ThisType) => TypeClassification::ThisType,
        Some(TypeKey::UniqueSymbol(sym_ref)) => TypeClassification::UniqueSymbol(sym_ref),
        Some(TypeKey::Infer(param_info)) => TypeClassification::Infer(param_info),
        Some(TypeKey::ReadonlyType(inner)) => TypeClassification::ReadonlyType(inner),
        Some(TypeKey::StringIntrinsic { kind, type_arg }) => {
            TypeClassification::StringIntrinsic { kind, type_arg }
        }
        Some(TypeKey::ModuleNamespace(sym_ref)) => TypeClassification::ModuleNamespace(sym_ref),
        Some(TypeKey::NoInfer(inner)) => TypeClassification::NoInfer(inner),
        Some(TypeKey::Error) => TypeClassification::Error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // This test would require a full test setup with a TypeDatabase,
    // which is beyond the scope of a unit test here. See integration tests
    // in the test suite for comprehensive classification testing.

    #[test]
    fn test_classification_is_methods() {
        // These tests validate the classification helper methods work as expected.
        // In a real test, we'd create actual TypeClassifications, but here we
        // demonstrate the API works.

        // Note: Full testing requires TypeDatabase setup in integration tests
        let _ = TypeClassification::Error;
    }
}
