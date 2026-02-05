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

// Re-export extended type queries so callers can use `type_queries::*`
pub use crate::solver::type_queries_extended::*;

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

/// Check if a type is invokable (has call signatures, not just construct signatures).
///
/// This is more specific than is_callable_type - it ensures the type can be called
/// as a function (not just constructed with `new`).
///
/// # Arguments
///
/// * `db` - The type database/interner
/// * `type_id` - The type to check
///
/// # Returns
///
/// * `true` - If the type has call signatures
/// * `false` - Otherwise
///
/// # Examples
///
/// ```ignore
/// // Functions are invokable
/// assert!(is_invokable_type(&db, function_type));
///
/// // Callables with call signatures are invokable
/// assert!(is_invokable_type(&db, callable_with_call_sigs));
///
/// // Callables with ONLY construct signatures are NOT invokable
/// assert!(!is_invokable_type(&db, class_constructor_only));
/// ```
pub fn is_invokable_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    match db.lookup(type_id) {
        Some(TypeKey::Function(_)) => true,
        Some(TypeKey::Callable(shape_id)) => {
            let shape = db.callable_shape(shape_id);
            // Must have at least one call signature (not just construct signatures)
            !shape.call_signatures.is_empty()
        }
        // Intersections might contain a callable
        Some(TypeKey::Intersection(list_id)) => {
            let members = db.type_list(list_id);
            members.iter().any(|&m| is_invokable_type(db, m))
        }
        _ => false,
    }
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
/// Returns true for TypeKey::Lazy(DefId) (interfaces, classes, type aliases).
pub fn is_type_reference(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(db.lookup(type_id), Some(TypeKey::Lazy(_)))
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
        Some(TypeKey::Enum(_, _)) => false,
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
        Some(TypeKey::Enum(_, _)) => false,
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
            TypeKey::Lazy(_)
            | TypeKey::TypeQuery(_)
            | TypeKey::UniqueSymbol(_)
            | TypeKey::ModuleNamespace(_) => false,
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
            TypeKey::Enum(_def_id, member_type) => self.check(*member_type),
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

/// Get the DefId from a Lazy type.
///
/// Returns None if the type is not a Lazy type.
pub fn get_lazy_def_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<crate::solver::def::DefId> {
    match db.lookup(type_id) {
        Some(TypeKey::Lazy(def_id)) => Some(def_id),
        _ => None,
    }
}

/// Get the DefId from an Enum type.
///
/// Returns None if the type is not an Enum type.
pub fn get_enum_def_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<crate::solver::def::DefId> {
    match db.lookup(type_id) {
        Some(TypeKey::Enum(def_id, _)) => Some(def_id),
        _ => None,
    }
}

// =============================================================================
// Constructor Type Collection Helpers
// =============================================================================

/// Result of classifying a type for constructor collection.
///
/// This enum tells the caller what kind of type this is and how to proceed
/// when collecting constructor types from a composite type structure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstructorTypeKind {
    /// This is a Callable type - always a constructor type
    Callable,
    /// This is a Function type - check is_constructor flag on the shape
    Function(crate::solver::types::FunctionShapeId),
    /// Recurse into these member types (Union, Intersection)
    Members(Vec<TypeId>),
    /// Recurse into the inner type (ReadonlyType)
    Inner(TypeId),
    /// Recurse into the constraint (TypeParameter, Infer)
    Constraint(Option<TypeId>),
    /// This type needs full type evaluation (Conditional, Mapped, IndexAccess, KeyOf)
    NeedsTypeEvaluation,
    /// This is a generic application that needs instantiation
    NeedsApplicationEvaluation,
    /// This is a TypeQuery - resolve the symbol reference to get its type
    TypeQuery(crate::solver::types::SymbolRef),
    /// This type cannot be a constructor (primitives, literals, etc.)
    NotConstructor,
}

/// Classify a type for constructor type collection.
///
/// This function examines a TypeKey and returns information about how to handle it
/// when collecting constructor types. The caller is responsible for:
/// - Checking the `is_constructor` flag for Function types
/// - Evaluating types when `NeedsTypeEvaluation` or `NeedsApplicationEvaluation` is returned
/// - Resolving symbol references for TypeQuery
/// - Recursing into members/inner types
///
/// # Example
///
/// ```rust
/// use crate::solver::type_queries::{classify_constructor_type, ConstructorTypeKind};
///
/// match classify_constructor_type(db, type_id) {
///     ConstructorTypeKind::Callable => {
///         // This is a constructor type
///         ctor_types.push(type_id);
///     }
///     ConstructorTypeKind::Function(shape_id) => {
///         let shape = db.function_shape(shape_id);
///         if shape.is_constructor {
///             ctor_types.push(type_id);
///         }
///     }
///     ConstructorTypeKind::Members(members) => {
///         for member in members {
///             // Recurse
///         }
///     }
///     ConstructorTypeKind::NeedsTypeEvaluation => {
///         // Use evaluate_type_with_env
///     }
///     ConstructorTypeKind::NeedsApplicationEvaluation => {
///         // Use evaluate_application_type
///     }
///     // ... handle other cases
/// }
/// ```
pub fn classify_constructor_type(db: &dyn TypeDatabase, type_id: TypeId) -> ConstructorTypeKind {
    let Some(key) = db.lookup(type_id) else {
        return ConstructorTypeKind::NotConstructor;
    };

    match key {
        TypeKey::Callable(_) => ConstructorTypeKind::Callable,
        TypeKey::Function(shape_id) => ConstructorTypeKind::Function(shape_id),
        TypeKey::Intersection(members_id) | TypeKey::Union(members_id) => {
            let members = db.type_list(members_id);
            ConstructorTypeKind::Members(members.to_vec())
        }
        TypeKey::ReadonlyType(inner) => ConstructorTypeKind::Inner(inner),
        TypeKey::TypeParameter(info) | TypeKey::Infer(info) => {
            ConstructorTypeKind::Constraint(info.constraint)
        }
        TypeKey::Conditional(_)
        | TypeKey::Mapped(_)
        | TypeKey::IndexAccess(_, _)
        | TypeKey::KeyOf(_) => ConstructorTypeKind::NeedsTypeEvaluation,
        TypeKey::Application(_) => ConstructorTypeKind::NeedsApplicationEvaluation,
        TypeKey::TypeQuery(sym_ref) => ConstructorTypeKind::TypeQuery(sym_ref),
        // All other types cannot be constructors
        TypeKey::Enum(_, _) => ConstructorTypeKind::NotConstructor,
        TypeKey::Intrinsic(_)
        | TypeKey::Literal(_)
        | TypeKey::Object(_)
        | TypeKey::ObjectWithIndex(_)
        | TypeKey::Array(_)
        | TypeKey::Tuple(_)
        | TypeKey::Lazy(_)
        | TypeKey::TemplateLiteral(_)
        | TypeKey::UniqueSymbol(_)
        | TypeKey::ThisType
        | TypeKey::StringIntrinsic { .. }
        | TypeKey::ModuleNamespace(_)
        | TypeKey::Error => ConstructorTypeKind::NotConstructor,
    }
}

// =============================================================================
// Static Property Collection Helpers
// =============================================================================

/// Result of extracting static properties from a type.
///
/// This enum allows the caller to handle recursion and type evaluation
/// while keeping the TypeKey matching logic in the solver layer.
#[derive(Debug, Clone)]
pub enum StaticPropertySource {
    /// Direct properties from Callable, Object, or ObjectWithIndex types.
    Properties(Vec<crate::solver::PropertyInfo>),
    /// Member types that should be recursively processed (Union/Intersection).
    RecurseMembers(Vec<TypeId>),
    /// Single type to recurse into (TypeParameter constraint, ReadonlyType inner).
    RecurseSingle(TypeId),
    /// Type that needs evaluation before property extraction (Conditional, Mapped, etc.).
    NeedsEvaluation,
    /// Type that needs application evaluation (Application type).
    NeedsApplicationEvaluation,
    /// No properties available (primitives, error types, etc.).
    None,
}

