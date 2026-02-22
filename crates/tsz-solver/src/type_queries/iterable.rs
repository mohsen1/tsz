//! Iterable Type Classification
//!
//! Classification enums and functions for iterable types, used for spread handling,
//! `for-of` element type computation, and async iterable checking.

use crate::{TypeData, TypeDatabase, TypeId};

// =============================================================================
// Iterable Type Classification (Spread Handling)
// =============================================================================

/// Classification for iterable types (used for spread element handling).
#[derive(Debug, Clone)]
pub enum IterableTypeKind {
    /// Tuple type - elements can be expanded
    Tuple(Vec<crate::types::TupleElement>),
    /// Array type - element type for variadic handling
    Array(TypeId),
    /// Not a directly iterable type (caller should handle as-is)
    Other,
}

/// Classify a type for iterable/spread handling.
pub fn classify_iterable_type(db: &dyn TypeDatabase, type_id: TypeId) -> IterableTypeKind {
    let Some(key) = db.lookup(type_id) else {
        return IterableTypeKind::Other;
    };

    match key {
        TypeData::Tuple(tuple_id) => {
            let elements = db.tuple_list(tuple_id);
            IterableTypeKind::Tuple(elements.to_vec())
        }
        TypeData::Array(elem_type) => IterableTypeKind::Array(elem_type),
        _ => IterableTypeKind::Other,
    }
}

// =============================================================================
// Full Iterable Type Classification (For is_iterable_type checks)
// =============================================================================

/// Comprehensive classification for iterable type checking.
///
/// This enum is used by `is_iterable_type` and related functions to determine
/// if a type is iterable (has Symbol.iterator protocol) without directly
/// matching on `TypeData` in the checker layer.
#[derive(Debug, Clone)]
pub enum FullIterableTypeKind {
    /// Array type - always iterable
    Array(TypeId),
    /// Tuple type - always iterable
    Tuple(Vec<crate::types::TupleElement>),
    /// String literal - always iterable
    StringLiteral(tsz_common::interner::Atom),
    /// Union type - all members must be iterable
    Union(Vec<TypeId>),
    /// Intersection type - at least one member must be iterable
    Intersection(Vec<TypeId>),
    /// Object type - check for [Symbol.iterator] method
    Object(crate::types::ObjectShapeId),
    /// Application type (Set<T>, Map<K,V>, etc.) - check base type
    Application { base: TypeId },
    /// Type parameter - check constraint if present
    TypeParameter { constraint: Option<TypeId> },
    /// Readonly wrapper - check inner type
    Readonly(TypeId),
    /// Function or Callable - not iterable
    FunctionOrCallable,
    /// Index access, Conditional, Mapped - not directly iterable
    ComplexType,
    /// Unknown type - not iterable (or needs special handling)
    NotIterable,
}

