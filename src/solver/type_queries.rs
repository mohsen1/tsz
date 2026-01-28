//! Type Query Functions
//!
//! This module provides high-level query functions for inspecting type characteristics.
//! These functions abstract away the internal TypeKey representation and provide
//! a stable API for the checker to query type properties.
//!
//! # Design Principles
//!
//! - **Abstraction**: Checker code should use these functions instead of matching on TypeKey
//! - **TypeDatabase-based**: All queries work through the TypeDatabase trait
//! - **Comprehensive**: Covers all common type checking scenarios
//! - **Efficient**: Simple lookups with minimal overhead
//!
//! # Usage
//!
//! ```rust
//! use crate::solver::type_queries::*;
//!
//! // Check if a type is callable
//! if is_callable_type(&db, type_id) {
//!     // Handle callable type
//! }
//!
//! // Check if a type is a tuple
//! if is_tuple_type(&db, type_id) {
//!     // Handle tuple type
//! }
//! ```

use crate::solver::{TypeDatabase, TypeId, TypeKey};

// =============================================================================
// Core Type Queries
// =============================================================================

/// Check if a type is a callable type (function or callable with signatures).
///
/// Returns true for TypeKey::Callable and TypeKey::Function types.
pub fn is_callable_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        db.lookup(type_id),
        Some(TypeKey::Callable(_) | TypeKey::Function(_))
    )
}

/// Check if a type is a tuple type.
///
/// Returns true for TypeKey::Tuple.
pub fn is_tuple_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(db.lookup(type_id), Some(TypeKey::Tuple(_)))
}

/// Check if a type is a union type (A | B).
///
/// Returns true for TypeKey::Union.
pub fn is_union_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(db.lookup(type_id), Some(TypeKey::Union(_)))
}

/// Check if a type is an intersection type (A & B).
///
/// Returns true for TypeKey::Intersection.
pub fn is_intersection_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(db.lookup(type_id), Some(TypeKey::Intersection(_)))
}

/// Check if a type is an object type (with or without index signatures).
///
/// Returns true for TypeKey::Object and TypeKey::ObjectWithIndex.
pub fn is_object_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        db.lookup(type_id),
        Some(TypeKey::Object(_) | TypeKey::ObjectWithIndex(_))
    )
}

/// Check if a type is an array type (T[]).
///
/// Returns true for TypeKey::Array.
pub fn is_array_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(db.lookup(type_id), Some(TypeKey::Array(_)))
}

/// Check if a type is a literal type (specific value).
///
/// Returns true for TypeKey::Literal.
pub fn is_literal_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(db.lookup(type_id), Some(TypeKey::Literal(_)))
}

/// Check if a type is a generic type application (Base<Args>).
///
/// Returns true for TypeKey::Application.
pub fn is_generic_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(db.lookup(type_id), Some(TypeKey::Application(_)))
}

/// Check if a type is a named type reference.
///
/// Returns true for TypeKey::Ref (interfaces, classes, type aliases).
pub fn is_type_reference(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(db.lookup(type_id), Some(TypeKey::Ref(_)))
}

/// Check if a type is a conditional type (T extends U ? X : Y).
///
/// Returns true for TypeKey::Conditional.
pub fn is_conditional_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(db.lookup(type_id), Some(TypeKey::Conditional(_)))
}

/// Check if a type is a mapped type ({ [K in Keys]: V }).
///
/// Returns true for TypeKey::Mapped.
pub fn is_mapped_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(db.lookup(type_id), Some(TypeKey::Mapped(_)))
}

/// Check if a type is a template literal type (`hello${T}world`).
///
/// Returns true for TypeKey::TemplateLiteral.
pub fn is_template_literal_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(db.lookup(type_id), Some(TypeKey::TemplateLiteral(_)))
}

/// Check if a type is a type parameter or infer type.
///
/// Returns true for TypeKey::TypeParameter and TypeKey::Infer.
pub fn is_type_parameter(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        db.lookup(type_id),
        Some(TypeKey::TypeParameter(_) | TypeKey::Infer(_))
    )
}

/// Check if a type is an index access type (T[K]).
///
/// Returns true for TypeKey::IndexAccess.
pub fn is_index_access_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(db.lookup(type_id), Some(TypeKey::IndexAccess(_, _)))
}