/// Extract static property information from a type.
///
/// This function handles the TypeKey matching for property collection,
/// returning a `StaticPropertySource` that tells the caller how to proceed.
/// The caller is responsible for:
/// - Handling recursion for `RecurseMembers` and `RecurseSingle` cases
/// - Evaluating types for `NeedsEvaluation` and `NeedsApplicationEvaluation` cases
/// - Tracking visited types to prevent infinite loops
///
/// # Example
///
/// ```ignore
/// match get_static_property_source(&db, type_id) {
///     StaticPropertySource::Properties(props) => {
///         for prop in props {
///             properties.entry(prop.name).or_insert(prop);
///         }
///     }
///     StaticPropertySource::RecurseMembers(members) => {
///         for member in members {
///             // Recursively collect from member
///         }
///     }
///     // ... handle other cases
/// }
/// ```
pub fn get_static_property_source(db: &dyn TypeDatabase, type_id: TypeId) -> StaticPropertySource {
    let Some(key) = db.lookup(type_id) else {
        return StaticPropertySource::None;
    };

    match key {
        TypeKey::Callable(shape_id) => {
            let shape = db.callable_shape(shape_id);
            StaticPropertySource::Properties(shape.properties.to_vec())
        }
        TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id) => {
            let shape = db.object_shape(shape_id);
            StaticPropertySource::Properties(shape.properties.to_vec())
        }
        TypeKey::Intersection(members_id) | TypeKey::Union(members_id) => {
            let members = db.type_list(members_id);
            StaticPropertySource::RecurseMembers(members.to_vec())
        }
        TypeKey::TypeParameter(info) | TypeKey::Infer(info) => {
            if let Some(constraint) = info.constraint {
                StaticPropertySource::RecurseSingle(constraint)
            } else {
                StaticPropertySource::None
            }
        }
        TypeKey::ReadonlyType(inner) => StaticPropertySource::RecurseSingle(inner),
        TypeKey::Conditional(_)
        | TypeKey::Mapped(_)
        | TypeKey::IndexAccess(_, _)
        | TypeKey::KeyOf(_) => StaticPropertySource::NeedsEvaluation,
        TypeKey::Application(_) => StaticPropertySource::NeedsApplicationEvaluation,
        _ => StaticPropertySource::None,
    }
}

// =============================================================================
// Construct Signature Queries
// =============================================================================

/// Check if a Callable type has construct signatures.
///
/// Returns true only for Callable types that have non-empty construct_signatures.
/// This is a direct check and does not resolve through Ref or TypeQuery types.
pub fn has_construct_signatures(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    match db.lookup(type_id) {
        Some(TypeKey::Callable(shape_id)) => {
            let shape = db.callable_shape(shape_id);
            !shape.construct_signatures.is_empty()
        }
        _ => false,
    }
}

/// Get the symbol reference from a TypeQuery type.
///
/// Returns None if the type is not a TypeQuery.
pub fn get_symbol_ref_from_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<crate::solver::types::SymbolRef> {
    match db.lookup(type_id) {
        Some(TypeKey::TypeQuery(sym_ref)) => Some(sym_ref),
        _ => None,
    }
}

/// Kind of constructable type for `get_construct_type_from_type`.
///
/// This enum represents the different ways a type can be constructable,
/// allowing the caller to handle each case appropriately without matching
/// directly on TypeKey.
#[derive(Debug, Clone)]
pub enum ConstructableTypeKind {
    /// Callable type with construct signatures - return transformed callable
    CallableWithConstruct,
    /// Callable type without construct signatures - check for prototype property
    CallableMaybePrototype,
    /// Function type - always constructable
    Function,
    /// Reference to a symbol - need to check symbol flags
    SymbolRef(crate::solver::types::SymbolRef),
    /// TypeQuery (typeof expr) - need to check symbol flags
    TypeQueryRef(crate::solver::types::SymbolRef),
    /// Type parameter with a constraint to check recursively
    TypeParameterWithConstraint(TypeId),
    /// Type parameter without constraint - not constructable
    TypeParameterNoConstraint,
    /// Intersection type - all members must be constructable
    Intersection(Vec<TypeId>),
    /// Application (generic instantiation) - return as-is
    Application,
    /// Object type - return as-is (may have construct signatures)
    Object,
    /// Not constructable
    NotConstructable,
}