/// Classify a type for full iterable checking.
///
/// This is used by `is_iterable_type` and related functions.
pub fn classify_full_iterable_type(db: &dyn TypeDatabase, type_id: TypeId) -> FullIterableTypeKind {
    let Some(key) = db.lookup(type_id) else {
        return FullIterableTypeKind::NotIterable;
    };

    match key {
        TypeData::Array(elem) => FullIterableTypeKind::Array(elem),
        TypeData::Tuple(tuple_id) => {
            let elements = db.tuple_list(tuple_id);
            FullIterableTypeKind::Tuple(elements.to_vec())
        }
        TypeData::Literal(crate::LiteralValue::String(s)) => FullIterableTypeKind::StringLiteral(s),
        TypeData::Union(members_id) => {
            let members = db.type_list(members_id);
            FullIterableTypeKind::Union(members.to_vec())
        }
        TypeData::Intersection(members_id) => {
            let members = db.type_list(members_id);
            FullIterableTypeKind::Intersection(members.to_vec())
        }
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            FullIterableTypeKind::Object(shape_id)
        }
        TypeData::Application(app_id) => {
            let app = db.type_application(app_id);
            FullIterableTypeKind::Application { base: app.base }
        }
        TypeData::TypeParameter(info) | TypeData::Infer(info) => {
            FullIterableTypeKind::TypeParameter {
                constraint: info.constraint,
            }
        }
        TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => {
            FullIterableTypeKind::Readonly(inner)
        }
        TypeData::Function(_) | TypeData::Callable(_) => FullIterableTypeKind::FunctionOrCallable,
        TypeData::IndexAccess(_, _) | TypeData::Conditional(_) | TypeData::Mapped(_) => {
            FullIterableTypeKind::ComplexType
        }
        // All other types are not directly iterable
        TypeData::BoundParameter(_)
        | TypeData::Intrinsic(_)
        | TypeData::Literal(_)
        | TypeData::Lazy(_)
        | TypeData::Recursive(_)
        | TypeData::TemplateLiteral(_)
        | TypeData::UniqueSymbol(_)
        | TypeData::ThisType
        | TypeData::TypeQuery(_)
        | TypeData::KeyOf(_)
        | TypeData::StringIntrinsic { .. }
        | TypeData::ModuleNamespace(_)
        | TypeData::Enum(_, _)
        | TypeData::Error => FullIterableTypeKind::NotIterable,
    }
}

/// Classification for async iterable type checking.
#[derive(Debug, Clone)]
pub enum AsyncIterableTypeKind {
    /// Union type - all members must be async iterable
    Union(Vec<TypeId>),
    /// Object type - check for [Symbol.asyncIterator] method
    Object(crate::types::ObjectShapeId),
    /// Readonly wrapper - check inner type
    Readonly(TypeId),
    /// Not async iterable
    NotAsyncIterable,
}

/// Classify a type for async iterable checking.
pub fn classify_async_iterable_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> AsyncIterableTypeKind {
    let Some(key) = db.lookup(type_id) else {
        return AsyncIterableTypeKind::NotAsyncIterable;
    };

    match key {
        TypeData::Union(members_id) => {
            let members = db.type_list(members_id);
            AsyncIterableTypeKind::Union(members.to_vec())
        }
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            AsyncIterableTypeKind::Object(shape_id)
        }
        TypeData::ReadonlyType(inner) => AsyncIterableTypeKind::Readonly(inner),
        _ => AsyncIterableTypeKind::NotAsyncIterable,
    }
}

/// Classification for for-of element type computation.
#[derive(Debug, Clone)]
pub enum ForOfElementKind {
    /// Array type - element is the array element type
    Array(TypeId),
    /// Tuple type - element is union of tuple element types
    Tuple(Vec<crate::types::TupleElement>),
    /// Union type - compute element type for each member
    Union(Vec<TypeId>),
    /// Readonly wrapper - unwrap and compute
    Readonly(TypeId),
    /// String type - iteration yields string
    String,
    /// Other types - resolve via iterator protocol or return ANY as fallback
    Other,
}

/// Classify a type for for-of element type computation.
pub fn classify_for_of_element_type(db: &dyn TypeDatabase, type_id: TypeId) -> ForOfElementKind {
    let Some(key) = db.lookup(type_id) else {
        return ForOfElementKind::Other;
    };

    match key {
        TypeData::Array(elem) => ForOfElementKind::Array(elem),
        TypeData::Tuple(tuple_id) => {
            let elements = db.tuple_list(tuple_id);
            ForOfElementKind::Tuple(elements.to_vec())
        }
        TypeData::Union(members_id) => {
            let members = db.type_list(members_id);
            ForOfElementKind::Union(members.to_vec())
        }
        TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => {
            ForOfElementKind::Readonly(inner)
        }
        // String literals iterate to produce `string`
        TypeData::Literal(crate::LiteralValue::String(_)) => ForOfElementKind::String,
        _ => ForOfElementKind::Other,
    }
}
