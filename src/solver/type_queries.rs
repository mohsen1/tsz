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
            TypeKey::Ref(_)
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
        TypeKey::Intrinsic(_)
        | TypeKey::Literal(_)
        | TypeKey::Object(_)
        | TypeKey::ObjectWithIndex(_)
        | TypeKey::Array(_)
        | TypeKey::Tuple(_)
        | TypeKey::Ref(_)
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

/// Get the symbol reference from a Ref or TypeQuery type.
///
/// Returns None if the type is not a Ref or TypeQuery.
pub fn get_symbol_ref_from_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<crate::solver::types::SymbolRef> {
    match db.lookup(type_id) {
        Some(TypeKey::Ref(sym_ref)) | Some(TypeKey::TypeQuery(sym_ref)) => Some(sym_ref),
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
        TypeKey::Ref(sym_ref) => ConstructableTypeKind::SymbolRef(sym_ref),
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
        TypeKey::Ref(sym_ref) => ConstraintTypeKind::SymbolRef(sym_ref),
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
        | TypeKey::Ref(_)
        | TypeKey::Application(_)
        | TypeKey::TemplateLiteral(_)
        | TypeKey::UniqueSymbol(_)
        | TypeKey::ThisType
        | TypeKey::TypeQuery(_)
        | TypeKey::StringIntrinsic { .. }
        | TypeKey::ModuleNamespace(_)
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
        | TypeKey::Ref(_)
        | TypeKey::TemplateLiteral(_)
        | TypeKey::UniqueSymbol(_)
        | TypeKey::ThisType
        | TypeKey::TypeQuery(_)
        | TypeKey::KeyOf(_)
        | TypeKey::StringIntrinsic { .. }
        | TypeKey::ModuleNamespace(_)
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
        | TypeKey::Ref(_)
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
        TypeKey::Ref(sym_ref) => EvaluationNeeded::SymbolRef(sym_ref),
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
        // Already resolved types
        TypeKey::Intrinsic(_)
        | TypeKey::Literal(_)
        | TypeKey::Object(_)
        | TypeKey::ObjectWithIndex(_)
        | TypeKey::Array(_)
        | TypeKey::Tuple(_)
        | TypeKey::TemplateLiteral(_)
        | TypeKey::UniqueSymbol(_)
        | TypeKey::ThisType
        | TypeKey::StringIntrinsic { .. }
        | TypeKey::ModuleNamespace(_)
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
        TypeKey::Ref(sym_ref) => PropertyAccessClassification::SymbolRef(sym_ref),
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
        // Primitives and resolved types
        TypeKey::Intrinsic(_)
        | TypeKey::Literal(_)
        | TypeKey::Array(_)
        | TypeKey::Tuple(_)
        | TypeKey::TemplateLiteral(_)
        | TypeKey::UniqueSymbol(_)
        | TypeKey::ThisType
        | TypeKey::StringIntrinsic { .. }
        | TypeKey::ModuleNamespace(_)
        | TypeKey::Error => PropertyAccessClassification::Resolved(type_id),
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
        TypeKey::Ref(sym_ref) => TypeTraversalKind::SymbolRef(sym_ref),
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
        // Terminal types - no nested types to traverse
        TypeKey::Intrinsic(_)
        | TypeKey::Literal(_)
        | TypeKey::TemplateLiteral(_)
        | TypeKey::UniqueSymbol(_)
        | TypeKey::ThisType
        | TypeKey::TypeQuery(_)
        | TypeKey::StringIntrinsic { .. }
        | TypeKey::ModuleNamespace(_)
        | TypeKey::Error => TypeTraversalKind::Terminal,
    }
}

/// Check if a type is a symbol reference and return the symbol ref.
///
/// This is a helper for checking if the base of an Application is a Ref.
pub fn get_ref_if_symbol(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<crate::solver::types::SymbolRef> {
    match db.lookup(type_id) {
        Some(TypeKey::Ref(sym_ref)) => Some(sym_ref),
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
        | TypeKey::Ref(_)
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
        | TypeKey::Error => InterfaceMergeKind::Other,
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

// =============================================================================
// Full Literal Type Classification (includes boolean)
// =============================================================================

/// Classification for all literal types including boolean.
/// Used by literal_type.rs for comprehensive literal handling.
#[derive(Debug, Clone)]
pub enum LiteralTypeKind {
    /// String literal type with the atom for the string value
    String(crate::interner::Atom),
    /// Number literal type with the numeric value
    Number(f64),
    /// BigInt literal type with the atom for the bigint value
    BigInt(crate::interner::Atom),
    /// Boolean literal type with the boolean value
    Boolean(bool),
    /// Not a literal type
    NotLiteral,
}

/// Classify a type for literal type handling.
///
/// This function examines a type and returns information about what kind
/// of literal it is. Used for:
/// - Detecting string/number/boolean literals
/// - Extracting literal values
/// - Literal type comparison
pub fn classify_literal_type(db: &dyn TypeDatabase, type_id: TypeId) -> LiteralTypeKind {
    let Some(key) = db.lookup(type_id) else {
        return LiteralTypeKind::NotLiteral;
    };

    match key {
        TypeKey::Literal(crate::solver::LiteralValue::String(atom)) => {
            LiteralTypeKind::String(atom)
        }
        TypeKey::Literal(crate::solver::LiteralValue::Number(ordered_float)) => {
            LiteralTypeKind::Number(ordered_float.0)
        }
        TypeKey::Literal(crate::solver::LiteralValue::BigInt(atom)) => {
            LiteralTypeKind::BigInt(atom)
        }
        TypeKey::Literal(crate::solver::LiteralValue::Boolean(value)) => {
            LiteralTypeKind::Boolean(value)
        }
        _ => LiteralTypeKind::NotLiteral,
    }
}

/// Check if a type is a string literal type.
pub fn is_string_literal(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        classify_literal_type(db, type_id),
        LiteralTypeKind::String(_)
    )
}

/// Check if a type is a number literal type.
pub fn is_number_literal(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        classify_literal_type(db, type_id),
        LiteralTypeKind::Number(_)
    )
}

/// Check if a type is a boolean literal type.
pub fn is_boolean_literal(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        classify_literal_type(db, type_id),
        LiteralTypeKind::Boolean(_)
    )
}