/// Classify a type for constructability checking.
///
/// This function examines a type and returns information about how to handle it
/// when determining if it can be used with `new`. This is specifically for
/// the `get_construct_type_from_type` use case.
///
/// The caller is responsible for:
/// - Checking symbol flags for SymbolRef/TypeQueryRef cases
/// - Checking prototype property for CallableMaybePrototype
/// - Recursing into constraint for TypeParameterWithConstraint
/// - Checking all members for Intersection
pub fn classify_for_constructability(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> ConstructableTypeKind {
    let Some(key) = db.lookup(type_id) else {
        return ConstructableTypeKind::NotConstructable;
    };

    match key {
        TypeKey::Callable(shape_id) => {
            let shape = db.callable_shape(shape_id);
            if shape.construct_signatures.is_empty() {
                ConstructableTypeKind::CallableMaybePrototype
            } else {
                ConstructableTypeKind::CallableWithConstruct
            }
        }
        TypeKey::Function(_) => ConstructableTypeKind::Function,
        TypeKey::TypeQuery(sym_ref) => ConstructableTypeKind::TypeQueryRef(sym_ref),
        TypeKey::TypeParameter(info) | TypeKey::Infer(info) => {
            if let Some(constraint) = info.constraint {
                ConstructableTypeKind::TypeParameterWithConstraint(constraint)
            } else {
                ConstructableTypeKind::TypeParameterNoConstraint
            }
        }
        TypeKey::Intersection(members_id) => {
            let members = db.type_list(members_id);
            ConstructableTypeKind::Intersection(members.to_vec())
        }
        TypeKey::Application(_) => ConstructableTypeKind::Application,
        TypeKey::Object(_) | TypeKey::ObjectWithIndex(_) => ConstructableTypeKind::Object,
        _ => ConstructableTypeKind::NotConstructable,
    }
}

/// Create a callable type with construct signatures converted to call signatures.
///
/// This is used when resolving `new` expressions where we need to treat
/// construct signatures as call signatures for type checking purposes.
/// Returns None if the type doesn't have construct signatures.
pub fn construct_to_call_callable(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    match db.lookup(type_id) {
        Some(TypeKey::Callable(shape_id)) => {
            let shape = db.callable_shape(shape_id);
            if shape.construct_signatures.is_empty() {
                None
            } else {
                Some(db.callable(crate::solver::types::CallableShape {
                    call_signatures: shape.construct_signatures.clone(),
                    construct_signatures: Vec::new(),
                    properties: Vec::new(),
                    string_index: None,
                    number_index: None,
                    symbol: None,
                }))
            }
        }
        _ => None,
    }
}

// =============================================================================
// Constraint Type Classification Helpers
// =============================================================================

/// Classification for constraint types.
#[derive(Debug, Clone)]
pub enum ConstraintTypeKind {
    /// Type parameter or infer with constraint
    TypeParameter {
        constraint: Option<TypeId>,
        default: Option<TypeId>,
    },
    /// Union - get constraint from each member
    Union(Vec<TypeId>),
    /// Intersection - get constraint from each member
    Intersection(Vec<TypeId>),
    /// Symbol reference - resolve first
    SymbolRef(crate::solver::types::SymbolRef),
    /// Application - evaluate first
    Application { app_id: u32 },
    /// Mapped type - evaluate constraint
    Mapped { mapped_id: u32 },
    /// KeyOf - special handling
    KeyOf(TypeId),
    /// Literal or resolved constraint
    Resolved(TypeId),
    /// No constraint
    NoConstraint,
}

/// Classify a type for constraint extraction.
pub fn classify_for_constraint(db: &dyn TypeDatabase, type_id: TypeId) -> ConstraintTypeKind {
    let Some(key) = db.lookup(type_id) else {
        return ConstraintTypeKind::NoConstraint;
    };
    match key {
        TypeKey::TypeParameter(info) | TypeKey::Infer(info) => ConstraintTypeKind::TypeParameter {
            constraint: info.constraint,
            default: info.default,
        },
        TypeKey::Union(list_id) => {
            let members = db.type_list(list_id);
            ConstraintTypeKind::Union(members.to_vec())
        }
        TypeKey::Intersection(list_id) => {
            let members = db.type_list(list_id);
            ConstraintTypeKind::Intersection(members.to_vec())
        }
        TypeKey::Application(app_id) => ConstraintTypeKind::Application { app_id: app_id.0 },
        TypeKey::Mapped(mapped_id) => ConstraintTypeKind::Mapped {
            mapped_id: mapped_id.0,
        },
        TypeKey::KeyOf(operand) => ConstraintTypeKind::KeyOf(operand),
        TypeKey::Literal(_) => ConstraintTypeKind::Resolved(type_id),
        TypeKey::Intrinsic(_)
        | TypeKey::Object(_)
        | TypeKey::ObjectWithIndex(_)
        | TypeKey::Array(_)
        | TypeKey::Tuple(_)
        | TypeKey::Function(_)
        | TypeKey::Callable(_)
        | TypeKey::Conditional(_)
        | TypeKey::IndexAccess(_, _)
        | TypeKey::TemplateLiteral(_)
        | TypeKey::UniqueSymbol(_)
        | TypeKey::ThisType
        | TypeKey::ReadonlyType(_)
        | TypeKey::TypeQuery(_)
        | TypeKey::StringIntrinsic { .. }
        | TypeKey::ModuleNamespace(_)
        | TypeKey::Enum(_, _)
        | TypeKey::Lazy(_)
        | TypeKey::Error => ConstraintTypeKind::NoConstraint,
    }
}

// =============================================================================
// Signature Classification
// =============================================================================

/// Classification for types when extracting call/construct signatures.
#[derive(Debug, Clone)]
pub enum SignatureTypeKind {
    /// Callable type with shape_id - has call_signatures and construct_signatures
    Callable(crate::solver::types::CallableShapeId),
    /// Function type with shape_id - has single signature
    Function(crate::solver::types::FunctionShapeId),
    /// Union type - get signatures from each member
    Union(Vec<TypeId>),
    /// Intersection type - get signatures from each member
    Intersection(Vec<TypeId>),
    /// Readonly wrapper - unwrap and get signatures from inner type
    ReadonlyType(TypeId),
    /// Type parameter with optional constraint - may need to check constraint
    TypeParameter { constraint: Option<TypeId> },
    /// Types that need evaluation before signature extraction (Conditional, Mapped, IndexAccess, KeyOf)
    NeedsEvaluation(TypeId),
    /// Types without signatures (Intrinsic, Literal, Object without callable, etc.)
    NoSignatures,
}

/// Classify a type for signature extraction.
pub fn classify_for_signatures(db: &dyn TypeDatabase, type_id: TypeId) -> SignatureTypeKind {
    // Handle special TypeIds first
    if type_id == TypeId::ERROR || type_id == TypeId::NEVER {
        return SignatureTypeKind::NoSignatures;
    }
    if type_id == TypeId::ANY {
        // any is callable but has no concrete signatures
        return SignatureTypeKind::NoSignatures;
    }

    let Some(key) = db.lookup(type_id) else {
        return SignatureTypeKind::NoSignatures;
    };

    match key {
        // Callable types - have call_signatures and construct_signatures
        TypeKey::Callable(shape_id) => SignatureTypeKind::Callable(shape_id),

        // Function types - have a single signature
        TypeKey::Function(shape_id) => SignatureTypeKind::Function(shape_id),

        // Union type - get signatures from each member
        TypeKey::Union(members_id) => {
            let members = db.type_list(members_id);
            SignatureTypeKind::Union(members.to_vec())
        }

        // Intersection type - get signatures from each member
        TypeKey::Intersection(members_id) => {
            let members = db.type_list(members_id);
            SignatureTypeKind::Intersection(members.to_vec())
        }

        // Readonly wrapper - unwrap and recurse
        TypeKey::ReadonlyType(inner) => SignatureTypeKind::ReadonlyType(inner),

        // Type parameter - may have constraint with signatures
        TypeKey::TypeParameter(info) | TypeKey::Infer(info) => SignatureTypeKind::TypeParameter {
            constraint: info.constraint,
        },

        // Complex types that need evaluation before signature extraction
        TypeKey::Conditional(_)
        | TypeKey::Mapped(_)
        | TypeKey::IndexAccess(_, _)
        | TypeKey::KeyOf(_) => SignatureTypeKind::NeedsEvaluation(type_id),

        // All other types don't have callable signatures
        TypeKey::Intrinsic(_)
        | TypeKey::Literal(_)
        | TypeKey::Object(_)
        | TypeKey::ObjectWithIndex(_)
        | TypeKey::Array(_)
        | TypeKey::Tuple(_)
        | TypeKey::Lazy(_)
        | TypeKey::Application(_)
        | TypeKey::TemplateLiteral(_)
        | TypeKey::UniqueSymbol(_)
        | TypeKey::ThisType
        | TypeKey::TypeQuery(_)
        | TypeKey::StringIntrinsic { .. }
        | TypeKey::ModuleNamespace(_)
        | TypeKey::Enum(_, _)
        | TypeKey::Error => SignatureTypeKind::NoSignatures,
    }
}

// =============================================================================
// Iterable Type Classification (Spread Handling)
// =============================================================================

/// Classification for iterable types (used for spread element handling).
#[derive(Debug, Clone)]
pub enum IterableTypeKind {
    /// Tuple type - elements can be expanded
    Tuple(Vec<crate::solver::types::TupleElement>),
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
        TypeKey::Tuple(tuple_id) => {
            let elements = db.tuple_list(tuple_id);
            IterableTypeKind::Tuple(elements.to_vec())
        }
        TypeKey::Array(elem_type) => IterableTypeKind::Array(elem_type),
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
/// matching on TypeKey in the checker layer.
#[derive(Debug, Clone)]
pub enum FullIterableTypeKind {
    /// Array type - always iterable
    Array(TypeId),
    /// Tuple type - always iterable
    Tuple(Vec<crate::solver::types::TupleElement>),
    /// String literal - always iterable
    StringLiteral(crate::interner::Atom),
    /// Union type - all members must be iterable
    Union(Vec<TypeId>),
    /// Intersection type - at least one member must be iterable
    Intersection(Vec<TypeId>),
    /// Object type - check for [Symbol.iterator] method
    Object(crate::solver::types::ObjectShapeId),
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
        TypeKey::Array(elem) => FullIterableTypeKind::Array(elem),
        TypeKey::Tuple(tuple_id) => {
            let elements = db.tuple_list(tuple_id);
            FullIterableTypeKind::Tuple(elements.to_vec())
        }
        TypeKey::Literal(crate::solver::LiteralValue::String(s)) => {
            FullIterableTypeKind::StringLiteral(s)
        }
        TypeKey::Union(members_id) => {
            let members = db.type_list(members_id);
            FullIterableTypeKind::Union(members.to_vec())
        }
        TypeKey::Intersection(members_id) => {
            let members = db.type_list(members_id);
            FullIterableTypeKind::Intersection(members.to_vec())
        }
        TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id) => {
            FullIterableTypeKind::Object(shape_id)
        }
        TypeKey::Application(app_id) => {
            let app = db.type_application(app_id);
            FullIterableTypeKind::Application { base: app.base }
        }
        TypeKey::TypeParameter(info) | TypeKey::Infer(info) => {
            FullIterableTypeKind::TypeParameter {
                constraint: info.constraint,
            }
        }
        TypeKey::ReadonlyType(inner) => FullIterableTypeKind::Readonly(inner),
        TypeKey::Function(_) | TypeKey::Callable(_) => FullIterableTypeKind::FunctionOrCallable,
        TypeKey::IndexAccess(_, _) | TypeKey::Conditional(_) | TypeKey::Mapped(_) => {
            FullIterableTypeKind::ComplexType
        }
        // All other types are not directly iterable
        TypeKey::Intrinsic(_)
        | TypeKey::Literal(_)
        | TypeKey::Lazy(_)
        | TypeKey::TemplateLiteral(_)
        | TypeKey::UniqueSymbol(_)
        | TypeKey::ThisType
        | TypeKey::TypeQuery(_)
        | TypeKey::KeyOf(_)
        | TypeKey::StringIntrinsic { .. }
        | TypeKey::ModuleNamespace(_)
        | TypeKey::Enum(_, _)
        | TypeKey::Error => FullIterableTypeKind::NotIterable,
    }
}