/// Check if a type is a keyof type.
///
/// Returns true for TypeKey::KeyOf.
pub fn is_keyof_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(db.lookup(type_id), Some(TypeKey::KeyOf(_)))
}

/// Check if a type is a type query (typeof expr).
///
/// Returns true for TypeKey::TypeQuery.
pub fn is_type_query(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(db.lookup(type_id), Some(TypeKey::TypeQuery(_)))
}

/// Check if a type is a readonly type modifier.
///
/// Returns true for TypeKey::ReadonlyType.
pub fn is_readonly_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(db.lookup(type_id), Some(TypeKey::ReadonlyType(_)))
}

/// Check if a type is a unique symbol type.
///
/// Returns true for TypeKey::UniqueSymbol.
pub fn is_unique_symbol_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(db.lookup(type_id), Some(TypeKey::UniqueSymbol(_)))
}

/// Check if a type is the this type.
///
/// Returns true for TypeKey::ThisType.
pub fn is_this_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(db.lookup(type_id), Some(TypeKey::ThisType))
}

/// Check if a type is an error type.
///
/// Returns true for TypeKey::Error or TypeId::ERROR.
pub fn is_error_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    type_id == TypeId::ERROR || matches!(db.lookup(type_id), Some(TypeKey::Error))
}

/// Check if a type is an intrinsic type (any, unknown, never, void, etc.).
///
/// Returns true for TypeKey::Intrinsic.
pub fn is_intrinsic_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(db.lookup(type_id), Some(TypeKey::Intrinsic(_)))
}

/// Check if a type is a primitive type (intrinsic or literal).
///
/// Returns true for intrinsic types and literal types.
pub fn is_primitive_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    // Check well-known intrinsic TypeIds first
    if type_id.is_intrinsic() {
        return true;
    }
    matches!(
        db.lookup(type_id),
        Some(TypeKey::Intrinsic(_)) | Some(TypeKey::Literal(_))
    )
}

// =============================================================================
// Composite Type Queries
// =============================================================================

/// Check if a type is an object-like type suitable for typeof "object".
///
/// Returns true for: Object, ObjectWithIndex, Array, Tuple, Mapped
pub fn is_object_like_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    is_object_like_type_impl(db, type_id)
}

fn is_object_like_type_impl(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    match db.lookup(type_id) {
        Some(TypeKey::Object(_))
        | Some(TypeKey::ObjectWithIndex(_))
        | Some(TypeKey::Array(_))
        | Some(TypeKey::Tuple(_))
        | Some(TypeKey::Mapped(_)) => true,
        Some(TypeKey::ReadonlyType(inner)) => is_object_like_type_impl(db, inner),
        Some(TypeKey::Intersection(members)) => {
            let members = db.type_list(members);
            members
                .iter()
                .all(|&member| is_object_like_type_impl(db, member))
        }
        Some(TypeKey::TypeParameter(info) | TypeKey::Infer(info)) => info
            .constraint
            .map(|constraint| is_object_like_type_impl(db, constraint))
            .unwrap_or(false),
        _ => false,
    }
}

/// Check if a type is a function type (Function or Callable).
///
/// This also handles intersections containing function types.
pub fn is_function_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    is_function_type_impl(db, type_id)
}

fn is_function_type_impl(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    match db.lookup(type_id) {
        Some(TypeKey::Function(_) | TypeKey::Callable(_)) => true,
        Some(TypeKey::Intersection(members)) => {
            let members = db.type_list(members);
            members
                .iter()
                .any(|&member| is_function_type_impl(db, member))
        }
        _ => false,
    }
}

/// Check if a type is an empty object type (no properties, no index signatures).
pub fn is_empty_object_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    match db.lookup(type_id) {
        Some(TypeKey::Object(shape_id)) => {
            let shape = db.object_shape(shape_id);
            shape.properties.is_empty()
        }
        Some(TypeKey::ObjectWithIndex(shape_id)) => {
            let shape = db.object_shape(shape_id);
            shape.properties.is_empty()
                && shape.string_index.is_none()
                && shape.number_index.is_none()
        }
        _ => false,
    }
}

// =============================================================================
// Type Content Queries
// =============================================================================

/// Check if a type contains any type parameters (TypeDatabase version).
///
/// This is a TypeDatabase-based alternative to visitor::contains_type_parameters.
pub fn contains_type_parameters_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    contains_type_matching_impl(db, type_id, |key| {
        matches!(key, TypeKey::TypeParameter(_) | TypeKey::Infer(_))
    })
}