/// Get string atom from a string literal type.
pub fn get_string_literal_atom(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<crate::interner::Atom> {
    match classify_literal_type(db, type_id) {
        LiteralTypeKind::String(atom) => Some(atom),
        _ => None,
    }
}

/// Get number value from a number literal type.
pub fn get_number_literal_value(db: &dyn TypeDatabase, type_id: TypeId) -> Option<f64> {
    match classify_literal_type(db, type_id) {
        LiteralTypeKind::Number(value) => Some(value),
        _ => None,
    }
}

/// Get boolean value from a boolean literal type.
pub fn get_boolean_literal_value(db: &dyn TypeDatabase, type_id: TypeId) -> Option<bool> {
    match classify_literal_type(db, type_id) {
        LiteralTypeKind::Boolean(value) => Some(value),
        _ => None,
    }
}

// =============================================================================
// Spread Type Classification
// =============================================================================

/// Classification for spread operations.
///
/// This enum provides a structured way to handle spread types without
/// directly matching on TypeKey in the checker layer.
#[derive(Debug, Clone)]
pub enum SpreadTypeKind {
    /// Array type - element type for spread
    Array(TypeId),
    /// Tuple type - can expand individual elements
    Tuple(crate::solver::types::TupleListId),
    /// Object type - properties can be spread
    Object(crate::solver::types::ObjectShapeId),
    /// Object with index signature
    ObjectWithIndex(crate::solver::types::ObjectShapeId),
    /// String literal - can be spread as characters
    StringLiteral(crate::interner::Atom),
    /// Type that needs further checks for iterability
    Other,
    /// Type that cannot be spread
    NotSpreadable,
}

/// Classify a type for spread operations.
///
/// This function examines a type and returns information about how to handle it
/// when used in a spread context.
pub fn classify_spread_type(db: &dyn TypeDatabase, type_id: TypeId) -> SpreadTypeKind {
    // Handle intrinsic types first
    if type_id.is_any() || type_id == TypeId::STRING {
        return SpreadTypeKind::Other;
    }
    if type_id.is_unknown() {
        return SpreadTypeKind::NotSpreadable;
    }

    let Some(key) = db.lookup(type_id) else {
        return SpreadTypeKind::NotSpreadable;
    };

    match key {
        TypeKey::Array(element_type) => SpreadTypeKind::Array(element_type),
        TypeKey::Tuple(tuple_id) => SpreadTypeKind::Tuple(tuple_id),
        TypeKey::Object(shape_id) => SpreadTypeKind::Object(shape_id),
        TypeKey::ObjectWithIndex(shape_id) => SpreadTypeKind::ObjectWithIndex(shape_id),
        TypeKey::Literal(crate::solver::LiteralValue::String(atom)) => {
            SpreadTypeKind::StringLiteral(atom)
        }
        _ => SpreadTypeKind::Other,
    }
}

/// Check if a type has Symbol.iterator or is otherwise iterable.
///
/// This is a helper for checking iterability without matching on TypeKey.
pub fn is_iterable_type_kind(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    // Handle intrinsic string type
    if type_id == TypeId::STRING {
        return true;
    }

    let Some(key) = db.lookup(type_id) else {
        return false;
    };

    match key {
        TypeKey::Array(_) | TypeKey::Tuple(_) => true,
        TypeKey::Literal(crate::solver::LiteralValue::String(_)) => true,
        TypeKey::Object(shape_id) => {
            // Check for [Symbol.iterator] method
            let shape = db.object_shape(shape_id);
            shape.properties.iter().any(|prop| {
                let prop_name = db.resolve_atom_ref(prop.name);
                (prop_name.as_ref() == "[Symbol.iterator]" || prop_name.as_ref() == "next")
                    && prop.is_method
            })
        }
        _ => false,
    }
}

/// Get the iterable element type for a type if it's iterable.
pub fn get_iterable_element_type_from_db(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    // Handle intrinsic string type
    if type_id == TypeId::STRING {
        return Some(TypeId::STRING);
    }

    let key = db.lookup(type_id)?;

    match key {
        TypeKey::Array(elem_type) => Some(elem_type),
        TypeKey::Tuple(tuple_list_id) => {
            let elements = db.tuple_list(tuple_list_id);
            if elements.is_empty() {
                Some(TypeId::NEVER)
            } else {
                let types: Vec<TypeId> = elements.iter().map(|e| e.type_id).collect();
                Some(db.union(types))
            }
        }
        TypeKey::Literal(crate::solver::LiteralValue::String(_)) => Some(TypeId::STRING),
        TypeKey::Object(shape_id) => {
            // For objects with [Symbol.iterator], we'd need to infer the element type
            // from the iterator's return type. For now, return Any as a fallback.
            let shape = db.object_shape(shape_id);
            let has_iterator = shape.properties.iter().any(|prop| {
                let prop_name = db.resolve_atom_ref(prop.name);
                (prop_name.as_ref() == "[Symbol.iterator]" || prop_name.as_ref() == "next")
                    && prop.is_method
            });
            if has_iterator {
                Some(TypeId::ANY)
            } else {
                None
            }
        }
        _ => None,
    }
}

// =============================================================================
// Type Parameter Classification (Extended)
// =============================================================================

/// Classification for type parameter types.
///
/// This enum provides a structured way to handle type parameters without
/// directly matching on TypeKey in the checker layer.
#[derive(Debug, Clone)]
pub enum TypeParameterKind {
    /// Type parameter with info
    TypeParameter(crate::solver::types::TypeParamInfo),
    /// Infer type with info
    Infer(crate::solver::types::TypeParamInfo),
    /// Type application - may contain type parameters
    Application(crate::solver::types::TypeApplicationId),
    /// Union - may contain type parameters in members
    Union(Vec<TypeId>),
    /// Intersection - may contain type parameters in members
    Intersection(Vec<TypeId>),
    /// Callable - may have type parameters
    Callable(crate::solver::types::CallableShapeId),
    /// Not a type parameter or type containing type parameters
    NotTypeParameter,
}

/// Classify a type for type parameter handling.
///
/// Returns detailed information about type parameter types.
pub fn classify_type_parameter(db: &dyn TypeDatabase, type_id: TypeId) -> TypeParameterKind {
    let Some(key) = db.lookup(type_id) else {
        return TypeParameterKind::NotTypeParameter;
    };

    match key {
        TypeKey::TypeParameter(info) => TypeParameterKind::TypeParameter(info.clone()),
        TypeKey::Infer(info) => TypeParameterKind::Infer(info.clone()),
        TypeKey::Application(app_id) => TypeParameterKind::Application(app_id),
        TypeKey::Union(list_id) => {
            let members = db.type_list(list_id);
            TypeParameterKind::Union(members.to_vec())
        }
        TypeKey::Intersection(list_id) => {
            let members = db.type_list(list_id);
            TypeParameterKind::Intersection(members.to_vec())
        }
        TypeKey::Callable(shape_id) => TypeParameterKind::Callable(shape_id),
        _ => TypeParameterKind::NotTypeParameter,
    }
}

/// Check if a type is directly a type parameter (TypeParameter or Infer).
pub fn is_direct_type_parameter(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        classify_type_parameter(db, type_id),
        TypeParameterKind::TypeParameter(_) | TypeParameterKind::Infer(_)
    )
}

/// Get the type parameter default if this is a type parameter.
pub fn get_type_param_default(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    match db.lookup(type_id) {
        Some(TypeKey::TypeParameter(info)) | Some(TypeKey::Infer(info)) => info.default,
        _ => None,
    }
}

/// Get the callable type parameter count.
pub fn get_callable_type_param_count(db: &dyn TypeDatabase, type_id: TypeId) -> usize {
    match db.lookup(type_id) {
        Some(TypeKey::Callable(shape_id)) => {
            let shape = db.callable_shape(shape_id);
            shape
                .call_signatures
                .iter()
                .map(|sig| sig.type_params.len())
                .max()
                .unwrap_or(0)
        }
        _ => 0,
    }
}

// =============================================================================
// Promise Type Classification
// =============================================================================

/// Classification for promise-like types.
///
/// This enum provides a structured way to handle promise types without
/// directly matching on TypeKey in the checker layer.
#[derive(Debug, Clone)]
pub enum PromiseTypeKind {
    /// Type application (like Promise<T>) - contains base and args
    Application {
        app_id: crate::solver::types::TypeApplicationId,
        base: TypeId,
        args: Vec<TypeId>,
    },
    /// Symbol reference (like Promise or PromiseLike)
    SymbolRef(crate::solver::types::SymbolRef),
    /// Object type (might be Promise interface from lib)
    Object(crate::solver::types::ObjectShapeId),
    /// Union type - check each member
    Union(Vec<TypeId>),
    /// Not a promise type
    NotPromise,
}

/// Classify a type for promise handling.
///
/// This function examines a type and returns information about how to handle it
/// when checking for promise-like types.
pub fn classify_promise_type(db: &dyn TypeDatabase, type_id: TypeId) -> PromiseTypeKind {
    let Some(key) = db.lookup(type_id) else {
        return PromiseTypeKind::NotPromise;
    };

    match key {
        TypeKey::Application(app_id) => {
            let app = db.type_application(app_id);
            PromiseTypeKind::Application {
                app_id,
                base: app.base,
                args: app.args.clone(),
            }
        }
        TypeKey::Ref(sym_ref) => PromiseTypeKind::SymbolRef(sym_ref),
        TypeKey::Object(shape_id) => PromiseTypeKind::Object(shape_id),
        TypeKey::Union(list_id) => {
            let members = db.type_list(list_id);
            PromiseTypeKind::Union(members.to_vec())
        }
        _ => PromiseTypeKind::NotPromise,
    }
}

// =============================================================================
// New Expression Type Classification
// =============================================================================

/// Classification for types in `new` expressions.
#[derive(Debug, Clone)]
pub enum NewExpressionTypeKind {
    /// Callable type - check for construct signatures
    Callable(crate::solver::types::CallableShapeId),
    /// Function type - always constructable
    Function(crate::solver::types::FunctionShapeId),
    /// Symbol reference - resolve the symbol
    SymbolRef(crate::solver::types::SymbolRef),
    /// TypeQuery (typeof X) - needs symbol resolution
    TypeQuery(crate::solver::types::SymbolRef),
    /// Intersection type - check all members for construct signatures
    Intersection(Vec<TypeId>),
    /// Union type - all members must be constructable
    Union(Vec<TypeId>),
    /// Type parameter with constraint
    TypeParameter { constraint: Option<TypeId> },
    /// Not constructable
    NotConstructable,
}