/// Classification for async iterable type checking.
#[derive(Debug, Clone)]
pub enum AsyncIterableTypeKind {
    /// Union type - all members must be async iterable
    Union(Vec<TypeId>),
    /// Object type - check for [Symbol.asyncIterator] method
    Object(crate::solver::types::ObjectShapeId),
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
        TypeKey::Union(members_id) => {
            let members = db.type_list(members_id);
            AsyncIterableTypeKind::Union(members.to_vec())
        }
        TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id) => {
            AsyncIterableTypeKind::Object(shape_id)
        }
        TypeKey::ReadonlyType(inner) => AsyncIterableTypeKind::Readonly(inner),
        _ => AsyncIterableTypeKind::NotAsyncIterable,
    }
}

/// Classification for for-of element type computation.
#[derive(Debug, Clone)]
pub enum ForOfElementKind {
    /// Array type - element is the array element type
    Array(TypeId),
    /// Tuple type - element is union of tuple element types
    Tuple(Vec<crate::solver::types::TupleElement>),
    /// Union type - compute element type for each member
    Union(Vec<TypeId>),
    /// Readonly wrapper - unwrap and compute
    Readonly(TypeId),
    /// Other types - return ANY as fallback
    Other,
}

/// Classify a type for for-of element type computation.
pub fn classify_for_of_element_type(db: &dyn TypeDatabase, type_id: TypeId) -> ForOfElementKind {
    let Some(key) = db.lookup(type_id) else {
        return ForOfElementKind::Other;
    };

    match key {
        TypeKey::Array(elem) => ForOfElementKind::Array(elem),
        TypeKey::Tuple(tuple_id) => {
            let elements = db.tuple_list(tuple_id);
            ForOfElementKind::Tuple(elements.to_vec())
        }
        TypeKey::Union(members_id) => {
            let members = db.type_list(members_id);
            ForOfElementKind::Union(members.to_vec())
        }
        TypeKey::ReadonlyType(inner) => ForOfElementKind::Readonly(inner),
        _ => ForOfElementKind::Other,
    }
}

// =============================================================================
// Property Lookup Type Classification
// =============================================================================

/// Classification for types when looking up properties.
///
/// This enum provides a structured way to handle property lookups on different
/// type kinds, abstracting away the internal TypeKey representation.
///
/// # Design Principles
///
/// - **No Symbol Resolution**: Keeps solver layer pure
/// - **No Type Evaluation**: Returns classification for caller to handle
/// - **Complete Coverage**: Handles all common property access patterns
#[derive(Debug, Clone)]
pub enum PropertyLookupKind {
    /// Object type with shape_id - has properties
    Object(crate::solver::types::ObjectShapeId),
    /// Object with index signature - has properties and index signatures
    ObjectWithIndex(crate::solver::types::ObjectShapeId),
    /// Union type - lookup on each member
    Union(Vec<TypeId>),
    /// Intersection type - lookup on each member
    Intersection(Vec<TypeId>),
    /// Array type - element type for numeric access
    Array(TypeId),
    /// Tuple type - element types
    Tuple(Vec<crate::solver::types::TupleElement>),
    /// Type that doesn't have direct properties (Intrinsic, Literal, etc.)
    NoProperties,
}

/// Classify a type for property lookup operations.
///
/// This function examines a type and returns information about how to handle it
/// when looking up properties. This is used for:
/// - Merging base type properties
/// - Checking excess properties in object literals
/// - Getting binding element types from destructuring patterns
///
/// The caller is responsible for:
/// - Recursing into Union/Intersection members
/// - Handling Array/Tuple element access appropriately
/// - Accessing the object shape using the returned shape_id
///
/// # Example
///
/// ```ignore
/// use crate::solver::type_queries::{classify_for_property_lookup, PropertyLookupKind};
///
/// match classify_for_property_lookup(&db, type_id) {
///     PropertyLookupKind::Object(shape_id) | PropertyLookupKind::ObjectWithIndex(shape_id) => {
///         let shape = db.object_shape(shape_id);
///         for prop in shape.properties.iter() {
///             // Process property
///         }
///     }
///     PropertyLookupKind::Union(members) | PropertyLookupKind::Intersection(members) => {
///         for member in members {
///             // Recurse
///         }
///     }
///     PropertyLookupKind::Array(elem_type) => {
///         // Use element type for numeric index access
///     }
///     PropertyLookupKind::Tuple(elements) => {
///         // Use specific element type by index
///     }
///     PropertyLookupKind::NoProperties => {
///         // Handle types without properties
///     }
/// }
/// ```
pub fn classify_for_property_lookup(db: &dyn TypeDatabase, type_id: TypeId) -> PropertyLookupKind {
    let Some(key) = db.lookup(type_id) else {
        return PropertyLookupKind::NoProperties;
    };

    match key {
        TypeKey::Object(shape_id) => PropertyLookupKind::Object(shape_id),
        TypeKey::ObjectWithIndex(shape_id) => PropertyLookupKind::ObjectWithIndex(shape_id),
        TypeKey::Union(list_id) => {
            let members = db.type_list(list_id);
            PropertyLookupKind::Union(members.to_vec())
        }
        TypeKey::Intersection(list_id) => {
            let members = db.type_list(list_id);
            PropertyLookupKind::Intersection(members.to_vec())
        }
        TypeKey::Array(elem_type) => PropertyLookupKind::Array(elem_type),
        TypeKey::Tuple(tuple_id) => {
            let elements = db.tuple_list(tuple_id);
            PropertyLookupKind::Tuple(elements.to_vec())
        }
        // All other types don't have direct properties for this use case
        TypeKey::Intrinsic(_)
        | TypeKey::Literal(_)
        | TypeKey::Function(_)
        | TypeKey::Callable(_)
        | TypeKey::TypeParameter(_)
        | TypeKey::Infer(_)
        | TypeKey::Lazy(_)
        | TypeKey::Application(_)
        | TypeKey::Conditional(_)
        | TypeKey::Mapped(_)
        | TypeKey::IndexAccess(_, _)
        | TypeKey::KeyOf(_)
        | TypeKey::TemplateLiteral(_)
        | TypeKey::UniqueSymbol(_)
        | TypeKey::ThisType
        | TypeKey::TypeQuery(_)
        | TypeKey::ReadonlyType(_)
        | TypeKey::StringIntrinsic { .. }
        | TypeKey::ModuleNamespace(_)
        | TypeKey::Enum(_, _)
        | TypeKey::Error => PropertyLookupKind::NoProperties,
    }
}

// =============================================================================
// EvaluationNeeded - Classification for types that need evaluation
// =============================================================================