/// Check if a type contains any `infer` types (TypeDatabase version).
///
/// This is a TypeDatabase-based alternative to visitor::contains_infer_types.
pub fn contains_infer_types_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    contains_type_matching_impl(db, type_id, |key| matches!(key, TypeKey::Infer(_)))
}

/// Check if a type contains the error type (TypeDatabase version).
///
/// This is a TypeDatabase-based alternative to visitor::contains_error_type.
pub fn contains_error_type_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id == TypeId::ERROR {
        return true;
    }
    contains_type_matching_impl(db, type_id, |key| matches!(key, TypeKey::Error))
}

/// Check if a type contains any type matching a predicate.
fn contains_type_matching_impl<F>(db: &dyn TypeDatabase, type_id: TypeId, predicate: F) -> bool
where
    F: Fn(&TypeKey) -> bool + Copy,
{
    let mut checker = ContainsTypeChecker {
        db,
        predicate,
        visiting: rustc_hash::FxHashSet::default(),
        max_depth: 20,
        current_depth: 0,
    };
    checker.check(type_id)
}

struct ContainsTypeChecker<'a, F>
where
    F: Fn(&TypeKey) -> bool,
{
    db: &'a dyn TypeDatabase,
    predicate: F,
    visiting: rustc_hash::FxHashSet<TypeId>,
    max_depth: usize,
    current_depth: usize,
}

impl<'a, F> ContainsTypeChecker<'a, F>
where
    F: Fn(&TypeKey) -> bool,
{
    fn check(&mut self, type_id: TypeId) -> bool {
        if self.current_depth >= self.max_depth {
            return false;
        }
        if self.visiting.contains(&type_id) {
            return false;
        }

        let Some(key) = self.db.lookup(type_id) else {
            return false;
        };

        if (self.predicate)(&key) {
            return true;
        }

        self.visiting.insert(type_id);
        self.current_depth += 1;

        let result = self.check_key(&key);

        self.current_depth -= 1;
        self.visiting.remove(&type_id);

        result
    }

    fn check_key(&mut self, key: &TypeKey) -> bool {
        match key {
            TypeKey::Intrinsic(_) | TypeKey::Literal(_) | TypeKey::Error | TypeKey::ThisType => {
                false
            }
            TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id) => {
                let shape = self.db.object_shape(*shape_id);
                shape.properties.iter().any(|p| self.check(p.type_id))
                    || shape
                        .string_index
                        .as_ref()
                        .map(|i| self.check(i.value_type))
                        .unwrap_or(false)
                    || shape
                        .number_index
                        .as_ref()
                        .map(|i| self.check(i.value_type))
                        .unwrap_or(false)
            }
            TypeKey::Union(list_id) | TypeKey::Intersection(list_id) => {
                let members = self.db.type_list(*list_id);
                members.iter().any(|&m| self.check(m))
            }
            TypeKey::Array(elem) => self.check(*elem),
            TypeKey::Tuple(list_id) => {
                let elements = self.db.tuple_list(*list_id);
                elements.iter().any(|e| self.check(e.type_id))
            }
            TypeKey::Function(shape_id) => {
                let shape = self.db.function_shape(*shape_id);
                shape.params.iter().any(|p| self.check(p.type_id))
                    || self.check(shape.return_type)
                    || shape.this_type.map(|t| self.check(t)).unwrap_or(false)
            }
            TypeKey::Callable(shape_id) => {
                let shape = self.db.callable_shape(*shape_id);
                shape.call_signatures.iter().any(|s| {
                    s.params.iter().any(|p| self.check(p.type_id)) || self.check(s.return_type)
                }) || shape.construct_signatures.iter().any(|s| {
                    s.params.iter().any(|p| self.check(p.type_id)) || self.check(s.return_type)
                }) || shape.properties.iter().any(|p| self.check(p.type_id))
            }
            TypeKey::TypeParameter(info) | TypeKey::Infer(info) => {
                info.constraint.map(|c| self.check(c)).unwrap_or(false)
                    || info.default.map(|d| self.check(d)).unwrap_or(false)
            }
            TypeKey::Ref(_) | TypeKey::TypeQuery(_) | TypeKey::UniqueSymbol(_) => false,
            TypeKey::Application(app_id) => {
                let app = self.db.type_application(*app_id);
                self.check(app.base) || app.args.iter().any(|&a| self.check(a))
            }
            TypeKey::Conditional(cond_id) => {
                let cond = self.db.conditional_type(*cond_id);
                self.check(cond.check_type)
                    || self.check(cond.extends_type)
                    || self.check(cond.true_type)
                    || self.check(cond.false_type)
            }
            TypeKey::Mapped(mapped_id) => {
                let mapped = self.db.mapped_type(*mapped_id);
                self.check(mapped.constraint)
                    || self.check(mapped.template)
                    || mapped.name_type.map(|n| self.check(n)).unwrap_or(false)
            }
            TypeKey::IndexAccess(obj, idx) => self.check(*obj) || self.check(*idx),
            TypeKey::TemplateLiteral(list_id) => {
                let spans = self.db.template_list(*list_id);
                spans.iter().any(|span| {
                    if let crate::solver::types::TemplateSpan::Type(type_id) = span {
                        self.check(*type_id)
                    } else {
                        false
                    }
                })
            }
            TypeKey::KeyOf(inner) | TypeKey::ReadonlyType(inner) => self.check(*inner),
            TypeKey::StringIntrinsic { type_arg, .. } => self.check(*type_arg),
        }
    }
}