/// Classify a type for new expression handling.
pub fn classify_for_new_expression(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> NewExpressionTypeKind {
    let Some(key) = db.lookup(type_id) else {
        return NewExpressionTypeKind::NotConstructable;
    };

    match key {
        TypeKey::Callable(shape_id) => NewExpressionTypeKind::Callable(shape_id),
        TypeKey::Function(shape_id) => NewExpressionTypeKind::Function(shape_id),
        TypeKey::Ref(sym_ref) => NewExpressionTypeKind::SymbolRef(sym_ref),
        TypeKey::TypeQuery(sym_ref) => NewExpressionTypeKind::TypeQuery(sym_ref),
        TypeKey::Intersection(list_id) => {
            let members = db.type_list(list_id);
            NewExpressionTypeKind::Intersection(members.to_vec())
        }
        TypeKey::Union(list_id) => {
            let members = db.type_list(list_id);
            NewExpressionTypeKind::Union(members.to_vec())
        }
        TypeKey::TypeParameter(info) | TypeKey::Infer(info) => {
            NewExpressionTypeKind::TypeParameter {
                constraint: info.constraint,
            }
        }
        TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id) => {
            // Objects might contain callable properties that represent construct signatures
            // Check if the object has a "new" property or if any property is callable with construct signatures
            let shape = db.object_shape(shape_id);
            for prop in shape.properties.iter() {
                // Check if this property is a callable type with construct signatures
                if let Some(TypeKey::Callable(callable_shape_id)) = db.lookup(prop.type_id) {
                    let callable_shape = db.callable_shape(callable_shape_id);
                    if !callable_shape.construct_signatures.is_empty() {
                        // Found a callable property with construct signatures
                        return NewExpressionTypeKind::Callable(callable_shape_id);
                    }
                }
            }
            NewExpressionTypeKind::NotConstructable
        }
        _ => NewExpressionTypeKind::NotConstructable,
    }
}

// =============================================================================
// Abstract Class Type Classification
// =============================================================================

/// Classification for checking if a type contains abstract classes.
#[derive(Debug, Clone)]
pub enum AbstractClassCheckKind {
    /// TypeQuery - check if symbol is abstract
    TypeQuery(crate::solver::types::SymbolRef),
    /// Union - check if any member is abstract
    Union(Vec<TypeId>),
    /// Intersection - check if any member is abstract
    Intersection(Vec<TypeId>),
    /// Other type - not an abstract class
    NotAbstract,
}