/// Classification for types that need evaluation before use.
#[derive(Debug, Clone)]
pub enum EvaluationNeeded {
    /// Already resolved, no evaluation needed
    Resolved(TypeId),
    /// Symbol reference - resolve symbol first
    SymbolRef(crate::solver::types::SymbolRef),
    /// Type query (typeof) - evaluate first
    TypeQuery(crate::solver::types::SymbolRef),
    /// Generic application - instantiate first
    Application {
        app_id: crate::solver::types::TypeApplicationId,
    },
    /// Index access T[K] - evaluate with environment
    IndexAccess { object: TypeId, index: TypeId },
    /// KeyOf type - evaluate
    KeyOf(TypeId),
    /// Mapped type - evaluate
    Mapped {
        mapped_id: crate::solver::types::MappedTypeId,
    },
    /// Conditional type - evaluate
    Conditional {
        cond_id: crate::solver::types::ConditionalTypeId,
    },
    /// Callable type (for contextual typing checks)
    Callable(crate::solver::types::CallableShapeId),
    /// Function type
    Function(crate::solver::types::FunctionShapeId),
    /// Union - may need per-member evaluation
    Union(Vec<TypeId>),
    /// Intersection - may need per-member evaluation
    Intersection(Vec<TypeId>),
    /// Type parameter with constraint
    TypeParameter { constraint: Option<TypeId> },
    /// Readonly wrapper - unwrap
    Readonly(TypeId),
}

/// Classify a type for what kind of evaluation it needs.
pub fn classify_for_evaluation(db: &dyn TypeDatabase, type_id: TypeId) -> EvaluationNeeded {
    let Some(key) = db.lookup(type_id) else {
        return EvaluationNeeded::Resolved(type_id);
    };

    match key {
        TypeKey::TypeQuery(sym_ref) => EvaluationNeeded::TypeQuery(sym_ref),
        TypeKey::Application(app_id) => EvaluationNeeded::Application { app_id },
        TypeKey::IndexAccess(object, index) => EvaluationNeeded::IndexAccess { object, index },
        TypeKey::KeyOf(inner) => EvaluationNeeded::KeyOf(inner),
        TypeKey::Mapped(mapped_id) => EvaluationNeeded::Mapped { mapped_id },
        TypeKey::Conditional(cond_id) => EvaluationNeeded::Conditional { cond_id },
        TypeKey::Callable(shape_id) => EvaluationNeeded::Callable(shape_id),
        TypeKey::Function(shape_id) => EvaluationNeeded::Function(shape_id),
        TypeKey::Union(list_id) => {
            let members = db.type_list(list_id);
            EvaluationNeeded::Union(members.to_vec())
        }
        TypeKey::Intersection(list_id) => {
            let members = db.type_list(list_id);
            EvaluationNeeded::Intersection(members.to_vec())
        }
        TypeKey::TypeParameter(info) => EvaluationNeeded::TypeParameter {
            constraint: info.constraint,
        },
        TypeKey::Infer(info) => EvaluationNeeded::TypeParameter {
            constraint: info.constraint,
        },
        TypeKey::ReadonlyType(inner) => EvaluationNeeded::Readonly(inner),
        // Already resolved types (Lazy needs special handling when DefId lookup is implemented)
        TypeKey::Intrinsic(_)
        | TypeKey::Literal(_)
        | TypeKey::Object(_)
        | TypeKey::ObjectWithIndex(_)
        | TypeKey::Array(_)
        | TypeKey::Tuple(_)
        | TypeKey::Lazy(_)
        | TypeKey::TemplateLiteral(_)
        | TypeKey::UniqueSymbol(_)
        | TypeKey::ThisType
        | TypeKey::StringIntrinsic { .. }
        | TypeKey::ModuleNamespace(_)
        | TypeKey::Enum(_, _)
        | TypeKey::Error => EvaluationNeeded::Resolved(type_id),
    }
}

// =============================================================================
// PropertyAccessClassification - Classification for property access resolution
// =============================================================================

/// Classification for property access resolution.
#[derive(Debug, Clone)]
pub enum PropertyAccessClassification {
    /// Direct object type that can have properties accessed
    Direct(TypeId),
    /// Symbol reference - needs resolution first
    SymbolRef(crate::solver::types::SymbolRef),
    /// Type query (typeof) - needs symbol resolution
    TypeQuery(crate::solver::types::SymbolRef),
    /// Generic application - needs instantiation
    Application {
        app_id: crate::solver::types::TypeApplicationId,
    },
    /// Union - access on each member
    Union(Vec<TypeId>),
    /// Intersection - access on each member
    Intersection(Vec<TypeId>),
    /// Index access - needs evaluation
    IndexAccess { object: TypeId, index: TypeId },
    /// Readonly wrapper - unwrap and continue
    Readonly(TypeId),
    /// Callable type - may need Function interface expansion
    Callable(TypeId),
    /// Type parameter with constraint
    TypeParameter { constraint: Option<TypeId> },
    /// Needs evaluation (Conditional, Mapped, KeyOf)
    NeedsEvaluation(TypeId),
    /// Primitive or resolved type
    Resolved(TypeId),
}

