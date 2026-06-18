//! Constraint-unwrapping type predicate helpers.

use crate::construction::TypeDatabase;
use crate::types::ObjectShapeId;
use crate::{TypeData, TypeId};

/// Check if a type is a literal type (`TypeDatabase` version).
pub fn is_literal_type_through_type_constraints(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    LiteralTypeChecker::check(types, type_id)
}

/// Check if a type is a function type (`TypeDatabase` version).
pub fn is_function_type_through_type_constraints(
    types: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    FunctionTypeChecker::check(types, type_id)
}

/// Check if a type is object-like (`TypeDatabase` version).
pub fn is_object_like_type_through_type_constraints(
    types: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    ObjectTypeChecker::check(types, type_id)
}

/// Check if a type is an empty object type (`TypeDatabase` version).
pub fn is_empty_object_type_through_type_constraints(
    types: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    let checker = EmptyObjectChecker::new(types);
    checker.check(type_id)
}

/// Classification of object types for freshness tracking.
pub enum ObjectTypeKind {
    /// A regular object type (no index signatures).
    Object(ObjectShapeId),
    /// An object type with index signatures.
    ObjectWithIndex(ObjectShapeId),
    /// Not an object type.
    NotObject,
}

/// Classify a type as an object type kind.
///
/// This is used by the freshness tracking system to determine if a type
/// is a fresh object literal that needs special handling.
pub fn classify_object_type(types: &dyn TypeDatabase, type_id: TypeId) -> ObjectTypeKind {
    if type_id.is_intrinsic() {
        return ObjectTypeKind::NotObject;
    }
    match types.lookup(type_id) {
        Some(TypeData::Object(shape_id)) => ObjectTypeKind::Object(shape_id),
        Some(TypeData::ObjectWithIndex(shape_id)) => ObjectTypeKind::ObjectWithIndex(shape_id),
        _ => ObjectTypeKind::NotObject,
    }
}

/// Visitor to check if a type is a literal type.
struct LiteralTypeChecker;

impl LiteralTypeChecker {
    fn check(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
        // Fast path: intrinsic types are never literal types EXCEPT for
        // `BOOLEAN_TRUE` (14) and `BOOLEAN_FALSE` (15) which are reserved
        // intrinsic IDs for the `true` / `false` literal types. All other
        // intrinsic IDs match no arm and fall through to `_ => false`.
        // `is_intrinsic()` is a free `TypeId`-range check; the explicit
        // exception preserves slow-path behaviour without `TypeData`
        // lookup. Same family as #2001 / #2005 / #2008 / #2009 / #2014
        // / #2019 / #2025.
        if type_id.is_intrinsic() {
            return type_id == TypeId::BOOLEAN_TRUE || type_id == TypeId::BOOLEAN_FALSE;
        }
        match types.lookup(type_id) {
            Some(TypeData::Literal(_)) => true,
            Some(TypeData::Enum(_, structural_type)) => Self::check(types, structural_type),
            Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => {
                Self::check(types, inner)
            }
            Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => {
                info.constraint.is_some_and(|c| Self::check(types, c))
            }
            _ => false,
        }
    }
}

/// Visitor to check if a type is a function type.
struct FunctionTypeChecker;

impl FunctionTypeChecker {
    fn check(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
        // Fast path: intrinsic types match no arm. Skip lookup + dispatch.
        // Same family as #2001 / #2005 / #2008 / #2009 / #2014 / #2019 / #2025 / #2032.
        if type_id.is_intrinsic() {
            return false;
        }
        match types.lookup(type_id) {
            Some(TypeData::Function(_) | TypeData::Callable(_)) => true,
            Some(TypeData::Intersection(members)) => {
                let members = types.type_list(members);
                members.iter().any(|&member| Self::check(types, member))
            }
            Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => {
                info.constraint.is_some_and(|c| Self::check(types, c))
            }
            // The global `Function` interface is typeof "function" at runtime.
            // Check if this Lazy type is the known boxed Function type.
            Some(TypeData::Lazy(def_id)) => {
                types.is_boxed_def_id(def_id, crate::types::IntrinsicKind::Function)
            }
            _ => false,
        }
    }
}

/// Visitor to check if a type is object-like.
struct ObjectTypeChecker;

impl ObjectTypeChecker {
    fn check(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
        // Fast path: the only object-like intrinsics are the `object`
        // (non-primitive) and `Function` types; every other intrinsic
        // (`string`, `number`, `never`, ...) matches no arm below. This mirrors
        // `is_object_like_type_impl`, whose intrinsic fast-path also admits
        // `OBJECT`/`FUNCTION`. Without this, an intersection that carries the
        // `object` intrinsic member (e.g. `object & Record<"k", unknown>` from
        // `in`-operator narrowing) was judged NOT object-like, so a follow-up
        // `typeof x === "object"` guard narrowed it to `never`.
        if type_id.is_intrinsic() {
            return type_id == TypeId::OBJECT || type_id == TypeId::FUNCTION;
        }
        match types.lookup(type_id) {
            Some(
                TypeData::Object(_)
                | TypeData::ObjectWithIndex(_)
                | TypeData::Array(_)
                | TypeData::Tuple(_)
                | TypeData::Mapped(_)
                | TypeData::Application(_),
            ) => true,
            Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => {
                Self::check(types, inner)
            }
            Some(TypeData::Intersection(members)) => {
                let members = types.type_list(members);
                members.iter().all(|&member| Self::check(types, member))
            }
            Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => info
                .constraint
                .is_some_and(|constraint| Self::check(types, constraint)),
            // Lazy types represent unresolved type references (interfaces, classes).
            // Most are object-like at runtime (interfaces/classes), but the global
            // `Function` interface is typeof "function". Check if this Lazy type
            // is the known boxed Function -- if so, it's NOT object-like.
            Some(TypeData::Lazy(def_id)) => {
                !types.is_boxed_def_id(def_id, crate::types::IntrinsicKind::Function)
            }
            _ => false,
        }
    }
}

/// Visitor to check if a type is an empty object type.
struct EmptyObjectChecker<'a> {
    db: &'a dyn TypeDatabase,
}

impl<'a> EmptyObjectChecker<'a> {
    fn new(db: &'a dyn TypeDatabase) -> Self {
        Self { db }
    }

    fn check(&self, type_id: TypeId) -> bool {
        if type_id.is_intrinsic() {
            return false;
        }
        match self.db.lookup(type_id) {
            Some(TypeData::Object(shape_id)) => {
                let shape = self.db.object_shape(shape_id);
                shape.properties.is_empty()
            }
            Some(TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.db.object_shape(shape_id);
                shape.properties.is_empty()
                    && shape.string_index.is_none()
                    && shape.number_index.is_none()
            }
            Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => self.check(inner),
            Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => {
                info.constraint.is_some_and(|c| self.check(c))
            }
            _ => false,
        }
    }
}