/// Classify a type for abstract class checking.
pub fn classify_for_abstract_check(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> AbstractClassCheckKind {
    let Some(key) = db.lookup(type_id) else {
        return AbstractClassCheckKind::NotAbstract;
    };

    match key {
        TypeKey::TypeQuery(sym_ref) => AbstractClassCheckKind::TypeQuery(sym_ref),
        TypeKey::Union(list_id) => {
            let members = db.type_list(list_id);
            AbstractClassCheckKind::Union(members.to_vec())
        }
        TypeKey::Intersection(list_id) => {
            let members = db.type_list(list_id);
            AbstractClassCheckKind::Intersection(members.to_vec())
        }
        _ => AbstractClassCheckKind::NotAbstract,
    }
}

// =============================================================================
// Construct Signature Return Type Classification
// =============================================================================

/// Classification for extracting construct signature return types.
#[derive(Debug, Clone)]
pub enum ConstructSignatureKind {
    /// Callable type with potential construct signatures
    Callable(crate::solver::types::CallableShapeId),
    /// Symbol reference - may be a class
    Ref(crate::solver::types::SymbolRef),
    /// TypeQuery (typeof X) - check if class
    TypeQuery(crate::solver::types::SymbolRef),
    /// Application type - needs evaluation
    Application(crate::solver::types::TypeApplicationId),
    /// Union - all members must have construct signatures
    Union(Vec<TypeId>),
    /// Intersection - any member with construct signature is sufficient
    Intersection(Vec<TypeId>),
    /// Type parameter with constraint
    TypeParameter { constraint: Option<TypeId> },
    /// Function type - check is_constructor flag
    Function(crate::solver::types::FunctionShapeId),
    /// No construct signatures available
    NoConstruct,
}

/// Classify a type for construct signature extraction.
pub fn classify_for_construct_signature(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> ConstructSignatureKind {
    let Some(key) = db.lookup(type_id) else {
        return ConstructSignatureKind::NoConstruct;
    };

    match key {
        TypeKey::Callable(shape_id) => ConstructSignatureKind::Callable(shape_id),
        TypeKey::Ref(sym_ref) => ConstructSignatureKind::Ref(sym_ref),
        TypeKey::TypeQuery(sym_ref) => ConstructSignatureKind::TypeQuery(sym_ref),
        TypeKey::Application(app_id) => ConstructSignatureKind::Application(app_id),
        TypeKey::Union(list_id) => {
            let members = db.type_list(list_id);
            ConstructSignatureKind::Union(members.to_vec())
        }
        TypeKey::Intersection(list_id) => {
            let members = db.type_list(list_id);
            ConstructSignatureKind::Intersection(members.to_vec())
        }
        TypeKey::TypeParameter(info) | TypeKey::Infer(info) => {
            ConstructSignatureKind::TypeParameter {
                constraint: info.constraint,
            }
        }
        TypeKey::Function(shape_id) => ConstructSignatureKind::Function(shape_id),
        _ => ConstructSignatureKind::NoConstruct,
    }
}

// =============================================================================
// KeyOf Type Classification
// =============================================================================

/// Classification for computing keyof types.
#[derive(Debug, Clone)]
pub enum KeyOfTypeKind {
    /// Object type with properties
    Object(crate::solver::types::ObjectShapeId),
    /// No keys available
    NoKeys,
}

/// Classify a type for keyof computation.
pub fn classify_for_keyof(db: &dyn TypeDatabase, type_id: TypeId) -> KeyOfTypeKind {
    let Some(key) = db.lookup(type_id) else {
        return KeyOfTypeKind::NoKeys;
    };

    match key {
        TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id) => {
            KeyOfTypeKind::Object(shape_id)
        }
        _ => KeyOfTypeKind::NoKeys,
    }
}

// =============================================================================
// String Literal Key Extraction
// =============================================================================

/// Classification for extracting string literal keys.
#[derive(Debug, Clone)]
pub enum StringLiteralKeyKind {
    /// Single string literal
    SingleString(crate::interner::Atom),
    /// Union of types - check each member
    Union(Vec<TypeId>),
    /// Not a string literal
    NotStringLiteral,
}

/// Classify a type for string literal key extraction.
pub fn classify_for_string_literal_keys(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> StringLiteralKeyKind {
    let Some(key) = db.lookup(type_id) else {
        return StringLiteralKeyKind::NotStringLiteral;
    };

    match key {
        TypeKey::Literal(crate::solver::types::LiteralValue::String(name)) => {
            StringLiteralKeyKind::SingleString(name)
        }
        TypeKey::Union(list_id) => {
            let members = db.type_list(list_id);
            StringLiteralKeyKind::Union(members.to_vec())
        }
        _ => StringLiteralKeyKind::NotStringLiteral,
    }
}

/// Extract string literal from a Literal type.
/// Returns None if not a string literal.
pub fn get_string_literal_value(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<crate::interner::Atom> {
    match db.lookup(type_id) {
        Some(TypeKey::Literal(crate::solver::types::LiteralValue::String(name))) => Some(name),
        _ => None,
    }
}

// =============================================================================
// Class Declaration from Type
// =============================================================================

/// Classification for extracting class declarations from types.
#[derive(Debug, Clone)]
pub enum ClassDeclTypeKind {
    /// Object type with properties (may have brand)
    Object(crate::solver::types::ObjectShapeId),
    /// Union/Intersection - check all members
    Members(Vec<TypeId>),
    /// Not an object type
    NotObject,
}

/// Classify a type for class declaration extraction.
pub fn classify_for_class_decl(db: &dyn TypeDatabase, type_id: TypeId) -> ClassDeclTypeKind {
    let Some(key) = db.lookup(type_id) else {
        return ClassDeclTypeKind::NotObject;
    };

    match key {
        TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id) => {
            ClassDeclTypeKind::Object(shape_id)
        }
        TypeKey::Union(list_id) | TypeKey::Intersection(list_id) => {
            let members = db.type_list(list_id);
            ClassDeclTypeKind::Members(members.to_vec())
        }
        _ => ClassDeclTypeKind::NotObject,
    }
}

// =============================================================================
// Call Expression Overload Classification
// =============================================================================

/// Classification for extracting call signatures from a type.
#[derive(Debug, Clone)]
pub enum CallSignaturesKind {
    /// Callable type with signatures
    Callable(crate::solver::types::CallableShapeId),
    /// Other type - no call signatures
    NoSignatures,
}

/// Classify a type for call signature extraction.
pub fn classify_for_call_signatures(db: &dyn TypeDatabase, type_id: TypeId) -> CallSignaturesKind {
    let Some(key) = db.lookup(type_id) else {
        return CallSignaturesKind::NoSignatures;
    };

    match key {
        TypeKey::Callable(shape_id) => CallSignaturesKind::Callable(shape_id),
        _ => CallSignaturesKind::NoSignatures,
    }
}

// =============================================================================
// Generic Application Type Extraction
// =============================================================================

/// Get the base and args from an Application type.
/// Returns None if not an Application.
pub fn get_application_info(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<(TypeId, Vec<TypeId>)> {
    match db.lookup(type_id) {
        Some(TypeKey::Application(app_id)) => {
            let app = db.type_application(app_id);
            Some((app.base, app.args.clone()))
        }
        _ => None,
    }
}

// =============================================================================
// Type Parameter Content Classification
// =============================================================================

/// Classification for types when checking for type parameters.
#[derive(Debug, Clone)]
pub enum TypeParameterContentKind {
    /// Is a type parameter or infer type
    IsTypeParameter,
    /// Array - check element type
    Array(TypeId),
    /// Tuple - check element types
    Tuple(crate::solver::types::TupleListId),
    /// Union - check all members
    Union(Vec<TypeId>),
    /// Intersection - check all members
    Intersection(Vec<TypeId>),
    /// Application - check base and args
    Application { base: TypeId, args: Vec<TypeId> },
    /// Not a type parameter and no nested types to check
    NotTypeParameter,
}

/// Classify a type for type parameter checking.
pub fn classify_for_type_parameter_content(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> TypeParameterContentKind {
    let Some(key) = db.lookup(type_id) else {
        return TypeParameterContentKind::NotTypeParameter;
    };

    match key {
        TypeKey::TypeParameter(_) | TypeKey::Infer(_) => TypeParameterContentKind::IsTypeParameter,
        TypeKey::Array(elem) => TypeParameterContentKind::Array(elem),
        TypeKey::Tuple(list_id) => TypeParameterContentKind::Tuple(list_id),
        TypeKey::Union(list_id) => {
            let members = db.type_list(list_id);
            TypeParameterContentKind::Union(members.to_vec())
        }
        TypeKey::Intersection(list_id) => {
            let members = db.type_list(list_id);
            TypeParameterContentKind::Intersection(members.to_vec())
        }
        TypeKey::Application(app_id) => {
            let app = db.type_application(app_id);
            TypeParameterContentKind::Application {
                base: app.base,
                args: app.args.clone(),
            }
        }
        _ => TypeParameterContentKind::NotTypeParameter,
    }
}

// =============================================================================
// Type Depth Classification
// =============================================================================

/// Classification for computing type depth.
#[derive(Debug, Clone)]
pub enum TypeDepthKind {
    /// Array - depth = 1 + element depth
    Array(TypeId),
    /// Tuple - depth = 1 + max element depth
    Tuple(crate::solver::types::TupleListId),
    /// Union or Intersection - depth = 1 + max member depth
    Members(Vec<TypeId>),
    /// Application - depth = 1 + max(base depth, arg depths)
    Application { base: TypeId, args: Vec<TypeId> },
    /// Terminal type - depth = 1
    Terminal,
}

/// Classify a type for depth computation.
pub fn classify_for_type_depth(db: &dyn TypeDatabase, type_id: TypeId) -> TypeDepthKind {
    let Some(key) = db.lookup(type_id) else {
        return TypeDepthKind::Terminal;
    };

    match key {
        TypeKey::Array(elem) => TypeDepthKind::Array(elem),
        TypeKey::Tuple(list_id) => TypeDepthKind::Tuple(list_id),
        TypeKey::Union(list_id) | TypeKey::Intersection(list_id) => {
            let members = db.type_list(list_id);
            TypeDepthKind::Members(members.to_vec())
        }
        TypeKey::Application(app_id) => {
            let app = db.type_application(app_id);
            TypeDepthKind::Application {
                base: app.base,
                args: app.args.clone(),
            }
        }
        _ => TypeDepthKind::Terminal,
    }
}

// =============================================================================
// Object Spread Property Classification
// =============================================================================

/// Classification for collecting properties from spread expressions.
#[derive(Debug, Clone)]
pub enum SpreadPropertyKind {
    /// Object type with properties
    Object(crate::solver::types::ObjectShapeId),
    /// Callable type with properties
    Callable(crate::solver::types::CallableShapeId),
    /// Intersection - collect from all members
    Intersection(Vec<TypeId>),
    /// No properties to spread
    NoProperties,
}

/// Classify a type for spread property collection.
pub fn classify_for_spread_properties(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> SpreadPropertyKind {
    let Some(key) = db.lookup(type_id) else {
        return SpreadPropertyKind::NoProperties;
    };

    match key {
        TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id) => {
            SpreadPropertyKind::Object(shape_id)
        }
        TypeKey::Callable(shape_id) => SpreadPropertyKind::Callable(shape_id),
        TypeKey::Intersection(list_id) => {
            let members = db.type_list(list_id);
            SpreadPropertyKind::Intersection(members.to_vec())
        }
        _ => SpreadPropertyKind::NoProperties,
    }
}

// =============================================================================
// Ref Type Resolution
// =============================================================================

/// Classification for Ref type resolution.
#[derive(Debug, Clone)]
pub enum RefTypeKind {
    /// Symbol reference - resolve to actual type
    Ref(crate::solver::types::SymbolRef),
    /// Not a Ref type
    NotRef,
}

/// Classify a type for Ref resolution.
pub fn classify_for_ref_resolution(db: &dyn TypeDatabase, type_id: TypeId) -> RefTypeKind {
    let Some(key) = db.lookup(type_id) else {
        return RefTypeKind::NotRef;
    };

    match key {
        TypeKey::Ref(sym_ref) => RefTypeKind::Ref(sym_ref),
        _ => RefTypeKind::NotRef,
    }
}

// =============================================================================
// Constructor Check Classification (for is_constructor_type)
// =============================================================================

/// Classification for checking if a type is a constructor type.
#[derive(Debug, Clone)]
pub enum ConstructorCheckKind {
    /// Type parameter with optional constraint - recurse into constraint
    TypeParameter { constraint: Option<TypeId> },
    /// Intersection type - check if any member is a constructor
    Intersection(Vec<TypeId>),
    /// Union type - check if all members are constructors
    Union(Vec<TypeId>),
    /// Application type - extract base and check
    Application { base: TypeId },
    /// Symbol reference - check symbol flags for CLASS
    SymbolRef(crate::solver::types::SymbolRef),
    /// TypeQuery (typeof) - check referenced symbol
    TypeQuery(crate::solver::types::SymbolRef),
    /// Not a constructor type or needs special handling
    Other,
}

/// Classify a type for constructor type checking.
pub fn classify_for_constructor_check(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> ConstructorCheckKind {
    let Some(key) = db.lookup(type_id) else {
        return ConstructorCheckKind::Other;
    };

    match key {
        TypeKey::TypeParameter(info) | TypeKey::Infer(info) => {
            ConstructorCheckKind::TypeParameter {
                constraint: info.constraint,
            }
        }
        TypeKey::Intersection(members_id) => {
            let members = db.type_list(members_id);
            ConstructorCheckKind::Intersection(members.to_vec())
        }
        TypeKey::Union(members_id) => {
            let members = db.type_list(members_id);
            ConstructorCheckKind::Union(members.to_vec())
        }
        TypeKey::Application(app_id) => {
            let app = db.type_application(app_id);
            ConstructorCheckKind::Application { base: app.base }
        }
        TypeKey::Ref(sym_ref) => ConstructorCheckKind::SymbolRef(sym_ref),
        TypeKey::TypeQuery(sym_ref) => ConstructorCheckKind::TypeQuery(sym_ref),
        _ => ConstructorCheckKind::Other,
    }
}

/// Check if a type is narrowable (union or type parameter).
pub fn is_narrowable_type_key(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        db.lookup(type_id),
        Some(TypeKey::Union(_) | TypeKey::TypeParameter(_) | TypeKey::Infer(_))
    )
}

// =============================================================================
// Private Brand Classification (for get_private_brand)
// =============================================================================

/// Classification for types when extracting private brands.
#[derive(Debug, Clone)]
pub enum PrivateBrandKind {
    /// Object type with shape_id - check properties for brand
    Object(crate::solver::types::ObjectShapeId),
    /// Callable type with shape_id - check properties for brand
    Callable(crate::solver::types::CallableShapeId),
    /// No private brand possible
    None,
}

/// Classify a type for private brand extraction.
pub fn classify_for_private_brand(db: &dyn TypeDatabase, type_id: TypeId) -> PrivateBrandKind {
    match db.lookup(type_id) {
        Some(TypeKey::Object(shape_id)) | Some(TypeKey::ObjectWithIndex(shape_id)) => {
            PrivateBrandKind::Object(shape_id)
        }
        Some(TypeKey::Callable(shape_id)) => PrivateBrandKind::Callable(shape_id),
        _ => PrivateBrandKind::None,
    }
}

/// Get the widened type for a literal type.
pub fn get_widened_literal_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    match db.lookup(type_id) {
        Some(TypeKey::Literal(crate::solver::LiteralValue::String(_))) => Some(TypeId::STRING),
        Some(TypeKey::Literal(crate::solver::LiteralValue::Number(_))) => Some(TypeId::NUMBER),
        Some(TypeKey::Literal(crate::solver::LiteralValue::BigInt(_))) => Some(TypeId::BIGINT),
        Some(TypeKey::Literal(crate::solver::LiteralValue::Boolean(_))) => Some(TypeId::BOOLEAN),
        _ => None,
    }
}

/// Get tuple elements list ID if the type is a tuple.
pub fn get_tuple_list_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<crate::solver::types::TupleListId> {
    match db.lookup(type_id) {
        Some(TypeKey::Tuple(list_id)) => Some(list_id),
        _ => None,
    }
}

/// Get the base type of an application type.
pub fn get_application_base(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    match db.lookup(type_id) {
        Some(TypeKey::Application(app_id)) => Some(db.type_application(app_id).base),
        _ => None,
    }
}

// =============================================================================
// Literal Key Classification (for get_literal_key_union_from_type)
// =============================================================================

/// Classification for literal key extraction from types.
#[derive(Debug, Clone)]
pub enum LiteralKeyKind {
    StringLiteral(crate::interner::Atom),
    NumberLiteral(f64),
    Union(Vec<TypeId>),
    Other,
}

/// Classify a type for literal key extraction.
pub fn classify_literal_key(db: &dyn TypeDatabase, type_id: TypeId) -> LiteralKeyKind {
    match db.lookup(type_id) {
        Some(TypeKey::Literal(crate::solver::LiteralValue::String(atom))) => {
            LiteralKeyKind::StringLiteral(atom)
        }
        Some(TypeKey::Literal(crate::solver::LiteralValue::Number(num))) => {
            LiteralKeyKind::NumberLiteral(num.0)
        }
        Some(TypeKey::Union(members_id)) => {
            LiteralKeyKind::Union(db.type_list(members_id).to_vec())
        }
        _ => LiteralKeyKind::Other,
    }
}

/// Get literal value from a type if it's a literal.
pub fn get_literal_value(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<crate::solver::LiteralValue> {
    match db.lookup(type_id) {
        Some(TypeKey::Literal(value)) => Some(value),
        _ => None,
    }
}

// =============================================================================
// Array-Like Type Classification (for is_array_like_type)
// =============================================================================

/// Classification for array-like types.
#[derive(Debug, Clone)]
pub enum ArrayLikeKind {
    Array(TypeId),
    Tuple,
    Readonly(TypeId),
    Union(Vec<TypeId>),
    Intersection(Vec<TypeId>),
    Other,
}

/// Classify a type for array-like checking.
pub fn classify_array_like(db: &dyn TypeDatabase, type_id: TypeId) -> ArrayLikeKind {
    match db.lookup(type_id) {
        Some(TypeKey::Array(elem)) => ArrayLikeKind::Array(elem),
        Some(TypeKey::Tuple(_)) => ArrayLikeKind::Tuple,
        Some(TypeKey::ReadonlyType(inner)) => ArrayLikeKind::Readonly(inner),
        Some(TypeKey::Union(members_id)) => ArrayLikeKind::Union(db.type_list(members_id).to_vec()),
        Some(TypeKey::Intersection(members_id)) => {
            ArrayLikeKind::Intersection(db.type_list(members_id).to_vec())
        }
        _ => ArrayLikeKind::Other,
    }
}

// =============================================================================
// Index Key Classification (for get_index_key_kind)
// =============================================================================

/// Classification for index key types.
#[derive(Debug, Clone)]
pub enum IndexKeyKind {
    String,
    Number,
    StringLiteral,
    NumberLiteral,
    Union(Vec<TypeId>),
    Other,
}

/// Classify a type for index key checking.
pub fn classify_index_key(db: &dyn TypeDatabase, type_id: TypeId) -> IndexKeyKind {
    match db.lookup(type_id) {
        Some(TypeKey::Intrinsic(crate::solver::IntrinsicKind::String)) => IndexKeyKind::String,
        Some(TypeKey::Intrinsic(crate::solver::IntrinsicKind::Number)) => IndexKeyKind::Number,
        Some(TypeKey::Literal(crate::solver::LiteralValue::String(_))) => {
            IndexKeyKind::StringLiteral
        }
        Some(TypeKey::Literal(crate::solver::LiteralValue::Number(_))) => {
            IndexKeyKind::NumberLiteral
        }
        Some(TypeKey::Union(members_id)) => IndexKeyKind::Union(db.type_list(members_id).to_vec()),
        _ => IndexKeyKind::Other,
    }
}

// =============================================================================
// Element Indexable Classification (for is_element_indexable_key)
// =============================================================================

/// Classification for element indexable types.
#[derive(Debug, Clone)]
pub enum ElementIndexableKind {
    Array,
    Tuple,
    ObjectWithIndex { has_string: bool, has_number: bool },
    Union(Vec<TypeId>),
    Intersection(Vec<TypeId>),
    StringLike,
    Other,
}

/// Classify a type for element indexing capability.
pub fn classify_element_indexable(db: &dyn TypeDatabase, type_id: TypeId) -> ElementIndexableKind {
    match db.lookup(type_id) {
        Some(TypeKey::Array(_)) => ElementIndexableKind::Array,
        Some(TypeKey::Tuple(_)) => ElementIndexableKind::Tuple,
        Some(TypeKey::ObjectWithIndex(shape_id)) => {
            let shape = db.object_shape(shape_id);
            ElementIndexableKind::ObjectWithIndex {
                has_string: shape.string_index.is_some(),
                has_number: shape.number_index.is_some(),
            }
        }
        Some(TypeKey::Union(members_id)) => {
            ElementIndexableKind::Union(db.type_list(members_id).to_vec())
        }
        Some(TypeKey::Intersection(members_id)) => {
            ElementIndexableKind::Intersection(db.type_list(members_id).to_vec())
        }
        Some(TypeKey::Literal(crate::solver::LiteralValue::String(_))) => {
            ElementIndexableKind::StringLike
        }
        Some(TypeKey::Intrinsic(crate::solver::IntrinsicKind::String)) => {
            ElementIndexableKind::StringLike
        }
        _ => ElementIndexableKind::Other,
    }
}

// =============================================================================
// Type Query Classification (for resolve_type_query_type)
// =============================================================================

/// Classification for type query resolution.
#[derive(Debug, Clone)]
pub enum TypeQueryKind {
    TypeQuery(crate::solver::types::SymbolRef),
    ApplicationWithTypeQuery {
        base_sym_ref: crate::solver::types::SymbolRef,
        args: Vec<TypeId>,
    },
    Application {
        app_id: crate::solver::types::TypeApplicationId,
    },
    Other,
}

/// Classify a type for type query resolution.
pub fn classify_type_query(db: &dyn TypeDatabase, type_id: TypeId) -> TypeQueryKind {
    match db.lookup(type_id) {
        Some(TypeKey::TypeQuery(sym_ref)) => TypeQueryKind::TypeQuery(sym_ref),
        Some(TypeKey::Application(app_id)) => {
            let app = db.type_application(app_id);
            match db.lookup(app.base) {
                Some(TypeKey::TypeQuery(base_sym_ref)) => TypeQueryKind::ApplicationWithTypeQuery {
                    base_sym_ref,
                    args: app.args.clone(),
                },
                _ => TypeQueryKind::Application { app_id },
            }
        }
        _ => TypeQueryKind::Other,
    }
}

// =============================================================================
// Symbol Reference Classification (for enum_symbol_from_value_type)
// =============================================================================

/// Classification for symbol reference types.
#[derive(Debug, Clone)]
pub enum SymbolRefKind {
    Ref(crate::solver::types::SymbolRef),
    TypeQuery(crate::solver::types::SymbolRef),
    Other,
}

/// Classify a type as a symbol reference.
pub fn classify_symbol_ref(db: &dyn TypeDatabase, type_id: TypeId) -> SymbolRefKind {
    match db.lookup(type_id) {
        Some(TypeKey::Ref(sym_ref)) => SymbolRefKind::Ref(sym_ref),
        Some(TypeKey::TypeQuery(sym_ref)) => SymbolRefKind::TypeQuery(sym_ref),
        _ => SymbolRefKind::Other,
    }
}

// =============================================================================
// Type Contains Classification (for type_contains_any_inner)
// =============================================================================

/// Classification for recursive type traversal.
#[derive(Debug, Clone)]
pub enum TypeContainsKind {
    Array(TypeId),
    Tuple(crate::solver::types::TupleListId),
    Members(Vec<TypeId>),
    Object(crate::solver::types::ObjectShapeId),
    Function(crate::solver::types::FunctionShapeId),
    Callable(crate::solver::types::CallableShapeId),
    Application(crate::solver::types::TypeApplicationId),
    Conditional(crate::solver::types::ConditionalTypeId),
    Mapped(crate::solver::types::MappedTypeId),
    IndexAccess {
        base: TypeId,
        index: TypeId,
    },
    TemplateLiteral(crate::solver::types::TemplateLiteralId),
    Inner(TypeId),
    TypeParam {
        constraint: Option<TypeId>,
        default: Option<TypeId>,
    },
    Terminal,
}

/// Classify a type for recursive traversal.
pub fn classify_for_contains_traversal(db: &dyn TypeDatabase, type_id: TypeId) -> TypeContainsKind {
    match db.lookup(type_id) {
        Some(TypeKey::Array(elem)) => TypeContainsKind::Array(elem),
        Some(TypeKey::Tuple(list_id)) => TypeContainsKind::Tuple(list_id),
        Some(TypeKey::Union(list_id)) | Some(TypeKey::Intersection(list_id)) => {
            TypeContainsKind::Members(db.type_list(list_id).to_vec())
        }
        Some(TypeKey::Object(shape_id)) | Some(TypeKey::ObjectWithIndex(shape_id)) => {
            TypeContainsKind::Object(shape_id)
        }
        Some(TypeKey::Function(shape_id)) => TypeContainsKind::Function(shape_id),
        Some(TypeKey::Callable(shape_id)) => TypeContainsKind::Callable(shape_id),
        Some(TypeKey::Application(app_id)) => TypeContainsKind::Application(app_id),
        Some(TypeKey::Conditional(cond_id)) => TypeContainsKind::Conditional(cond_id),
        Some(TypeKey::Mapped(mapped_id)) => TypeContainsKind::Mapped(mapped_id),
        Some(TypeKey::IndexAccess(base, index)) => TypeContainsKind::IndexAccess { base, index },
        Some(TypeKey::TemplateLiteral(template_id)) => {
            TypeContainsKind::TemplateLiteral(template_id)
        }
        Some(TypeKey::KeyOf(inner)) | Some(TypeKey::ReadonlyType(inner)) => {
            TypeContainsKind::Inner(inner)
        }
        Some(TypeKey::TypeParameter(info)) | Some(TypeKey::Infer(info)) => {
            TypeContainsKind::TypeParam {
                constraint: info.constraint,
                default: info.default,
            }
        }
        _ => TypeContainsKind::Terminal,
    }
}

// =============================================================================
// Namespace Member Classification (for resolve_namespace_value_member)
// =============================================================================

/// Classification for namespace member resolution.
#[derive(Debug, Clone)]
pub enum NamespaceMemberKind {
    SymbolRef(crate::solver::types::SymbolRef),
    Callable(crate::solver::types::CallableShapeId),
    Other,
}

/// Classify a type for namespace member resolution.
pub fn classify_namespace_member(db: &dyn TypeDatabase, type_id: TypeId) -> NamespaceMemberKind {
    match db.lookup(type_id) {
        Some(TypeKey::Ref(sym_ref)) => NamespaceMemberKind::SymbolRef(sym_ref),
        Some(TypeKey::Callable(shape_id)) => NamespaceMemberKind::Callable(shape_id),
        _ => NamespaceMemberKind::Other,
    }
}

/// Unwrap readonly type wrapper if present.
pub fn unwrap_readonly_for_lookup(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    match db.lookup(type_id) {
        Some(TypeKey::ReadonlyType(inner)) => inner,
        _ => type_id,
    }
}

// =============================================================================
// Literal Type Creation Helpers
// =============================================================================

/// Create a string literal type from a string value.
///
/// This abstracts away the TypeKey construction from the checker layer.
pub fn create_string_literal_type(db: &dyn TypeDatabase, value: &str) -> TypeId {
    let atom = db.intern_string(value);
    db.intern(TypeKey::Literal(crate::solver::LiteralValue::String(atom)))
}

/// Create a number literal type from a numeric value.
///
/// This abstracts away the TypeKey construction from the checker layer.
pub fn create_number_literal_type(db: &dyn TypeDatabase, value: f64) -> TypeId {
    db.intern(TypeKey::Literal(crate::solver::LiteralValue::Number(
        crate::solver::OrderedFloat(value),
    )))
}

/// Create a boolean literal type.
///
/// This abstracts away the TypeKey construction from the checker layer.
pub fn create_boolean_literal_type(db: &dyn TypeDatabase, value: bool) -> TypeId {
    db.intern(TypeKey::Literal(crate::solver::LiteralValue::Boolean(
        value,
    )))
}

// =============================================================================
// Instance Type from Constructor Classification
// =============================================================================

/// Classification for extracting instance types from constructor types.
#[derive(Debug, Clone)]
pub enum InstanceTypeKind {
    /// Callable type - extract from construct_signatures return types
    Callable(crate::solver::types::CallableShapeId),
    /// Function type - check is_constructor flag
    Function(crate::solver::types::FunctionShapeId),
    /// Intersection type - recursively extract instance types from members
    Intersection(Vec<TypeId>),
    /// Union type - recursively extract instance types from members
    Union(Vec<TypeId>),
    /// ReadonlyType - unwrap and recurse
    Readonly(TypeId),
    /// Type parameter with constraint - follow constraint
    TypeParameter { constraint: Option<TypeId> },
    /// Complex types (Conditional, Mapped, IndexAccess, KeyOf) - need evaluation
    NeedsEvaluation,
    /// Not a constructor type
    NotConstructor,
}

/// Classify a type for instance type extraction.
pub fn classify_for_instance_type(db: &dyn TypeDatabase, type_id: TypeId) -> InstanceTypeKind {
    let Some(key) = db.lookup(type_id) else {
        return InstanceTypeKind::NotConstructor;
    };

    match key {
        TypeKey::Callable(shape_id) => InstanceTypeKind::Callable(shape_id),
        TypeKey::Function(shape_id) => InstanceTypeKind::Function(shape_id),
        TypeKey::Intersection(list_id) => {
            let members = db.type_list(list_id);
            InstanceTypeKind::Intersection(members.to_vec())
        }
        TypeKey::Union(list_id) => {
            let members = db.type_list(list_id);
            InstanceTypeKind::Union(members.to_vec())
        }
        TypeKey::ReadonlyType(inner) => InstanceTypeKind::Readonly(inner),
        TypeKey::TypeParameter(info) | TypeKey::Infer(info) => InstanceTypeKind::TypeParameter {
            constraint: info.constraint,
        },
        TypeKey::Conditional(_)
        | TypeKey::Mapped(_)
        | TypeKey::IndexAccess(_, _)
        | TypeKey::KeyOf(_) => InstanceTypeKind::NeedsEvaluation,
        _ => InstanceTypeKind::NotConstructor,
    }
}

// =============================================================================
// Constructor Return Merge Classification
// =============================================================================

/// Classification for merging base instance into constructor return.
#[derive(Debug, Clone)]
pub enum ConstructorReturnMergeKind {
    /// Callable type - update construct_signatures
    Callable(crate::solver::types::CallableShapeId),
    /// Function type - check is_constructor flag
    Function(crate::solver::types::FunctionShapeId),
    /// Intersection type - update all members
    Intersection(Vec<TypeId>),
    /// Not mergeable
    Other,
}

/// Classify a type for constructor return merging.
pub fn classify_for_constructor_return_merge(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> ConstructorReturnMergeKind {
    let Some(key) = db.lookup(type_id) else {
        return ConstructorReturnMergeKind::Other;
    };

    match key {
        TypeKey::Callable(shape_id) => ConstructorReturnMergeKind::Callable(shape_id),
        TypeKey::Function(shape_id) => ConstructorReturnMergeKind::Function(shape_id),
        TypeKey::Intersection(list_id) => {
            let members = db.type_list(list_id);
            ConstructorReturnMergeKind::Intersection(members.to_vec())
        }
        _ => ConstructorReturnMergeKind::Other,
    }
}

// =============================================================================
// Abstract Constructor Type Classification
// =============================================================================

/// Classification for checking if a type is an abstract constructor type.
#[derive(Debug, Clone)]
pub enum AbstractConstructorKind {
    /// TypeQuery (typeof AbstractClass) - check if symbol is abstract
    TypeQuery(crate::solver::types::SymbolRef),
    /// Ref - resolve and check
    Ref(crate::solver::types::SymbolRef),
    /// Callable - check if marked as abstract
    Callable(crate::solver::types::CallableShapeId),
    /// Application - check base type
    Application(crate::solver::types::TypeApplicationId),
    /// Not an abstract constructor type
    NotAbstract,
}

/// Classify a type for abstract constructor checking.
pub fn classify_for_abstract_constructor(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> AbstractConstructorKind {
    let Some(key) = db.lookup(type_id) else {
        return AbstractConstructorKind::NotAbstract;
    };

    match key {
        TypeKey::TypeQuery(sym_ref) => AbstractConstructorKind::TypeQuery(sym_ref),
        TypeKey::Ref(sym_ref) => AbstractConstructorKind::Ref(sym_ref),
        TypeKey::Callable(shape_id) => AbstractConstructorKind::Callable(shape_id),
        TypeKey::Application(app_id) => AbstractConstructorKind::Application(app_id),
        _ => AbstractConstructorKind::NotAbstract,
    }
}

// =============================================================================
// Property Access Resolution Classification
// =============================================================================

/// Classification for resolving types for property access.
#[derive(Debug, Clone)]
pub enum PropertyAccessResolutionKind {
    /// Ref type - resolve the symbol
    Ref(crate::solver::types::SymbolRef),
    /// TypeQuery (typeof) - resolve the symbol
    TypeQuery(crate::solver::types::SymbolRef),
    /// Application - needs evaluation
    Application(crate::solver::types::TypeApplicationId),
    /// Type parameter - follow constraint
    TypeParameter { constraint: Option<TypeId> },
    /// Complex types that need evaluation
    NeedsEvaluation,
    /// Union - resolve each member
    Union(Vec<TypeId>),
    /// Intersection - resolve each member
    Intersection(Vec<TypeId>),
    /// Readonly wrapper - unwrap
    Readonly(TypeId),
    /// Function or Callable - may need Function interface
    FunctionLike,
    /// Already resolved
    Resolved,
}

/// Classify a type for property access resolution.
pub fn classify_for_property_access_resolution(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> PropertyAccessResolutionKind {
    let Some(key) = db.lookup(type_id) else {
        return PropertyAccessResolutionKind::Resolved;
    };

    match key {
        TypeKey::Ref(sym_ref) => PropertyAccessResolutionKind::Ref(sym_ref),
        TypeKey::TypeQuery(sym_ref) => PropertyAccessResolutionKind::TypeQuery(sym_ref),
        TypeKey::Application(app_id) => PropertyAccessResolutionKind::Application(app_id),
        TypeKey::TypeParameter(info) | TypeKey::Infer(info) => {
            PropertyAccessResolutionKind::TypeParameter {
                constraint: info.constraint,
            }
        }
        TypeKey::Conditional(_)
        | TypeKey::Mapped(_)
        | TypeKey::IndexAccess(_, _)
        | TypeKey::KeyOf(_) => PropertyAccessResolutionKind::NeedsEvaluation,
        TypeKey::Union(list_id) => {
            let members = db.type_list(list_id);
            PropertyAccessResolutionKind::Union(members.to_vec())
        }
        TypeKey::Intersection(list_id) => {
            let members = db.type_list(list_id);
            PropertyAccessResolutionKind::Intersection(members.to_vec())
        }
        TypeKey::ReadonlyType(inner) => PropertyAccessResolutionKind::Readonly(inner),
        TypeKey::Function(_) | TypeKey::Callable(_) => PropertyAccessResolutionKind::FunctionLike,
        _ => PropertyAccessResolutionKind::Resolved,
    }
}

// =============================================================================
// Contextual Type Literal Allow Classification
// =============================================================================

/// Classification for checking if contextual type allows literals.
#[derive(Debug, Clone)]
pub enum ContextualLiteralAllowKind {
    /// Union or Intersection - check all members
    Members(Vec<TypeId>),
    /// Type parameter - check constraint
    TypeParameter { constraint: Option<TypeId> },
    /// Ref - resolve and check
    Ref(crate::solver::types::SymbolRef),
    /// Application - needs evaluation
    Application,
    /// Mapped type - needs evaluation
    Mapped,
    /// Does not allow literal
    NotAllowed,
}

/// Classify a type for contextual literal checking.
pub fn classify_for_contextual_literal(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> ContextualLiteralAllowKind {
    let Some(key) = db.lookup(type_id) else {
        return ContextualLiteralAllowKind::NotAllowed;
    };

    match key {
        TypeKey::Union(list_id) | TypeKey::Intersection(list_id) => {
            let members = db.type_list(list_id);
            ContextualLiteralAllowKind::Members(members.to_vec())
        }
        TypeKey::TypeParameter(info) | TypeKey::Infer(info) => {
            ContextualLiteralAllowKind::TypeParameter {
                constraint: info.constraint,
            }
        }
        TypeKey::Ref(sym_ref) => ContextualLiteralAllowKind::Ref(sym_ref),
        TypeKey::Application(_) => ContextualLiteralAllowKind::Application,
        TypeKey::Mapped(_) => ContextualLiteralAllowKind::Mapped,
        _ => ContextualLiteralAllowKind::NotAllowed,
    }
}

// =============================================================================
// Mapped Constraint Resolution Classification
// =============================================================================

/// Classification for evaluating mapped type constraints.
#[derive(Debug, Clone)]
pub enum MappedConstraintKind {
    /// KeyOf type - evaluate operand
    KeyOf(TypeId),
    /// Union or Literal - return as-is
    Resolved,
    /// Other type - return as-is
    Other,
}

/// Classify a constraint type for mapped type evaluation.
pub fn classify_mapped_constraint(db: &dyn TypeDatabase, type_id: TypeId) -> MappedConstraintKind {
    let Some(key) = db.lookup(type_id) else {
        return MappedConstraintKind::Other;
    };

    match key {
        TypeKey::KeyOf(operand) => MappedConstraintKind::KeyOf(operand),
        TypeKey::Union(_) | TypeKey::Literal(_) => MappedConstraintKind::Resolved,
        _ => MappedConstraintKind::Other,
    }
}

// =============================================================================
// Type Resolution Classification
// =============================================================================

/// Classification for evaluating types with symbol resolution.
#[derive(Debug, Clone)]
pub enum TypeResolutionKind {
    /// Ref - resolve to symbol type
    Ref(crate::solver::types::SymbolRef),
    /// Application - evaluate the application
    Application,
    /// Already resolved
    Resolved,
}

/// Classify a type for resolution.
pub fn classify_for_type_resolution(db: &dyn TypeDatabase, type_id: TypeId) -> TypeResolutionKind {
    let Some(key) = db.lookup(type_id) else {
        return TypeResolutionKind::Resolved;
    };

    match key {
        TypeKey::Ref(sym_ref) => TypeResolutionKind::Ref(sym_ref),
        TypeKey::Application(_) => TypeResolutionKind::Application,
        _ => TypeResolutionKind::Resolved,
    }
}

// =============================================================================
// Type Argument Extraction Classification
// =============================================================================

/// Classification for extracting type parameters from a type for instantiation.
#[derive(Debug, Clone)]
pub enum TypeArgumentExtractionKind {
    /// Function type with type params
    Function(crate::solver::types::FunctionShapeId),
    /// Callable type with signatures potentially having type params
    Callable(crate::solver::types::CallableShapeId),
    /// Not applicable
    Other,
}

/// Classify a type for type argument extraction.
pub fn classify_for_type_argument_extraction(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> TypeArgumentExtractionKind {
    let Some(key) = db.lookup(type_id) else {
        return TypeArgumentExtractionKind::Other;
    };

    match key {
        TypeKey::Function(shape_id) => TypeArgumentExtractionKind::Function(shape_id),
        TypeKey::Callable(shape_id) => TypeArgumentExtractionKind::Callable(shape_id),
        _ => TypeArgumentExtractionKind::Other,
    }
}

// =============================================================================
// Base Instance Properties Merge Classification
// =============================================================================

/// Classification for merging base instance properties.
#[derive(Debug, Clone)]
pub enum BaseInstanceMergeKind {
    /// Object type with shape
    Object(crate::solver::types::ObjectShapeId),
    /// Intersection - merge all members
    Intersection(Vec<TypeId>),
    /// Union - find common properties
    Union(Vec<TypeId>),
    /// Not mergeable
    Other,
}

/// Classify a type for base instance property merging.
pub fn classify_for_base_instance_merge(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> BaseInstanceMergeKind {
    let Some(key) = db.lookup(type_id) else {
        return BaseInstanceMergeKind::Other;
    };

    match key {
        TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id) => {
            BaseInstanceMergeKind::Object(shape_id)
        }
        TypeKey::Intersection(list_id) => {
            let members = db.type_list(list_id);
            BaseInstanceMergeKind::Intersection(members.to_vec())
        }
        TypeKey::Union(list_id) => {
            let members = db.type_list(list_id);
            BaseInstanceMergeKind::Union(members.to_vec())
        }
        _ => BaseInstanceMergeKind::Other,
    }
}

// =============================================================================
// Excess Properties Classification
// =============================================================================

/// Classification for checking excess properties.
#[derive(Debug, Clone)]
pub enum ExcessPropertiesKind {
    /// Object type (without index signature) - check for excess
    Object(crate::solver::types::ObjectShapeId),
    /// Object with index signature - accepts any property
    ObjectWithIndex(crate::solver::types::ObjectShapeId),
    /// Union - check all members
    Union(Vec<TypeId>),
    /// Not an object type
    NotObject,
}

/// Classify a type for excess property checking.
pub fn classify_for_excess_properties(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> ExcessPropertiesKind {
    let Some(key) = db.lookup(type_id) else {
        return ExcessPropertiesKind::NotObject;
    };

    match key {
        TypeKey::Object(shape_id) => ExcessPropertiesKind::Object(shape_id),
        TypeKey::ObjectWithIndex(shape_id) => ExcessPropertiesKind::ObjectWithIndex(shape_id),
        TypeKey::Union(list_id) => {
            let members = db.type_list(list_id);
            ExcessPropertiesKind::Union(members.to_vec())
        }
        _ => ExcessPropertiesKind::NotObject,
    }
}

// =============================================================================
// Constructor Access Level Classification
// =============================================================================

/// Classification for checking constructor access level.
#[derive(Debug, Clone)]
pub enum ConstructorAccessKind {
    /// Ref or TypeQuery - resolve symbol
    SymbolRef(crate::solver::types::SymbolRef),
    /// Application - check base
    Application(crate::solver::types::TypeApplicationId),
    /// Not applicable
    Other,
}

/// Classify a type for constructor access level checking.
pub fn classify_for_constructor_access(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> ConstructorAccessKind {
    let Some(key) = db.lookup(type_id) else {
        return ConstructorAccessKind::Other;
    };

    match key {
        TypeKey::Ref(sym_ref) | TypeKey::TypeQuery(sym_ref) => {
            ConstructorAccessKind::SymbolRef(sym_ref)
        }
        TypeKey::Application(app_id) => ConstructorAccessKind::Application(app_id),
        _ => ConstructorAccessKind::Other,
    }
}

// =============================================================================
// Assignability Evaluation Classification
// =============================================================================

/// Classification for types that need evaluation before assignability.
#[derive(Debug, Clone)]
pub enum AssignabilityEvalKind {
    /// Application - evaluate with resolution
    Application,
    /// Index/KeyOf/Mapped/Conditional - evaluate with env
    NeedsEnvEval,
    /// Already resolved
    Resolved,
}

/// Classify a type for assignability evaluation.
pub fn classify_for_assignability_eval(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> AssignabilityEvalKind {
    let Some(key) = db.lookup(type_id) else {
        return AssignabilityEvalKind::Resolved;
    };

    match key {
        TypeKey::Application(_) => AssignabilityEvalKind::Application,
        TypeKey::IndexAccess(_, _)
        | TypeKey::KeyOf(_)
        | TypeKey::Mapped(_)
        | TypeKey::Conditional(_) => AssignabilityEvalKind::NeedsEnvEval,
        _ => AssignabilityEvalKind::Resolved,
    }
}

// =============================================================================
// Binding Element Type Classification
// =============================================================================

/// Classification for binding element (destructuring) type extraction.
#[derive(Debug, Clone)]
pub enum BindingElementTypeKind {
    /// Array type - use element type
    Array(TypeId),
    /// Tuple type - use element by index
    Tuple(crate::solver::types::TupleListId),
    /// Object type - use property type
    Object(crate::solver::types::ObjectShapeId),
    /// Not applicable
    Other,
}

/// Classify a type for binding element type extraction.
pub fn classify_for_binding_element(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> BindingElementTypeKind {
    let Some(key) = db.lookup(type_id) else {
        return BindingElementTypeKind::Other;
    };

    match key {
        TypeKey::Array(elem) => BindingElementTypeKind::Array(elem),
        TypeKey::Tuple(list_id) => BindingElementTypeKind::Tuple(list_id),
        TypeKey::Object(shape_id) => BindingElementTypeKind::Object(shape_id),
        _ => BindingElementTypeKind::Other,
    }
}

// =============================================================================
// Additional Accessor Helpers
// =============================================================================

/// Get the symbol ref from a Ref type.
pub fn get_symbol_ref(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<crate::solver::types::SymbolRef> {
    match db.lookup(type_id) {
        Some(TypeKey::Ref(sym_ref)) => Some(sym_ref),
        _ => None,
    }
}

/// Get the mapped type ID if the type is a Mapped type.
pub fn get_mapped_type_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<crate::solver::types::MappedTypeId> {
    match db.lookup(type_id) {
        Some(TypeKey::Mapped(mapped_id)) => Some(mapped_id),
        _ => None,
    }
}

/// Get the conditional type ID if the type is a Conditional type.
pub fn get_conditional_type_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<crate::solver::types::ConditionalTypeId> {
    match db.lookup(type_id) {
        Some(TypeKey::Conditional(cond_id)) => Some(cond_id),
        _ => None,
    }
}

/// Get the keyof inner type if the type is a KeyOf type.
pub fn get_keyof_inner(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    match db.lookup(type_id) {
        Some(TypeKey::KeyOf(inner)) => Some(inner),
        _ => None,
    }
}

// =============================================================================
// Symbol Resolution Traversal Classification
// =============================================================================

/// Classification for traversing types to resolve symbols.
/// Used by ensure_application_symbols_resolved_inner.
#[derive(Debug, Clone)]
pub enum SymbolResolutionTraversalKind {
    /// Application type - resolve base symbol and recurse
    Application {
        app_id: crate::solver::types::TypeApplicationId,
        base: TypeId,
        args: Vec<TypeId>,
    },
    /// Ref type - resolve the symbol
    Ref(crate::solver::types::SymbolRef),
    /// Type parameter - recurse into constraint/default
    TypeParameter {
        constraint: Option<TypeId>,
        default: Option<TypeId>,
    },
    /// Union or Intersection - recurse into members
    Members(Vec<TypeId>),
    /// Function type - recurse into signature components
    Function(crate::solver::types::FunctionShapeId),
    /// Callable type - recurse into signatures
    Callable(crate::solver::types::CallableShapeId),
    /// Object type - recurse into properties and index signatures
    Object(crate::solver::types::ObjectShapeId),
    /// Array type - recurse into element
    Array(TypeId),
    /// Tuple type - recurse into elements
    Tuple(crate::solver::types::TupleListId),
    /// Conditional type - recurse into all branches
    Conditional(crate::solver::types::ConditionalTypeId),
    /// Mapped type - recurse into constraint, template, name_type
    Mapped(crate::solver::types::MappedTypeId),
    /// Readonly wrapper - recurse into inner
    Readonly(TypeId),
    /// Index access - recurse into both types
    IndexAccess { object: TypeId, index: TypeId },
    /// KeyOf - recurse into inner
    KeyOf(TypeId),
    /// Terminal type - no further traversal needed
    Terminal,
}

/// Classify a type for symbol resolution traversal.
pub fn classify_for_symbol_resolution_traversal(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> SymbolResolutionTraversalKind {
    let Some(key) = db.lookup(type_id) else {
        return SymbolResolutionTraversalKind::Terminal;
    };

    match key {
        TypeKey::Application(app_id) => {
            let app = db.type_application(app_id);
            SymbolResolutionTraversalKind::Application {
                app_id,
                base: app.base,
                args: app.args.clone(),
            }
        }
        TypeKey::Ref(sym_ref) => SymbolResolutionTraversalKind::Ref(sym_ref),
        TypeKey::TypeParameter(param) | TypeKey::Infer(param) => {
            SymbolResolutionTraversalKind::TypeParameter {
                constraint: param.constraint,
                default: param.default,
            }
        }
        TypeKey::Union(members_id) | TypeKey::Intersection(members_id) => {
            let members = db.type_list(members_id);
            SymbolResolutionTraversalKind::Members(members.to_vec())
        }
        TypeKey::Function(shape_id) => SymbolResolutionTraversalKind::Function(shape_id),
        TypeKey::Callable(shape_id) => SymbolResolutionTraversalKind::Callable(shape_id),
        TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id) => {
            SymbolResolutionTraversalKind::Object(shape_id)
        }
        TypeKey::Array(elem) => SymbolResolutionTraversalKind::Array(elem),
        TypeKey::Tuple(elems_id) => SymbolResolutionTraversalKind::Tuple(elems_id),
        TypeKey::Conditional(cond_id) => SymbolResolutionTraversalKind::Conditional(cond_id),
        TypeKey::Mapped(mapped_id) => SymbolResolutionTraversalKind::Mapped(mapped_id),
        TypeKey::ReadonlyType(inner) => SymbolResolutionTraversalKind::Readonly(inner),
        TypeKey::IndexAccess(obj, idx) => SymbolResolutionTraversalKind::IndexAccess {
            object: obj,
            index: idx,
        },
        TypeKey::KeyOf(inner) => SymbolResolutionTraversalKind::KeyOf(inner),
        _ => SymbolResolutionTraversalKind::Terminal,
    }
}