/// Classify a type for property access resolution.
pub fn classify_for_property_access(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> PropertyAccessClassification {
    let Some(key) = db.lookup(type_id) else {
        return PropertyAccessClassification::Resolved(type_id);
    };

    match key {
        TypeKey::Object(_) | TypeKey::ObjectWithIndex(_) => {
            PropertyAccessClassification::Direct(type_id)
        }
        TypeKey::TypeQuery(sym_ref) => PropertyAccessClassification::TypeQuery(sym_ref),
        TypeKey::Application(app_id) => PropertyAccessClassification::Application { app_id },
        TypeKey::Union(list_id) => {
            let members = db.type_list(list_id);
            PropertyAccessClassification::Union(members.to_vec())
        }
        TypeKey::Intersection(list_id) => {
            let members = db.type_list(list_id);
            PropertyAccessClassification::Intersection(members.to_vec())
        }
        TypeKey::IndexAccess(object, index) => {
            PropertyAccessClassification::IndexAccess { object, index }
        }
        TypeKey::ReadonlyType(inner) => PropertyAccessClassification::Readonly(inner),
        TypeKey::Function(_) | TypeKey::Callable(_) => {
            PropertyAccessClassification::Callable(type_id)
        }
        TypeKey::TypeParameter(info) => PropertyAccessClassification::TypeParameter {
            constraint: info.constraint,
        },
        TypeKey::Infer(info) => PropertyAccessClassification::TypeParameter {
            constraint: info.constraint,
        },
        TypeKey::Conditional(_) | TypeKey::Mapped(_) | TypeKey::KeyOf(_) => {
            PropertyAccessClassification::NeedsEvaluation(type_id)
        }
        // Primitives and resolved types (Lazy needs special handling when DefId lookup is implemented)
        TypeKey::Intrinsic(_)
        | TypeKey::Literal(_)
        | TypeKey::Array(_)
        | TypeKey::Tuple(_)
        | TypeKey::Lazy(_)
        | TypeKey::TemplateLiteral(_)
        | TypeKey::UniqueSymbol(_)
        | TypeKey::ThisType
        | TypeKey::StringIntrinsic { .. }
        | TypeKey::ModuleNamespace(_)
        | TypeKey::Error
        | TypeKey::Enum(_, _) => PropertyAccessClassification::Resolved(type_id),
    }
}

// =============================================================================
// TypeTraversalKind - Classification for type structure traversal
// =============================================================================

/// Classification for traversing type structure to resolve symbols.
///
/// This enum is used by `ensure_application_symbols_resolved_inner` to
/// determine how to traverse into nested types without directly matching
/// on TypeKey in the checker layer.
#[derive(Debug, Clone)]
pub enum TypeTraversalKind {
    /// Application type - resolve base symbol and recurse into base and args
    Application {
        app_id: crate::solver::types::TypeApplicationId,
        base: TypeId,
        args: Vec<TypeId>,
    },
    /// Symbol reference - resolve the symbol
    SymbolRef(crate::solver::types::SymbolRef),
    /// Type parameter - recurse into constraint and default if present
    TypeParameter {
        constraint: Option<TypeId>,
        default: Option<TypeId>,
    },
    /// Union or intersection - recurse into members
    Members(Vec<TypeId>),
    /// Function type - recurse into type params, params, return type, etc.
    Function(crate::solver::types::FunctionShapeId),
    /// Callable type - recurse into signatures and properties
    Callable(crate::solver::types::CallableShapeId),
    /// Object type - recurse into properties and index signatures
    Object(crate::solver::types::ObjectShapeId),
    /// Array type - recurse into element type
    Array(TypeId),
    /// Tuple type - recurse into element types
    Tuple(crate::solver::types::TupleListId),
    /// Conditional type - recurse into check, extends, true, and false types
    Conditional(crate::solver::types::ConditionalTypeId),
    /// Mapped type - recurse into constraint, template, and name type
    Mapped(crate::solver::types::MappedTypeId),
    /// Readonly wrapper - recurse into inner type
    Readonly(TypeId),
    /// Index access - recurse into object and index types
    IndexAccess { object: TypeId, index: TypeId },
    /// KeyOf - recurse into inner type
    KeyOf(TypeId),
    /// Terminal type - no further traversal needed
    Terminal,
}

/// Classify a type for structure traversal (symbol resolution).
///
/// This function examines a type and returns information about how to
/// traverse into its nested types. Used by `ensure_application_symbols_resolved_inner`.
pub fn classify_for_traversal(db: &dyn TypeDatabase, type_id: TypeId) -> TypeTraversalKind {
    let Some(key) = db.lookup(type_id) else {
        return TypeTraversalKind::Terminal;
    };

    match key {
        TypeKey::Application(app_id) => {
            let app = db.type_application(app_id);
            TypeTraversalKind::Application {
                app_id,
                base: app.base,
                args: app.args.clone(),
            }
        }
        TypeKey::TypeParameter(info) => TypeTraversalKind::TypeParameter {
            constraint: info.constraint,
            default: info.default,
        },
        TypeKey::Infer(info) => TypeTraversalKind::TypeParameter {
            constraint: info.constraint,
            default: info.default,
        },
        TypeKey::Union(list_id) | TypeKey::Intersection(list_id) => {
            let members = db.type_list(list_id);
            TypeTraversalKind::Members(members.to_vec())
        }
        TypeKey::Function(shape_id) => TypeTraversalKind::Function(shape_id),
        TypeKey::Callable(shape_id) => TypeTraversalKind::Callable(shape_id),
        TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id) => {
            TypeTraversalKind::Object(shape_id)
        }
        TypeKey::Array(elem) => TypeTraversalKind::Array(elem),
        TypeKey::Tuple(list_id) => TypeTraversalKind::Tuple(list_id),
        TypeKey::Conditional(cond_id) => TypeTraversalKind::Conditional(cond_id),
        TypeKey::Mapped(mapped_id) => TypeTraversalKind::Mapped(mapped_id),
        TypeKey::ReadonlyType(inner) => TypeTraversalKind::Readonly(inner),
        TypeKey::IndexAccess(object, index) => TypeTraversalKind::IndexAccess { object, index },
        TypeKey::KeyOf(inner) => TypeTraversalKind::KeyOf(inner),
        // Terminal types - no nested types to traverse (Lazy needs resolution for traversal)
        TypeKey::Intrinsic(_)
        | TypeKey::Literal(_)
        | TypeKey::TemplateLiteral(_)
        | TypeKey::Lazy(_)
        | TypeKey::UniqueSymbol(_)
        | TypeKey::ThisType
        | TypeKey::TypeQuery(_)
        | TypeKey::StringIntrinsic { .. }
        | TypeKey::ModuleNamespace(_)
        | TypeKey::Error
        | TypeKey::Enum(_, _) => TypeTraversalKind::Terminal,
    }
}