// =============================================================================
// Type Extraction Helpers
// =============================================================================
// These functions extract data from types, avoiding the need for checker code
// to match on TypeKey directly.

/// Get the members of a union type.
///
/// Returns None if the type is not a union.
pub fn get_union_members(db: &dyn TypeDatabase, type_id: TypeId) -> Option<Vec<TypeId>> {
    match db.lookup(type_id) {
        Some(TypeKey::Union(list_id)) => {
            let members = db.type_list(list_id);
            Some(members.to_vec())
        }
        _ => None,
    }
}

/// Get the members of an intersection type.
///
/// Returns None if the type is not an intersection.
pub fn get_intersection_members(db: &dyn TypeDatabase, type_id: TypeId) -> Option<Vec<TypeId>> {
    match db.lookup(type_id) {
        Some(TypeKey::Intersection(list_id)) => {
            let members = db.type_list(list_id);
            Some(members.to_vec())
        }
        _ => None,
    }
}

/// Get the element type of an array.
///
/// Returns None if the type is not an array.
pub fn get_array_element_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    match db.lookup(type_id) {
        Some(TypeKey::Array(element_type)) => Some(element_type),
        _ => None,
    }
}

/// Get the elements of a tuple type.
///
/// Returns None if the type is not a tuple.
/// Returns a vector of (TypeId, optional, rest, name) tuples.
pub fn get_tuple_elements(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<crate::solver::types::TupleElement>> {
    match db.lookup(type_id) {
        Some(TypeKey::Tuple(list_id)) => {
            let elements = db.tuple_list(list_id);
            Some(elements.to_vec())
        }
        _ => None,
    }
}

/// Get the object shape ID for an object type.
///
/// Returns None if the type is not an object type.
pub fn get_object_shape_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<crate::solver::types::ObjectShapeId> {
    match db.lookup(type_id) {
        Some(TypeKey::Object(shape_id)) | Some(TypeKey::ObjectWithIndex(shape_id)) => {
            Some(shape_id)
        }
        _ => None,
    }
}

/// Get the object shape for an object type.
///
/// Returns None if the type is not an object type.
pub fn get_object_shape(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<crate::solver::types::ObjectShape>> {
    match db.lookup(type_id) {
        Some(TypeKey::Object(shape_id)) | Some(TypeKey::ObjectWithIndex(shape_id)) => {
            Some(db.object_shape(shape_id))
        }
        _ => None,
    }
}

/// Unwrap readonly type wrappers.
///
/// Returns the inner type if this is a ReadonlyType, otherwise returns the original type.
/// Does not recurse - call repeatedly to fully unwrap.
pub fn unwrap_readonly(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    match db.lookup(type_id) {
        Some(TypeKey::ReadonlyType(inner)) => inner,
        _ => type_id,
    }
}

/// Unwrap all readonly type wrappers recursively.
///
/// Keeps unwrapping until the type is no longer a ReadonlyType.
pub fn unwrap_readonly_deep(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    let mut current = type_id;
    let mut depth = 0;
    const MAX_DEPTH: usize = 100;

    while let Some(TypeKey::ReadonlyType(inner)) = db.lookup(current) {
        depth += 1;
        if depth > MAX_DEPTH {
            break;
        }
        current = inner;
    }
    current
}

/// Check if a type is an object type (Object or ObjectWithIndex) and return true.
///
/// This is a convenience alias for is_object_type for symmetry with extraction functions.
pub fn is_object_type_with_shape(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        db.lookup(type_id),
        Some(TypeKey::Object(_) | TypeKey::ObjectWithIndex(_))
    )
}

/// Get the type parameter info if this is a type parameter.
///
/// Returns None if not a type parameter.
pub fn get_type_parameter_info(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<crate::solver::types::TypeParamInfo> {
    match db.lookup(type_id) {
        Some(TypeKey::TypeParameter(info)) | Some(TypeKey::Infer(info)) => Some(info.clone()),
        _ => None,
    }
}

/// Get the constraint of a type parameter.
///
/// Returns None if not a type parameter or has no constraint.
pub fn get_type_parameter_constraint(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    match db.lookup(type_id) {
        Some(TypeKey::TypeParameter(info)) | Some(TypeKey::Infer(info)) => info.constraint,
        _ => None,
    }
}

/// Get the callable shape ID for a callable type.
///
/// Returns None if the type is not a Callable.
pub fn get_callable_shape_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<crate::solver::types::CallableShapeId> {
    match db.lookup(type_id) {
        Some(TypeKey::Callable(shape_id)) => Some(shape_id),
        _ => None,
    }
}

/// Get the callable shape for a callable type.
///
/// Returns None if the type is not a Callable.
pub fn get_callable_shape(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<crate::solver::types::CallableShape>> {
    match db.lookup(type_id) {
        Some(TypeKey::Callable(shape_id)) => Some(db.callable_shape(shape_id)),
        _ => None,
    }
}

/// Get the function shape ID for a function type.
///
/// Returns None if the type is not a Function.
pub fn get_function_shape_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<crate::solver::types::FunctionShapeId> {
    match db.lookup(type_id) {
        Some(TypeKey::Function(shape_id)) => Some(shape_id),
        _ => None,
    }
}

/// Get the function shape for a function type.
///
/// Returns None if the type is not a Function.
pub fn get_function_shape(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<crate::solver::types::FunctionShape>> {
    match db.lookup(type_id) {
        Some(TypeKey::Function(shape_id)) => Some(db.function_shape(shape_id)),
        _ => None,
    }
}

/// Get the conditional type info for a conditional type.
///
/// Returns None if the type is not a Conditional.
pub fn get_conditional_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<crate::solver::types::ConditionalType>> {
    match db.lookup(type_id) {
        Some(TypeKey::Conditional(cond_id)) => Some(db.conditional_type(cond_id)),
        _ => None,
    }
}

/// Get the mapped type info for a mapped type.
///
/// Returns None if the type is not a Mapped type.
pub fn get_mapped_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<crate::solver::types::MappedType>> {
    match db.lookup(type_id) {
        Some(TypeKey::Mapped(mapped_id)) => Some(db.mapped_type(mapped_id)),
        _ => None,
    }
}

/// Get the type application info for a generic application type.
///
/// Returns None if the type is not an Application.
pub fn get_type_application(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<crate::solver::types::TypeApplication>> {
    match db.lookup(type_id) {
        Some(TypeKey::Application(app_id)) => Some(db.type_application(app_id)),
        _ => None,
    }
}

/// Get the index access components (object type and index type).
///
/// Returns None if the type is not an IndexAccess.
pub fn get_index_access_types(db: &dyn TypeDatabase, type_id: TypeId) -> Option<(TypeId, TypeId)> {
    match db.lookup(type_id) {
        Some(TypeKey::IndexAccess(obj, idx)) => Some((obj, idx)),
        _ => None,
    }
}

/// Get the keyof inner type.
///
/// Returns None if the type is not a KeyOf.
pub fn get_keyof_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    match db.lookup(type_id) {
        Some(TypeKey::KeyOf(inner)) => Some(inner),
        _ => None,
    }
}

/// Get the symbol reference from a Ref type.
///
/// Returns None if the type is not a Ref.
pub fn get_ref_symbol(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<crate::solver::types::SymbolRef> {
    match db.lookup(type_id) {
        Some(TypeKey::Ref(sym_ref)) => Some(sym_ref),
        _ => None,
    }
}