/// Check if a type is a lazy type and return the DefId.
///
/// This is a helper for checking if the base of an Application is a Lazy type.
pub fn get_lazy_if_def(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<crate::solver::def::DefId> {
    match db.lookup(type_id) {
        Some(TypeKey::Lazy(def_id)) => Some(def_id),
        _ => None,
    }
}

// =============================================================================
// Interface Merge Type Classification
// =============================================================================

/// Classification for types when merging interfaces.
///
/// This enum provides a structured way to handle interface type merging,
/// abstracting away the internal TypeKey representation. Used for merging
/// derived and base interface types.
#[derive(Debug, Clone)]
pub enum InterfaceMergeKind {
    /// Callable type with call/construct signatures and properties
    Callable(crate::solver::types::CallableShapeId),
    /// Object type with properties only
    Object(crate::solver::types::ObjectShapeId),
    /// Object type with properties and index signatures
    ObjectWithIndex(crate::solver::types::ObjectShapeId),
    /// Intersection type - create intersection with base
    Intersection,
    /// Other type kinds - return derived unchanged
    Other,
}

/// Classify a type for interface merging operations.
///
/// This function examines a type and returns information about how to handle it
/// when merging interface types. Used by `merge_interface_types`.
///
/// # Example
///
/// ```ignore
/// use crate::solver::type_queries::{classify_for_interface_merge, InterfaceMergeKind};
///
/// match classify_for_interface_merge(&db, type_id) {
///     InterfaceMergeKind::Callable(shape_id) => {
///         let shape = db.callable_shape(shape_id);
///         // Merge signatures and properties
///     }
///     InterfaceMergeKind::Object(shape_id) => {
///         let shape = db.object_shape(shape_id);
///         // Merge properties only
///     }
///     InterfaceMergeKind::ObjectWithIndex(shape_id) => {
///         let shape = db.object_shape(shape_id);
///         // Merge properties and index signatures
///     }
///     InterfaceMergeKind::Intersection => {
///         // Create intersection with base type
///     }
///     InterfaceMergeKind::Other => {
///         // Return derived unchanged
///     }
/// }
/// ```
pub fn classify_for_interface_merge(db: &dyn TypeDatabase, type_id: TypeId) -> InterfaceMergeKind {
    let Some(key) = db.lookup(type_id) else {
        return InterfaceMergeKind::Other;
    };

    match key {
        TypeKey::Callable(shape_id) => InterfaceMergeKind::Callable(shape_id),
        TypeKey::Object(shape_id) => InterfaceMergeKind::Object(shape_id),
        TypeKey::ObjectWithIndex(shape_id) => InterfaceMergeKind::ObjectWithIndex(shape_id),
        TypeKey::Intersection(_) => InterfaceMergeKind::Intersection,
        // All other types cannot be structurally merged for interfaces
        TypeKey::Intrinsic(_)
        | TypeKey::Literal(_)
        | TypeKey::Union(_)
        | TypeKey::Array(_)
        | TypeKey::Tuple(_)
        | TypeKey::Function(_)
        | TypeKey::TypeParameter(_)
        | TypeKey::Infer(_)
        | TypeKey::Lazy(_)
        | TypeKey::Application(_)
        | TypeKey::Conditional(_)
        | TypeKey::Mapped(_)
        | TypeKey::IndexAccess(_, _)
        | TypeKey::KeyOf(_)
        | TypeKey::TemplateLiteral(_)
        | TypeKey::UniqueSymbol(_)
        | TypeKey::ThisType
        | TypeKey::TypeQuery(_)
        | TypeKey::ReadonlyType(_)
        | TypeKey::StringIntrinsic { .. }
        | TypeKey::ModuleNamespace(_)
        | TypeKey::Error
        | TypeKey::Enum(_, _) => InterfaceMergeKind::Other,
    }
}

/// Classification for augmentation operations on types.
///
/// Similar to InterfaceMergeKind but specifically for module augmentation
/// where we merge additional properties into an existing type.
#[derive(Debug, Clone)]
pub enum AugmentationTargetKind {
    /// Object type - merge properties directly
    Object(crate::solver::types::ObjectShapeId),
    /// Object with index signatures - preserve index signatures when merging
    ObjectWithIndex(crate::solver::types::ObjectShapeId),
    /// Callable type - merge properties while preserving signatures
    Callable(crate::solver::types::CallableShapeId),
    /// Other type - create new object with augmentation members
    Other,
}

/// Classify a type for augmentation operations.
///
/// This function examines a type and returns information about how to handle it
/// when applying module augmentations. Used by `apply_module_augmentations`.
pub fn classify_for_augmentation(db: &dyn TypeDatabase, type_id: TypeId) -> AugmentationTargetKind {
    let Some(key) = db.lookup(type_id) else {
        return AugmentationTargetKind::Other;
    };

    match key {
        TypeKey::Object(shape_id) => AugmentationTargetKind::Object(shape_id),
        TypeKey::ObjectWithIndex(shape_id) => AugmentationTargetKind::ObjectWithIndex(shape_id),
        TypeKey::Callable(shape_id) => AugmentationTargetKind::Callable(shape_id),
        // All other types are treated as Other for augmentation
        _ => AugmentationTargetKind::Other,
    }
}

// =============================================================================
// Control Flow Type Classification Helpers
// =============================================================================

/// Classification for type predicate signature extraction.
/// Used by control flow analysis to extract predicate signatures from callable types.
#[derive(Debug, Clone)]
pub enum PredicateSignatureKind {
    /// Function type - has type_predicate and params in function shape
    Function(crate::solver::types::FunctionShapeId),
    /// Callable type - check call_signatures for predicate
    Callable(crate::solver::types::CallableShapeId),
    /// Union - search members for predicate
    Union(Vec<TypeId>),
    /// Intersection - search members for predicate
    Intersection(Vec<TypeId>),
    /// No predicate available
    None,
}

/// Classify a type for predicate signature extraction.
pub fn classify_for_predicate_signature(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> PredicateSignatureKind {
    let Some(key) = db.lookup(type_id) else {
        return PredicateSignatureKind::None;
    };

    match key {
        TypeKey::Function(shape_id) => PredicateSignatureKind::Function(shape_id),
        TypeKey::Callable(shape_id) => PredicateSignatureKind::Callable(shape_id),
        TypeKey::Union(members_id) => {
            let members = db.type_list(members_id);
            PredicateSignatureKind::Union(members.to_vec())
        }
        TypeKey::Intersection(members_id) => {
            let members = db.type_list(members_id);
            PredicateSignatureKind::Intersection(members.to_vec())
        }
        _ => PredicateSignatureKind::None,
    }
}

/// Classification for constructor instance type extraction.
/// Used by instanceof narrowing to get the instance type from a constructor.
#[derive(Debug, Clone)]
pub enum ConstructorInstanceKind {
    /// Callable type with construct signatures
    Callable(crate::solver::types::CallableShapeId),
    /// Union - search members for construct signatures
    Union(Vec<TypeId>),
    /// Not a constructor type
    None,
}

/// Classify a type for constructor instance type extraction.
pub fn classify_for_constructor_instance(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> ConstructorInstanceKind {
    let Some(key) = db.lookup(type_id) else {
        return ConstructorInstanceKind::None;
    };

    match key {
        TypeKey::Callable(shape_id) => ConstructorInstanceKind::Callable(shape_id),
        TypeKey::Union(members_id) => {
            let members = db.type_list(members_id);
            ConstructorInstanceKind::Union(members.to_vec())
        }
        _ => ConstructorInstanceKind::None,
    }
}

/// Classification for type parameter constraint access.
/// Used by narrowing to check if a type has a constraint to narrow.
#[derive(Debug, Clone)]
pub enum TypeParameterConstraintKind {
    /// Type parameter with constraint
    TypeParameter { constraint: Option<TypeId> },
    /// Not a type parameter
    None,
}

/// Classify a type to check if it's a type parameter with a constraint.
pub fn classify_for_type_parameter_constraint(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> TypeParameterConstraintKind {
    let Some(key) = db.lookup(type_id) else {
        return TypeParameterConstraintKind::None;
    };

    match key {
        TypeKey::TypeParameter(info) | TypeKey::Infer(info) => {
            TypeParameterConstraintKind::TypeParameter {
                constraint: info.constraint,
            }
        }
        _ => TypeParameterConstraintKind::None,
    }
}

/// Classification for union member access.
/// Used by narrowing to filter union members.
#[derive(Debug, Clone)]
pub enum UnionMembersKind {
    /// Union with members
    Union(Vec<TypeId>),
    /// Not a union
    NotUnion,
}

/// Classify a type to check if it's a union and get its members.
pub fn classify_for_union_members(db: &dyn TypeDatabase, type_id: TypeId) -> UnionMembersKind {
    let Some(key) = db.lookup(type_id) else {
        return UnionMembersKind::NotUnion;
    };

    match key {
        TypeKey::Union(members_id) => {
            let members = db.type_list(members_id);
            UnionMembersKind::Union(members.to_vec())
        }
        _ => UnionMembersKind::NotUnion,
    }
}

/// Classification for checking if a type is definitely not an object.
/// Used by instanceof and typeof narrowing.
#[derive(Debug, Clone)]
pub enum NonObjectKind {
    /// Literal type (always non-object)
    Literal,
    /// Intrinsic primitive type (void, undefined, null, boolean, number, string, bigint, symbol, never)
    IntrinsicPrimitive,
    /// Object or potentially object type
    MaybeObject,
}

/// Classify a type to check if it's definitely not an object.
pub fn classify_for_non_object(db: &dyn TypeDatabase, type_id: TypeId) -> NonObjectKind {
    let Some(key) = db.lookup(type_id) else {
        return NonObjectKind::MaybeObject;
    };

    match key {
        TypeKey::Literal(_) => NonObjectKind::Literal,
        TypeKey::Intrinsic(kind) => {
            use crate::solver::IntrinsicKind;

            match kind {
                IntrinsicKind::Void
                | IntrinsicKind::Undefined
                | IntrinsicKind::Null
                | IntrinsicKind::Boolean
                | IntrinsicKind::Number
                | IntrinsicKind::String
                | IntrinsicKind::Bigint
                | IntrinsicKind::Symbol
                | IntrinsicKind::Never => NonObjectKind::IntrinsicPrimitive,
                _ => NonObjectKind::MaybeObject,
            }
        }
        _ => NonObjectKind::MaybeObject,
    }
}

/// Classification for property presence checking.
/// Used by 'in' operator narrowing.
#[derive(Debug, Clone)]
pub enum PropertyPresenceKind {
    /// Intrinsic object type (unknown properties)
    IntrinsicObject,
    /// Object with shape - check properties
    Object(crate::solver::types::ObjectShapeId),
    /// Callable with properties
    Callable(crate::solver::types::CallableShapeId),
    /// Array or Tuple - numeric access
    ArrayLike,
    /// Unknown property presence
    Unknown,
}

/// Classify a type for property presence checking.
pub fn classify_for_property_presence(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> PropertyPresenceKind {
    let Some(key) = db.lookup(type_id) else {
        return PropertyPresenceKind::Unknown;
    };

    match key {
        TypeKey::Intrinsic(crate::solver::IntrinsicKind::Object) => {
            PropertyPresenceKind::IntrinsicObject
        }
        TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id) => {
            PropertyPresenceKind::Object(shape_id)
        }
        TypeKey::Callable(shape_id) => PropertyPresenceKind::Callable(shape_id),
        TypeKey::Array(_) | TypeKey::Tuple(_) => PropertyPresenceKind::ArrayLike,
        _ => PropertyPresenceKind::Unknown,
    }
}

/// Classification for falsy component extraction.
/// Used by truthiness narrowing.
#[derive(Debug, Clone)]
pub enum FalsyComponentKind {
    /// Literal type - check if falsy value
    Literal(crate::solver::LiteralValue),
    /// Union - get falsy component from each member
    Union(Vec<TypeId>),
    /// Type parameter or infer - keep as is
    TypeParameter,
    /// Other types - no falsy component
    None,
}

/// Classify a type for falsy component extraction.
pub fn classify_for_falsy_component(db: &dyn TypeDatabase, type_id: TypeId) -> FalsyComponentKind {
    let Some(key) = db.lookup(type_id) else {
        return FalsyComponentKind::None;
    };

    match key {
        TypeKey::Literal(literal) => FalsyComponentKind::Literal(literal),
        TypeKey::Union(members_id) => {
            let members = db.type_list(members_id);
            FalsyComponentKind::Union(members.to_vec())
        }
        TypeKey::TypeParameter(_) | TypeKey::Infer(_) => FalsyComponentKind::TypeParameter,
        _ => FalsyComponentKind::None,
    }
}

/// Classification for literal value extraction.
/// Used by element access and property access narrowing.
#[derive(Debug, Clone)]
pub enum LiteralValueKind {
    /// String literal
    String(crate::interner::Atom),
    /// Number literal
    Number(f64),
    /// Not a literal
    None,
}

/// Classify a type to extract literal value (string or number).
pub fn classify_for_literal_value(db: &dyn TypeDatabase, type_id: TypeId) -> LiteralValueKind {
    let Some(key) = db.lookup(type_id) else {
        return LiteralValueKind::None;
    };

    match key {
        TypeKey::Literal(crate::solver::LiteralValue::String(atom)) => {
            LiteralValueKind::String(atom)
        }
        TypeKey::Literal(crate::solver::LiteralValue::Number(num)) => {
            LiteralValueKind::Number(num.0)
        }
        _ => LiteralValueKind::None,
    }
}

/// Extracts the return type from a callable type for declaration emit.
///
/// For overloaded functions (Callable), returns the return type of the first signature.
/// For intersections, finds the first callable member and extracts its return type.
///
/// # Examples
///
/// ```ignore
/// let return_type = type_queries::get_return_type(&db, function_type_id);
/// ```
///
/// # Arguments
///
/// * `db` - The type database/interner
/// * `type_id` - The TypeId of a function or callable type
///
/// # Returns
///
/// * `Some(TypeId)` - The return type if this is a callable type
/// * `None` - If this is not a callable type or type_id is unknown
pub fn get_return_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    match db.lookup(type_id) {
        Some(TypeKey::Function(shape_id)) => Some(db.function_shape(shape_id).return_type),
        Some(TypeKey::Callable(shape_id)) => {
            let shape = db.callable_shape(shape_id);
            // For overloads, use the first signature's return type
            shape.call_signatures.first().map(|sig| sig.return_type)
        }
        Some(TypeKey::Intersection(list_id)) => {
            // In an intersection, find the first callable member
            let members = db.type_list(list_id);
            members.iter().find_map(|&m| get_return_type(db, m))
        }
        _ => {
            // Handle special intrinsic types
            if type_id == TypeId::ANY {
                Some(TypeId::ANY)
            } else if type_id == TypeId::NEVER {
                Some(TypeId::NEVER)
            } else {
                None
            }
        }
    }
}

// =============================================================================
// Promise and Iterable Type Queries (Phase 5 - Anti-Pattern 8.1 Removal)
// =============================================================================

use crate::solver::operations_property::{PropertyAccessEvaluator, PropertyAccessResult};
use crate::solver::subtype::TypeResolver;

/// Check if a type is "promise-like" (has a callable 'then' method).
///
/// This is used to detect thenable types for async iterator handling.
/// A type is promise-like if it has a 'then' property that is callable.
///
/// # Arguments
///
/// * `db` - The type database/interner
/// * `resolver` - Type resolver for handling Lazy/Ref types
/// * `type_id` - The type to check
///
/// # Returns
///
/// * `true` - If the type is promise-like (has callable 'then')
/// * `false` - Otherwise
///
/// # Examples
///
/// ```ignore
/// // Promise<T> is promise-like
/// assert!(is_promise_like(&db, &resolver, promise_type));
///
/// // any is always promise-like
/// assert!(is_promise_like(&db, &resolver, TypeId::ANY));
///
/// // Objects with 'then' method are promise-like
/// // { then: (fn: (value: T) => void) => void }
/// ```
pub fn is_promise_like<R: TypeResolver>(
    db: &dyn TypeDatabase,
    resolver: &R,
    type_id: TypeId,
) -> bool {
    // The 'any' trap: any is always promise-like
    if type_id == TypeId::ANY {
        return true;
    }

    // Use PropertyAccessEvaluator to find 'then' property
    // This handles Lazy/Ref/Intersection/Readonly correctly
    let evaluator = PropertyAccessEvaluator::with_resolver(db, resolver);
    match evaluator.resolve_property_access(type_id, "then") {
        PropertyAccessResult::Success {
            type_id: then_type, ..
        } => {
            // 'then' must be invokable (have call signatures) to be "thenable"
            // A class with only construct signatures is not thenable
            is_invokable_type(db, then_type)
        }
        _ => false,
    }
}

/// Check if a type is a valid target for for...in loops.
///
/// In TypeScript, for...in loops work on object types, arrays, and type parameters.
/// This function validates that a type can be used in a for...in statement.
///
/// # Arguments
///
/// * `db` - The type database/interner
/// * `type_id` - The type to check
///
/// # Returns
///
/// * `true` - If valid for for...in (Object, Array, TypeParameter, Any)
/// * `false` - Otherwise
///
/// # Examples
///
/// ```ignore
/// // Objects are valid
/// assert!(is_valid_for_in_target(&db, object_type));
///
/// // Arrays are valid
/// assert!(is_valid_for_in_target(&db, array_type));
///
/// // Type parameters are valid (generic constraints)
/// assert!(is_valid_for_in_target(&db, type_param_type));
///
/// // Primitives (except any) are not valid
/// assert!(!is_valid_for_in_target(&db, TypeId::STRING));
/// ```
pub fn is_valid_for_in_target(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    // Any is always valid
    if type_id == TypeId::ANY {
        return true;
    }

    // Primitives are valid (they box to objects in JS for...in)
    if type_id == TypeId::STRING || type_id == TypeId::NUMBER || type_id == TypeId::BOOLEAN {
        return true;
    }

    use crate::solver::types::IntrinsicKind;
    match db.lookup(type_id) {
        // Object types are valid (for...in iterates properties)
        Some(TypeKey::Object(_) | TypeKey::ObjectWithIndex(_)) => true,
        // Array types are valid (for...in iterates indices)
        Some(TypeKey::Array(_)) => true,
        // Type parameters are valid (we don't know the constraint)
        Some(TypeKey::TypeParameter(_)) => true,
        // Tuples are valid (they're objects)
        Some(TypeKey::Tuple(_)) => true,
        // Unions are valid if all members are valid
        Some(TypeKey::Union(list_id)) => {
            let members = db.type_list(list_id);
            members.iter().all(|&m| is_valid_for_in_target(db, m))
        }
        // Intersections are valid if any member is valid
        Some(TypeKey::Intersection(list_id)) => {
            let members = db.type_list(list_id);
            members.iter().any(|&m| is_valid_for_in_target(db, m))
        }
        // Literals are valid (they box to objects)
        Some(TypeKey::Literal(_)) => true,
        // Intrinsic primitives
        Some(TypeKey::Intrinsic(kind)) => matches!(
            kind,
            IntrinsicKind::String
                | IntrinsicKind::Number
                | IntrinsicKind::Boolean
                | IntrinsicKind::Symbol
        ),
        // Everything else is not valid for for...in
        _ => false,
    }
}
