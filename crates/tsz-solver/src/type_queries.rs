//! Type Query Functions
//!
//! This module provides high-level query functions for inspecting type characteristics.
//! These functions abstract away the internal `TypeData` representation and provide
//! a stable API for the checker to query type properties.
//!
//! # Design Principles
//!
//! - **Abstraction**: Checker code should use these functions instead of matching on `TypeData`
//! - **TypeDatabase-based**: All queries work through the `TypeDatabase` trait
//! - **Comprehensive**: Covers all common type checking scenarios
//! - **Efficient**: Simple lookups with minimal overhead
//!
//! # Usage
//!
//! ```rust,ignore
//! use crate::type_queries::*;
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

use crate::{TypeData, TypeDatabase, TypeId};
use tsz_common::Atom;

// Re-export extended type queries so callers can use `type_queries::*`
pub use crate::type_queries_classifiers::{
    AssignabilityEvalKind, AugmentationTargetKind, BindingElementTypeKind, ConstructorAccessKind,
    ExcessPropertiesKind, InterfaceMergeKind, SymbolResolutionTraversalKind,
    classify_for_assignability_eval, classify_for_augmentation, classify_for_binding_element,
    classify_for_constructor_access, classify_for_excess_properties, classify_for_interface_merge,
    classify_for_symbol_resolution_traversal, get_conditional_type_id, get_def_id,
    get_enum_components, get_keyof_inner, get_lazy_def_id, get_mapped_type_id, get_type_identity,
};
pub use crate::type_queries_extended::get_application_info;
pub use crate::type_queries_extended::{
    AbstractClassCheckKind, AbstractConstructorKind, ArrayLikeKind, BaseInstanceMergeKind,
    CallSignaturesKind, ClassDeclTypeKind, ConstructorCheckKind, ConstructorReturnMergeKind,
    ContextualLiteralAllowKind, ElementIndexableKind, IndexKeyKind, InstanceTypeKind,
    KeyOfTypeKind, LazyTypeKind, LiteralKeyKind, LiteralTypeKind, MappedConstraintKind,
    NamespaceMemberKind, PrivateBrandKind, PromiseTypeKind, PropertyAccessResolutionKind,
    StringLiteralKeyKind, TypeArgumentExtractionKind, TypeParameterKind, TypeQueryKind,
    TypeResolutionKind, classify_array_like, classify_element_indexable,
    classify_for_abstract_check, classify_for_base_instance_merge, classify_for_call_signatures,
    classify_for_class_decl, classify_for_constructor_check, classify_for_constructor_return_merge,
    classify_for_contextual_literal, classify_for_instance_type, classify_for_lazy_resolution,
    classify_for_private_brand, classify_for_property_access_resolution,
    classify_for_string_literal_keys, classify_for_type_argument_extraction,
    classify_for_type_resolution, classify_index_key, classify_literal_key, classify_literal_type,
    classify_mapped_constraint, classify_namespace_member, classify_promise_type,
    classify_type_parameter, classify_type_query, create_boolean_literal_type,
    create_number_literal_type, create_string_literal_type, get_application_base,
    get_boolean_literal_value, get_callable_type_param_count, get_number_literal_value,
    get_string_literal_atom, get_string_literal_value, get_tuple_list_id, get_type_param_default,
    get_widened_literal_type, is_boolean_literal, is_direct_type_parameter, is_invalid_index_type,
    is_number_literal, is_object_with_index_type, is_string_literal, unwrap_readonly_for_lookup,
    widen_literal_to_primitive,
};

pub use crate::type_queries_data::*;
pub use crate::type_queries_flow::*;

pub fn get_keyof_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    get_keyof_inner(db, type_id)
}

pub fn get_allowed_keys(db: &dyn TypeDatabase, type_id: TypeId) -> rustc_hash::FxHashSet<String> {
    let atoms = collect_property_name_atoms_for_diagnostics(db, type_id, 10);
    atoms.into_iter().map(|a| db.resolve_atom(a)).collect()
}

// =============================================================================
// Core Type Queries
// =============================================================================

/// Check if a type is a callable type (function or callable with signatures).
///
/// Returns true for `TypeData::Callable` and `TypeData::Function` types.
pub fn is_callable_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        db.lookup(type_id),
        Some(TypeData::Callable(_) | TypeData::Function(_))
    )
}

/// Check if a type is invokable (has call signatures, not just construct signatures).
///
/// This is more specific than `is_callable_type` - it ensures the type can be called
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
        Some(TypeData::Function(_)) => true,
        Some(TypeData::Callable(shape_id)) => {
            let shape = db.callable_shape(shape_id);
            // Must have at least one call signature (not just construct signatures)
            !shape.call_signatures.is_empty()
        }
        // Intersections might contain a callable
        Some(TypeData::Intersection(list_id)) => {
            let members = db.type_list(list_id);
            members.iter().any(|&m| is_invokable_type(db, m))
        }
        _ => false,
    }
}

/// Check if a type is a tuple type.
///
/// Returns true for `TypeData::Tuple`.
pub fn is_tuple_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(db.lookup(type_id), Some(TypeData::Tuple(_)))
}

/// Check if a type is a union type (A | B).
///
/// Returns true for `TypeData::Union`.
pub fn is_union_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(db.lookup(type_id), Some(TypeData::Union(_)))
}

/// Check if a type is an intersection type (A & B).
///
/// Returns true for `TypeData::Intersection`.
pub fn is_intersection_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(db.lookup(type_id), Some(TypeData::Intersection(_)))
}

/// Check if a type is an object type (with or without index signatures).
///
/// Returns true for `TypeData::Object` and `TypeData::ObjectWithIndex`.
pub fn is_object_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        db.lookup(type_id),
        Some(TypeData::Object(_) | TypeData::ObjectWithIndex(_))
    )
}

/// Check if a type is an array type (T[]).
///
/// Returns true for `TypeData::Array`.
pub fn is_array_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(db.lookup(type_id), Some(TypeData::Array(_)))
}

/// Check if a type is a literal type (specific value).
///
/// Returns true for `TypeData::Literal`.
pub fn is_literal_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(db.lookup(type_id), Some(TypeData::Literal(_)))
}

/// Check if a type is a generic type application (Base<Args>).
///
/// Returns true for `TypeData::Application`.
pub fn is_generic_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(db.lookup(type_id), Some(TypeData::Application(_)))
}

/// Check if a type is a named type reference.
///
/// Returns true for `TypeData::Lazy(DefId)` (interfaces, classes, type aliases).
pub fn is_type_reference(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        db.lookup(type_id),
        Some(TypeData::Lazy(_) | TypeData::Recursive(_) | TypeData::BoundParameter(_))
    )
}

/// Check if a type is a conditional type (T extends U ? X : Y).
///
/// Returns true for `TypeData::Conditional`.
pub fn is_conditional_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(db.lookup(type_id), Some(TypeData::Conditional(_)))
}

/// Check if a type is a mapped type ({ [K in Keys]: V }).
///
/// Returns true for `TypeData::Mapped`.
pub fn is_mapped_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(db.lookup(type_id), Some(TypeData::Mapped(_)))
}

/// Check if a type is a template literal type (`hello${T}world`).
///
/// Returns true for `TypeData::TemplateLiteral`.
pub fn is_template_literal_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(db.lookup(type_id), Some(TypeData::TemplateLiteral(_)))
}

/// Check if a type is a type parameter, bound parameter, or infer type.
///
/// Returns true for `TypeData::TypeParameter`, `TypeData::BoundParameter`,
/// and `TypeData::Infer`. `BoundParameter` is included because it represents
/// a type parameter that has been bound to a specific index in a generic
/// signature — it should still be treated as "unresolved" for purposes like
/// excess property checking and constraint validation.
pub fn is_type_parameter(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        db.lookup(type_id),
        Some(TypeData::TypeParameter(_) | TypeData::BoundParameter(_) | TypeData::Infer(_))
    )
}

/// Check if a type is an index access type (T[K]).
///
/// Returns true for `TypeData::IndexAccess`.
pub fn is_index_access_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(db.lookup(type_id), Some(TypeData::IndexAccess(_, _)))
}

/// Check if a type is a keyof type.
///
/// Returns true for `TypeData::KeyOf`.
pub fn is_keyof_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(db.lookup(type_id), Some(TypeData::KeyOf(_)))
}

/// Check if a type is a type query (typeof expr).
///
/// Returns true for `TypeData::TypeQuery`.
pub fn is_type_query(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(db.lookup(type_id), Some(TypeData::TypeQuery(_)))
}

/// Check if a type is a readonly type modifier.
///
/// Returns true for `TypeData::ReadonlyType`.
pub fn is_readonly_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(db.lookup(type_id), Some(TypeData::ReadonlyType(_)))
}

/// Check if a type is a unique symbol type.
///
/// Returns true for `TypeData::UniqueSymbol`.
pub fn is_unique_symbol_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(db.lookup(type_id), Some(TypeData::UniqueSymbol(_)))
}

/// Check if a type is usable as a property name (TS1166/TS1165/TS1169).
///
/// Returns true for string literals, number literals, and unique symbol types.
/// This corresponds to TypeScript's `isTypeUsableAsPropertyName` check.
pub fn is_type_usable_as_property_name(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        db.lookup(type_id),
        Some(
            TypeData::Literal(crate::LiteralValue::String(_))
                | TypeData::Literal(crate::LiteralValue::Number(_))
                | TypeData::UniqueSymbol(_)
        )
    )
}

/// Check if a type is the this type.
///
/// Returns true for `TypeData::ThisType`.
pub fn is_this_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(db.lookup(type_id), Some(TypeData::ThisType))
}

/// Check if a type is an error type.
///
/// Returns true for `TypeData::Error` or `TypeId::ERROR`.
pub fn is_error_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    type_id == TypeId::ERROR || matches!(db.lookup(type_id), Some(TypeData::Error))
}

/// Check if a type needs evaluation before interface merging.
///
/// Returns true for Application and Lazy types, which are meta-types that
/// may resolve to Object/Callable types when evaluated. Used before
/// `classify_for_interface_merge` to ensure that type-alias-based heritage
/// (e.g., `interface X extends TypeAlias<T>`) is properly resolved.
pub fn needs_evaluation_for_merge(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        db.lookup(type_id),
        Some(TypeData::Application(_) | TypeData::Lazy(_))
    )
}

/// Get the return type of a function type.
///
/// Returns `TypeId::ERROR` if the type is not a Function.
pub fn get_function_return_type(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    match db.lookup(type_id) {
        Some(TypeData::Function(shape_id)) => db.function_shape(shape_id).return_type,
        _ => TypeId::ERROR,
    }
}

/// Get the parameter types of a function type.
///
/// Returns an empty vector if the type is not a Function.
pub fn get_function_parameter_types(db: &dyn TypeDatabase, type_id: TypeId) -> Vec<TypeId> {
    match db.lookup(type_id) {
        Some(TypeData::Function(shape_id)) => db
            .function_shape(shape_id)
            .params
            .iter()
            .map(|p| p.type_id)
            .collect(),
        _ => Vec::new(),
    }
}

/// Check if a type is an intrinsic type (any, unknown, never, void, etc.).
///
/// Returns true for `TypeData::Intrinsic`.
pub fn is_intrinsic_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(db.lookup(type_id), Some(TypeData::Intrinsic(_)))
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
        Some(TypeData::Intrinsic(_) | TypeData::Literal(_))
    )
}

// =============================================================================
// Intrinsic Type Queries
// =============================================================================
//
// These functions provide TypeData-free checking for intrinsic types.
// Checker code should use these instead of matching on TypeData::Intrinsic.
//
// ## Important Usage Notes
//
// These are TYPE IDENTITY checks, NOT compatibility checks:
//
// - Identity: `is_string_type(TypeId::STRING)` -> TRUE
// - Identity: `is_string_type(literal "hello")` -> FALSE (literal, not intrinsic)
// - Identity: `is_string_type(string & {tag: 1})` -> FALSE (intersection, not intrinsic)
//
// For assignability/compatibility checks, use Solver subtyping:
// - `solver.is_subtype_of(literal, TypeId::STRING)` -> TRUE
// - `solver.is_subtype_of(branded, TypeId::STRING)` -> TRUE (if assignable)
//
// ### When to use these helpers
// - Checking if a type annotation is explicitly the intrinsic keyword
// - Validating type constructor arguments
// - Distinguishing `void` from `undefined` in return types
//
// ### When NOT to use these helpers
// - Assignment/compatibility checks -> Use `is_subtype_of` instead
// - Type narrowing -> Use Solver's narrowing analysis
// - Checking if a value IS a string (not literal) -> Use `is_subtype_of`
//
// ## Implementation Notes
// - Shallow queries: do NOT resolve Lazy/Ref (caller's responsibility)
// - Defensive pattern: check both TypeId constants AND TypeData::Intrinsic
// - Fast-path O(1) using TypeId integer comparison

use crate::types::IntrinsicKind;

/// Check if a type is the `any` type.
///
/// Returns true for `TypeId::ANY` or `TypeData::Intrinsic(IntrinsicKind::Any)`.
pub fn is_any_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    type_id == TypeId::ANY
        || matches!(
            db.lookup(type_id),
            Some(TypeData::Intrinsic(IntrinsicKind::Any))
        )
}

/// Check if a type is the `unknown` type.
///
/// Returns true for `TypeId::UNKNOWN` or `TypeData::Intrinsic(IntrinsicKind::Unknown)`.
pub fn is_unknown_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    type_id == TypeId::UNKNOWN
        || matches!(
            db.lookup(type_id),
            Some(TypeData::Intrinsic(IntrinsicKind::Unknown))
        )
}

/// Check if a type is the `never` type.
///
/// Returns true for `TypeId::NEVER` or `TypeData::Intrinsic(IntrinsicKind::Never)`.
pub fn is_never_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    type_id == TypeId::NEVER
        || matches!(
            db.lookup(type_id),
            Some(TypeData::Intrinsic(IntrinsicKind::Never))
        )
}

/// Check if a type is the `void` type.
///
/// Returns true for `TypeId::VOID` or `TypeData::Intrinsic(IntrinsicKind::Void)`.
pub fn is_void_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    type_id == TypeId::VOID
        || matches!(
            db.lookup(type_id),
            Some(TypeData::Intrinsic(IntrinsicKind::Void))
        )
}

/// Check if a type is the `undefined` type.
///
/// Returns true for `TypeId::UNDEFINED` or `TypeData::Intrinsic(IntrinsicKind::Undefined)`.
pub fn is_undefined_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    type_id == TypeId::UNDEFINED
        || matches!(
            db.lookup(type_id),
            Some(TypeData::Intrinsic(IntrinsicKind::Undefined))
        )
}

/// Check if a type is the `null` type.
///
/// Returns true for `TypeId::NULL` or `TypeData::Intrinsic(IntrinsicKind::Null)`.
pub fn is_null_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    type_id == TypeId::NULL
        || matches!(
            db.lookup(type_id),
            Some(TypeData::Intrinsic(IntrinsicKind::Null))
        )
}

/// Check if a type is the `string` type.
///
/// Returns true for `TypeId::STRING` or `TypeData::Intrinsic(IntrinsicKind::String)`.
pub fn is_string_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    type_id == TypeId::STRING
        || matches!(
            db.lookup(type_id),
            Some(TypeData::Intrinsic(IntrinsicKind::String))
        )
}

/// Check if a type is the `number` type.
///
/// Returns true for `TypeId::NUMBER` or `TypeData::Intrinsic(IntrinsicKind::Number)`.
pub fn is_number_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    type_id == TypeId::NUMBER
        || matches!(
            db.lookup(type_id),
            Some(TypeData::Intrinsic(IntrinsicKind::Number))
        )
}

/// Check if a type is the `bigint` type.
///
/// Returns true for `TypeId::BIGINT` or `TypeData::Intrinsic(IntrinsicKind::Bigint)`.
pub fn is_bigint_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    type_id == TypeId::BIGINT
        || matches!(
            db.lookup(type_id),
            Some(TypeData::Intrinsic(IntrinsicKind::Bigint))
        )
}

/// Check if a type is the `boolean` type.
///
/// Returns true for `TypeId::BOOLEAN` or `TypeData::Intrinsic(IntrinsicKind::Boolean)`.
/// Note: This does NOT include boolean literals (true/false). For literal checks,
/// use `is_literal_type` combined with value inspection.
pub fn is_boolean_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    type_id == TypeId::BOOLEAN
        || matches!(
            db.lookup(type_id),
            Some(TypeData::Intrinsic(IntrinsicKind::Boolean))
        )
}

/// Check if a type is the `symbol` type.
///
/// Returns true for `TypeId::SYMBOL` or `TypeData::Intrinsic(IntrinsicKind::Symbol)`.
pub fn is_symbol_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    type_id == TypeId::SYMBOL
        || matches!(
            db.lookup(type_id),
            Some(TypeData::Intrinsic(IntrinsicKind::Symbol))
        )
}

// =============================================================================
// Composite Type Queries
// =============================================================================

/// Check if a type is an object-like type suitable for typeof "object".
///
/// Returns true for: Object, `ObjectWithIndex`, Array, Tuple, Mapped
pub fn is_object_like_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    is_object_like_type_impl(db, type_id)
}

fn is_object_like_type_impl(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    match db.lookup(type_id) {
        Some(
            TypeData::Object(_)
            | TypeData::ObjectWithIndex(_)
            | TypeData::Array(_)
            | TypeData::Tuple(_)
            | TypeData::Mapped(_)
            | TypeData::Function(_)
            | TypeData::Callable(_)
            | TypeData::Intrinsic(IntrinsicKind::Object | IntrinsicKind::Function),
        ) => true,
        Some(TypeData::ReadonlyType(inner)) => is_object_like_type_impl(db, inner),
        Some(TypeData::Intersection(members)) => {
            let members = db.type_list(members);
            members
                .iter()
                .all(|&member| is_object_like_type_impl(db, member))
        }
        Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => info
            .constraint
            .is_some_and(|constraint| is_object_like_type_impl(db, constraint)),
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
        Some(TypeData::Function(_) | TypeData::Callable(_)) => true,
        Some(TypeData::Intersection(members)) => {
            let members = db.type_list(members);
            members
                .iter()
                .any(|&member| is_function_type_impl(db, member))
        }
        _ => false,
    }
}

/// Check if a type is valid for object spreading (`{...x}`).
///
/// Returns `true` for types that can be spread into an object literal:
/// - `any`, `never`, `error` (always spreadable)
/// - Object types, arrays, tuples, functions, callables, mapped types
/// - `object` intrinsic (non-primitive)
/// - Type parameters (spreadable by default; constraint checked separately)
/// - Unions: all members must be spreadable
/// - Intersections: all members must be spreadable
///
/// Returns `false` for primitive types (`number`, `string`, `boolean`, etc.),
/// literals, `null`, `undefined`, `void`, and `unknown`.
pub fn is_valid_spread_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    is_valid_spread_type_impl(db, type_id, 0)
}

fn is_valid_spread_type_impl(db: &dyn TypeDatabase, type_id: TypeId, depth: u32) -> bool {
    if depth > 20 {
        return true;
    }
    match type_id {
        TypeId::ANY | TypeId::NEVER | TypeId::ERROR => return true,
        _ => {}
    }
    match db.lookup(type_id) {
        // Primitives and literals are not spreadable
        Some(
            TypeData::Intrinsic(
                IntrinsicKind::String
                | IntrinsicKind::Number
                | IntrinsicKind::Boolean
                | IntrinsicKind::Bigint
                | IntrinsicKind::Symbol
                | IntrinsicKind::Void
                | IntrinsicKind::Null
                | IntrinsicKind::Undefined
                | IntrinsicKind::Unknown,
            )
            | TypeData::Literal(_),
        ) => false,
        // Union: all members must be spreadable
        Some(TypeData::Union(members)) => {
            let members = db.type_list(members);
            members
                .iter()
                .all(|&m| is_valid_spread_type_impl(db, m, depth + 1))
        }
        // Intersection: all members must be spreadable
        Some(TypeData::Intersection(members)) => {
            let members = db.type_list(members);
            members
                .iter()
                .all(|&m| is_valid_spread_type_impl(db, m, depth + 1))
        }
        Some(TypeData::ReadonlyType(inner)) => is_valid_spread_type_impl(db, inner, depth + 1),
        // Everything else is spreadable: object types, arrays, tuples, functions,
        // callables, mapped types, type parameters, lazy refs, applications, etc.
        _ => true,
    }
}

/// Check if a type is an empty object type (no properties, no index signatures).
pub fn is_empty_object_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    match db.lookup(type_id) {
        Some(TypeData::Object(shape_id)) => {
            let shape = db.object_shape(shape_id);
            shape.properties.is_empty()
        }
        Some(TypeData::ObjectWithIndex(shape_id)) => {
            let shape = db.object_shape(shape_id);
            shape.properties.is_empty()
                && shape.string_index.is_none()
                && shape.number_index.is_none()
        }
        _ => false,
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
    /// This is a Function type - check `is_constructor` flag on the shape
    Function(crate::types::FunctionShapeId),
    /// Recurse into these member types (Union, Intersection)
    Members(Vec<TypeId>),
    /// Recurse into the inner type (`ReadonlyType`)
    Inner(TypeId),
    /// Recurse into the constraint (`TypeParameter`, Infer)
    Constraint(Option<TypeId>),
    /// This type needs full type evaluation (Conditional, Mapped, `IndexAccess`, `KeyOf`)
    NeedsTypeEvaluation,
    /// This is a generic application that needs instantiation
    NeedsApplicationEvaluation,
    /// This is a `TypeQuery` - resolve the symbol reference to get its type
    TypeQuery(crate::types::SymbolRef),
    /// This type cannot be a constructor (primitives, literals, etc.)
    NotConstructor,
}

/// Classify a type for constructor type collection.
///
/// This function examines a `TypeData` and returns information about how to handle it
/// when collecting constructor types. The caller is responsible for:
/// - Checking the `is_constructor` flag for Function types
/// - Evaluating types when `NeedsTypeEvaluation` or `NeedsApplicationEvaluation` is returned
/// - Resolving symbol references for `TypeQuery`
/// - Recursing into members/inner types
///
/// # Example
///
/// ```rust,ignore
/// use crate::type_queries::{classify_constructor_type, ConstructorTypeKind};
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
        TypeData::Callable(_) => ConstructorTypeKind::Callable,
        TypeData::Function(shape_id) => ConstructorTypeKind::Function(shape_id),
        TypeData::Intersection(members_id) | TypeData::Union(members_id) => {
            let members = db.type_list(members_id);
            ConstructorTypeKind::Members(members.to_vec())
        }
        TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => {
            ConstructorTypeKind::Inner(inner)
        }
        TypeData::TypeParameter(info) | TypeData::Infer(info) => {
            ConstructorTypeKind::Constraint(info.constraint)
        }
        TypeData::Conditional(_)
        | TypeData::Mapped(_)
        | TypeData::IndexAccess(_, _)
        | TypeData::KeyOf(_) => ConstructorTypeKind::NeedsTypeEvaluation,
        TypeData::Application(_) => ConstructorTypeKind::NeedsApplicationEvaluation,
        TypeData::TypeQuery(sym_ref) => ConstructorTypeKind::TypeQuery(sym_ref),
        // All other types cannot be constructors
        TypeData::Enum(_, _)
        | TypeData::BoundParameter(_)
        | TypeData::Intrinsic(_)
        | TypeData::Literal(_)
        | TypeData::Object(_)
        | TypeData::ObjectWithIndex(_)
        | TypeData::Array(_)
        | TypeData::Tuple(_)
        | TypeData::Lazy(_)
        | TypeData::Recursive(_)
        | TypeData::TemplateLiteral(_)
        | TypeData::UniqueSymbol(_)
        | TypeData::ThisType
        | TypeData::StringIntrinsic { .. }
        | TypeData::ModuleNamespace(_)
        | TypeData::Error => ConstructorTypeKind::NotConstructor,
    }
}

// =============================================================================
// Static Property Collection Helpers
// =============================================================================

/// Result of extracting static properties from a type.
///
/// This enum allows the caller to handle recursion and type evaluation
/// while keeping the `TypeData` matching logic in the solver layer.
#[derive(Debug, Clone)]
pub enum StaticPropertySource {
    /// Direct properties from Callable, Object, or `ObjectWithIndex` types.
    Properties(Vec<crate::PropertyInfo>),
    /// Member types that should be recursively processed (Union/Intersection).
    RecurseMembers(Vec<TypeId>),
    /// Single type to recurse into (`TypeParameter` constraint, `ReadonlyType` inner).
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
/// This function handles the `TypeData` matching for property collection,
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
        TypeData::Callable(shape_id) => {
            let shape = db.callable_shape(shape_id);
            StaticPropertySource::Properties(shape.properties.to_vec())
        }
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            let shape = db.object_shape(shape_id);
            StaticPropertySource::Properties(shape.properties.to_vec())
        }
        TypeData::Intersection(members_id) | TypeData::Union(members_id) => {
            let members = db.type_list(members_id);
            StaticPropertySource::RecurseMembers(members.to_vec())
        }
        TypeData::TypeParameter(info) | TypeData::Infer(info) => {
            if let Some(constraint) = info.constraint {
                StaticPropertySource::RecurseSingle(constraint)
            } else {
                StaticPropertySource::None
            }
        }
        TypeData::ReadonlyType(inner) => StaticPropertySource::RecurseSingle(inner),
        TypeData::Conditional(_)
        | TypeData::Mapped(_)
        | TypeData::IndexAccess(_, _)
        | TypeData::KeyOf(_) => StaticPropertySource::NeedsEvaluation,
        TypeData::Application(_) => StaticPropertySource::NeedsApplicationEvaluation,
        _ => StaticPropertySource::None,
    }
}

// =============================================================================
// Construct Signature Queries
// =============================================================================

/// Check if a Callable type has construct signatures.
///
/// Returns true only for Callable types that have non-empty `construct_signatures`.
/// This is a direct check and does not resolve through Ref or `TypeQuery` types.
pub fn has_construct_signatures(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    match db.lookup(type_id) {
        Some(TypeData::Callable(shape_id)) => {
            let shape = db.callable_shape(shape_id);
            !shape.construct_signatures.is_empty()
        }
        _ => false,
    }
}

/// Get the symbol reference from a `TypeQuery` type.
///
/// Returns None if the type is not a `TypeQuery`.
pub fn get_symbol_ref_from_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<crate::types::SymbolRef> {
    match db.lookup(type_id) {
        Some(TypeData::TypeQuery(sym_ref)) => Some(sym_ref),
        _ => None,
    }
}

/// Kind of constructable type for `get_construct_type_from_type`.
///
/// This enum represents the different ways a type can be constructable,
/// allowing the caller to handle each case appropriately without matching
/// directly on `TypeData`.
#[derive(Debug, Clone)]
pub enum ConstructableTypeKind {
    /// Callable type with construct signatures - return transformed callable
    CallableWithConstruct,
    /// Callable type without construct signatures - check for prototype property
    CallableMaybePrototype,
    /// Function type - always constructable
    Function,
    /// Reference to a symbol - need to check symbol flags
    SymbolRef(crate::types::SymbolRef),
    /// `TypeQuery` (typeof expr) - need to check symbol flags
    TypeQueryRef(crate::types::SymbolRef),
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
/// - Checking prototype property for `CallableMaybePrototype`
/// - Recursing into constraint for `TypeParameterWithConstraint`
/// - Checking all members for Intersection
pub fn classify_for_constructability(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> ConstructableTypeKind {
    let Some(key) = db.lookup(type_id) else {
        return ConstructableTypeKind::NotConstructable;
    };

    match key {
        TypeData::Callable(shape_id) => {
            let shape = db.callable_shape(shape_id);
            if shape.construct_signatures.is_empty() {
                ConstructableTypeKind::CallableMaybePrototype
            } else {
                ConstructableTypeKind::CallableWithConstruct
            }
        }
        TypeData::Function(_) => ConstructableTypeKind::Function,
        TypeData::TypeQuery(sym_ref) => ConstructableTypeKind::TypeQueryRef(sym_ref),
        TypeData::TypeParameter(info) | TypeData::Infer(info) => {
            if let Some(constraint) = info.constraint {
                ConstructableTypeKind::TypeParameterWithConstraint(constraint)
            } else {
                ConstructableTypeKind::TypeParameterNoConstraint
            }
        }
        TypeData::Intersection(members_id) => {
            let members = db.type_list(members_id);
            ConstructableTypeKind::Intersection(members.to_vec())
        }
        TypeData::Application(_) => ConstructableTypeKind::Application,
        TypeData::Object(_) | TypeData::ObjectWithIndex(_) => ConstructableTypeKind::Object,
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
        Some(TypeData::Callable(shape_id)) => {
            let shape = db.callable_shape(shape_id);
            if shape.construct_signatures.is_empty() {
                None
            } else {
                Some(db.callable(crate::types::CallableShape {
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
    SymbolRef(crate::types::SymbolRef),
    /// Application - evaluate first
    Application { app_id: u32 },
    /// Mapped type - evaluate constraint
    Mapped { mapped_id: u32 },
    /// `KeyOf` - special handling
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
        TypeData::TypeParameter(info) | TypeData::Infer(info) => {
            ConstraintTypeKind::TypeParameter {
                constraint: info.constraint,
                default: info.default,
            }
        }
        TypeData::Union(list_id) => {
            let members = db.type_list(list_id);
            ConstraintTypeKind::Union(members.to_vec())
        }
        TypeData::Intersection(list_id) => {
            let members = db.type_list(list_id);
            ConstraintTypeKind::Intersection(members.to_vec())
        }
        TypeData::Application(app_id) => ConstraintTypeKind::Application { app_id: app_id.0 },
        TypeData::Mapped(mapped_id) => ConstraintTypeKind::Mapped {
            mapped_id: mapped_id.0,
        },
        TypeData::KeyOf(operand) => ConstraintTypeKind::KeyOf(operand),
        TypeData::Literal(_) => ConstraintTypeKind::Resolved(type_id),
        TypeData::BoundParameter(_)
        | TypeData::Intrinsic(_)
        | TypeData::Object(_)
        | TypeData::ObjectWithIndex(_)
        | TypeData::Array(_)
        | TypeData::Tuple(_)
        | TypeData::Function(_)
        | TypeData::Callable(_)
        | TypeData::Conditional(_)
        | TypeData::IndexAccess(_, _)
        | TypeData::TemplateLiteral(_)
        | TypeData::UniqueSymbol(_)
        | TypeData::ThisType
        | TypeData::ReadonlyType(_)
        | TypeData::NoInfer(_)
        | TypeData::TypeQuery(_)
        | TypeData::StringIntrinsic { .. }
        | TypeData::ModuleNamespace(_)
        | TypeData::Enum(_, _)
        | TypeData::Lazy(_)
        | TypeData::Recursive(_)
        | TypeData::Error => ConstraintTypeKind::NoConstraint,
    }
}

// =============================================================================
// Signature Classification
// =============================================================================

/// Classification for types when extracting call/construct signatures.
#[derive(Debug, Clone)]
pub enum SignatureTypeKind {
    /// Callable type with `shape_id` - has `call_signatures` and `construct_signatures`
    Callable(crate::types::CallableShapeId),
    /// Function type with `shape_id` - has single signature
    Function(crate::types::FunctionShapeId),
    /// Union type - get signatures from each member
    Union(Vec<TypeId>),
    /// Intersection type - get signatures from each member
    Intersection(Vec<TypeId>),
    /// Readonly wrapper - unwrap and get signatures from inner type
    ReadonlyType(TypeId),
    /// Type parameter with optional constraint - may need to check constraint
    TypeParameter { constraint: Option<TypeId> },
    /// Types that need evaluation before signature extraction (Conditional, Mapped, `IndexAccess`, `KeyOf`)
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
        TypeData::Callable(shape_id) => SignatureTypeKind::Callable(shape_id),

        // Function types - have a single signature
        TypeData::Function(shape_id) => SignatureTypeKind::Function(shape_id),

        // Union type - get signatures from each member
        TypeData::Union(members_id) => {
            let members = db.type_list(members_id);
            SignatureTypeKind::Union(members.to_vec())
        }

        // Intersection type - get signatures from each member
        TypeData::Intersection(members_id) => {
            let members = db.type_list(members_id);
            SignatureTypeKind::Intersection(members.to_vec())
        }

        // Readonly wrapper - unwrap and recurse
        TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => {
            SignatureTypeKind::ReadonlyType(inner)
        }

        // Type parameter - may have constraint with signatures
        TypeData::TypeParameter(info) | TypeData::Infer(info) => SignatureTypeKind::TypeParameter {
            constraint: info.constraint,
        },

        // Complex types that need evaluation before signature extraction
        TypeData::Conditional(_)
        | TypeData::Mapped(_)
        | TypeData::IndexAccess(_, _)
        | TypeData::KeyOf(_) => SignatureTypeKind::NeedsEvaluation(type_id),

        // All other types don't have callable signatures
        TypeData::BoundParameter(_)
        | TypeData::Intrinsic(_)
        | TypeData::Literal(_)
        | TypeData::Object(_)
        | TypeData::ObjectWithIndex(_)
        | TypeData::Array(_)
        | TypeData::Tuple(_)
        | TypeData::Lazy(_)
        | TypeData::Recursive(_)
        | TypeData::Application(_)
        | TypeData::TemplateLiteral(_)
        | TypeData::UniqueSymbol(_)
        | TypeData::ThisType
        | TypeData::TypeQuery(_)
        | TypeData::StringIntrinsic { .. }
        | TypeData::ModuleNamespace(_)
        | TypeData::Enum(_, _)
        | TypeData::Error => SignatureTypeKind::NoSignatures,
    }
}

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

// =============================================================================
// Property Lookup Type Classification
// =============================================================================

/// Classification for types when looking up properties.
///
/// This enum provides a structured way to handle property lookups on different
/// type kinds, abstracting away the internal `TypeData` representation.
///
/// # Design Principles
///
/// - **No Symbol Resolution**: Keeps solver layer pure
/// - **No Type Evaluation**: Returns classification for caller to handle
/// - **Complete Coverage**: Handles all common property access patterns
#[derive(Debug, Clone)]
pub enum PropertyLookupKind {
    /// Object type with `shape_id` - has properties
    Object(crate::types::ObjectShapeId),
    /// Object with index signature - has properties and index signatures
    ObjectWithIndex(crate::types::ObjectShapeId),
    /// Union type - lookup on each member
    Union(Vec<TypeId>),
    /// Intersection type - lookup on each member
    Intersection(Vec<TypeId>),
    /// Array type - element type for numeric access
    Array(TypeId),
    /// Tuple type - element types
    Tuple(Vec<crate::types::TupleElement>),
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
/// - Accessing the object shape using the returned `shape_id`
///
/// # Example
///
/// ```ignore
/// use crate::type_queries::{classify_for_property_lookup, PropertyLookupKind};
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
        TypeData::Object(shape_id) => PropertyLookupKind::Object(shape_id),
        TypeData::ObjectWithIndex(shape_id) => PropertyLookupKind::ObjectWithIndex(shape_id),
        TypeData::Union(list_id) => {
            let members = db.type_list(list_id);
            PropertyLookupKind::Union(members.to_vec())
        }
        TypeData::Intersection(list_id) => {
            let members = db.type_list(list_id);
            PropertyLookupKind::Intersection(members.to_vec())
        }
        TypeData::Array(elem_type) => PropertyLookupKind::Array(elem_type),
        TypeData::Tuple(tuple_id) => {
            let elements = db.tuple_list(tuple_id);
            PropertyLookupKind::Tuple(elements.to_vec())
        }
        // All other types don't have direct properties for this use case
        TypeData::BoundParameter(_)
        | TypeData::Intrinsic(_)
        | TypeData::Literal(_)
        | TypeData::Function(_)
        | TypeData::Callable(_)
        | TypeData::TypeParameter(_)
        | TypeData::Infer(_)
        | TypeData::Lazy(_)
        | TypeData::Recursive(_)
        | TypeData::Application(_)
        | TypeData::Conditional(_)
        | TypeData::Mapped(_)
        | TypeData::IndexAccess(_, _)
        | TypeData::KeyOf(_)
        | TypeData::TemplateLiteral(_)
        | TypeData::UniqueSymbol(_)
        | TypeData::ThisType
        | TypeData::TypeQuery(_)
        | TypeData::ReadonlyType(_)
        | TypeData::NoInfer(_)
        | TypeData::StringIntrinsic { .. }
        | TypeData::ModuleNamespace(_)
        | TypeData::Enum(_, _)
        | TypeData::Error => PropertyLookupKind::NoProperties,
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
    SymbolRef(crate::types::SymbolRef),
    /// Type query (typeof) - evaluate first
    TypeQuery(crate::types::SymbolRef),
    /// Generic application - instantiate first
    Application {
        app_id: crate::types::TypeApplicationId,
    },
    /// Index access T[K] - evaluate with environment
    IndexAccess { object: TypeId, index: TypeId },
    /// `KeyOf` type - evaluate
    KeyOf(TypeId),
    /// Mapped type - evaluate
    Mapped {
        mapped_id: crate::types::MappedTypeId,
    },
    /// Conditional type - evaluate
    Conditional {
        cond_id: crate::types::ConditionalTypeId,
    },
    /// Callable type (for contextual typing checks)
    Callable(crate::types::CallableShapeId),
    /// Function type
    Function(crate::types::FunctionShapeId),
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
        TypeData::TypeQuery(sym_ref) => EvaluationNeeded::TypeQuery(sym_ref),
        TypeData::Application(app_id) => EvaluationNeeded::Application { app_id },
        TypeData::IndexAccess(object, index) => EvaluationNeeded::IndexAccess { object, index },
        TypeData::KeyOf(inner) => EvaluationNeeded::KeyOf(inner),
        TypeData::Mapped(mapped_id) => EvaluationNeeded::Mapped { mapped_id },
        TypeData::Conditional(cond_id) => EvaluationNeeded::Conditional { cond_id },
        TypeData::Callable(shape_id) => EvaluationNeeded::Callable(shape_id),
        TypeData::Function(shape_id) => EvaluationNeeded::Function(shape_id),
        TypeData::Union(list_id) => {
            let members = db.type_list(list_id);
            EvaluationNeeded::Union(members.to_vec())
        }
        TypeData::Intersection(list_id) => {
            let members = db.type_list(list_id);
            EvaluationNeeded::Intersection(members.to_vec())
        }
        TypeData::TypeParameter(info) | TypeData::Infer(info) => EvaluationNeeded::TypeParameter {
            constraint: info.constraint,
        },
        TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => {
            EvaluationNeeded::Readonly(inner)
        }
        // Already resolved types (Lazy needs special handling when DefId lookup is implemented)
        TypeData::BoundParameter(_)
        | TypeData::Intrinsic(_)
        | TypeData::Literal(_)
        | TypeData::Object(_)
        | TypeData::ObjectWithIndex(_)
        | TypeData::Array(_)
        | TypeData::Tuple(_)
        | TypeData::Lazy(_)
        | TypeData::Recursive(_)
        | TypeData::TemplateLiteral(_)
        | TypeData::UniqueSymbol(_)
        | TypeData::ThisType
        | TypeData::StringIntrinsic { .. }
        | TypeData::ModuleNamespace(_)
        | TypeData::Enum(_, _)
        | TypeData::Error => EvaluationNeeded::Resolved(type_id),
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
    SymbolRef(crate::types::SymbolRef),
    /// Type query (typeof) - needs symbol resolution
    TypeQuery(crate::types::SymbolRef),
    /// Generic application - needs instantiation
    Application {
        app_id: crate::types::TypeApplicationId,
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
    /// Needs evaluation (Conditional, Mapped, `KeyOf`)
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
        TypeData::Object(_) | TypeData::ObjectWithIndex(_) => {
            PropertyAccessClassification::Direct(type_id)
        }
        TypeData::TypeQuery(sym_ref) => PropertyAccessClassification::TypeQuery(sym_ref),
        TypeData::Application(app_id) => PropertyAccessClassification::Application { app_id },
        TypeData::Union(list_id) => {
            let members = db.type_list(list_id);
            PropertyAccessClassification::Union(members.to_vec())
        }
        TypeData::Intersection(list_id) => {
            let members = db.type_list(list_id);
            PropertyAccessClassification::Intersection(members.to_vec())
        }
        TypeData::IndexAccess(object, index) => {
            PropertyAccessClassification::IndexAccess { object, index }
        }
        TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => PropertyAccessClassification::Readonly(inner),
        TypeData::Function(_) | TypeData::Callable(_) => {
            PropertyAccessClassification::Callable(type_id)
        }
        TypeData::TypeParameter(info) | TypeData::Infer(info) => {
            PropertyAccessClassification::TypeParameter {
                constraint: info.constraint,
            }
        }
        TypeData::Conditional(_) | TypeData::Mapped(_) | TypeData::KeyOf(_) => {
            PropertyAccessClassification::NeedsEvaluation(type_id)
        }
        // BoundParameter is a resolved type (leaf node)
        TypeData::BoundParameter(_)
        // Primitives and resolved types (Lazy needs special handling when DefId lookup is implemented)
        | TypeData::Intrinsic(_)
        | TypeData::Literal(_)
        | TypeData::Array(_)
        | TypeData::Tuple(_)
        | TypeData::Lazy(_)
        | TypeData::Recursive(_)
        | TypeData::TemplateLiteral(_)
        | TypeData::UniqueSymbol(_)
        | TypeData::ThisType
        | TypeData::StringIntrinsic { .. }
        | TypeData::ModuleNamespace(_)
        | TypeData::Error
        | TypeData::Enum(_, _) => PropertyAccessClassification::Resolved(type_id),
    }
}

// =============================================================================
// TypeTraversalKind - Classification for type structure traversal
// =============================================================================

/// Classification for traversing type structure to resolve symbols.
///
/// This enum is used by `ensure_application_symbols_resolved_inner` to
/// determine how to traverse into nested types without directly matching
/// on `TypeData` in the checker layer.
#[derive(Debug, Clone)]
pub enum TypeTraversalKind {
    /// Application type - resolve base symbol and recurse into base and args
    Application {
        app_id: crate::types::TypeApplicationId,
        base: TypeId,
        args: Vec<TypeId>,
    },
    /// Symbol reference - resolve the symbol
    SymbolRef(crate::types::SymbolRef),
    /// Lazy type reference (`DefId`) - needs resolution before traversal
    Lazy(crate::def::DefId),
    /// Type query (typeof X) - value-space reference that needs resolution
    TypeQuery(crate::types::SymbolRef),
    /// Type parameter - recurse into constraint and default if present
    TypeParameter {
        constraint: Option<TypeId>,
        default: Option<TypeId>,
    },
    /// Union or intersection - recurse into members
    Members(Vec<TypeId>),
    /// Function type - recurse into type params, params, return type, etc.
    Function(crate::types::FunctionShapeId),
    /// Callable type - recurse into signatures and properties
    Callable(crate::types::CallableShapeId),
    /// Object type - recurse into properties and index signatures
    Object(crate::types::ObjectShapeId),
    /// Array type - recurse into element type
    Array(TypeId),
    /// Tuple type - recurse into element types
    Tuple(crate::types::TupleListId),
    /// Conditional type - recurse into check, extends, true, and false types
    Conditional(crate::types::ConditionalTypeId),
    /// Mapped type - recurse into constraint, template, and name type
    Mapped(crate::types::MappedTypeId),
    /// Readonly wrapper - recurse into inner type
    Readonly(TypeId),
    /// Index access - recurse into object and index types
    IndexAccess { object: TypeId, index: TypeId },
    /// `KeyOf` - recurse into inner type
    KeyOf(TypeId),
    /// Template literal - extract types from spans
    TemplateLiteral(Vec<TypeId>),
    /// String intrinsic - traverse the type argument
    StringIntrinsic(TypeId),
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
        TypeData::Application(app_id) => {
            let app = db.type_application(app_id);
            TypeTraversalKind::Application {
                app_id,
                base: app.base,
                args: app.args.clone(),
            }
        }
        TypeData::TypeParameter(info) | TypeData::Infer(info) => TypeTraversalKind::TypeParameter {
            constraint: info.constraint,
            default: info.default,
        },
        TypeData::Union(list_id) | TypeData::Intersection(list_id) => {
            let members = db.type_list(list_id);
            TypeTraversalKind::Members(members.to_vec())
        }
        TypeData::Function(shape_id) => TypeTraversalKind::Function(shape_id),
        TypeData::Callable(shape_id) => TypeTraversalKind::Callable(shape_id),
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            TypeTraversalKind::Object(shape_id)
        }
        TypeData::Array(elem) => TypeTraversalKind::Array(elem),
        TypeData::Tuple(list_id) => TypeTraversalKind::Tuple(list_id),
        TypeData::Conditional(cond_id) => TypeTraversalKind::Conditional(cond_id),
        TypeData::Mapped(mapped_id) => TypeTraversalKind::Mapped(mapped_id),
        TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => {
            TypeTraversalKind::Readonly(inner)
        }
        TypeData::IndexAccess(object, index) => TypeTraversalKind::IndexAccess { object, index },
        TypeData::KeyOf(inner) => TypeTraversalKind::KeyOf(inner),
        // Template literal - extract types from spans for traversal
        TypeData::TemplateLiteral(list_id) => {
            let spans = db.template_list(list_id);
            let types: Vec<TypeId> = spans
                .iter()
                .filter_map(|span| match span {
                    crate::types::TemplateSpan::Type(id) => Some(*id),
                    _ => None,
                })
                .collect();
            if types.is_empty() {
                TypeTraversalKind::Terminal
            } else {
                TypeTraversalKind::TemplateLiteral(types)
            }
        }
        // String intrinsic - traverse the type argument
        TypeData::StringIntrinsic { type_arg, .. } => TypeTraversalKind::StringIntrinsic(type_arg),
        // Lazy type reference - needs resolution before traversal
        TypeData::Lazy(def_id) => TypeTraversalKind::Lazy(def_id),
        // Type query (typeof X) - value-space reference
        TypeData::TypeQuery(symbol_ref) => TypeTraversalKind::TypeQuery(symbol_ref),
        // Terminal types - no nested types to traverse
        TypeData::BoundParameter(_)
        | TypeData::Intrinsic(_)
        | TypeData::Literal(_)
        | TypeData::Recursive(_)
        | TypeData::UniqueSymbol(_)
        | TypeData::ThisType
        | TypeData::ModuleNamespace(_)
        | TypeData::Error
        | TypeData::Enum(_, _) => TypeTraversalKind::Terminal,
    }
}

/// Check if a type is a lazy type and return the `DefId`.
///
/// This is a helper for checking if the base of an Application is a Lazy type.
pub fn get_lazy_if_def(db: &dyn TypeDatabase, type_id: TypeId) -> Option<crate::def::DefId> {
    match db.lookup(type_id) {
        Some(TypeData::Lazy(def_id)) => Some(def_id),
        _ => None,
    }
}

/// High-level property traversal classification for diagnostics/reporting.
///
/// This keeps traversal-shape branching inside solver queries so checker code
/// can remain thin orchestration.
#[derive(Debug, Clone)]
pub enum PropertyTraversalKind {
    Object(std::sync::Arc<crate::types::ObjectShape>),
    Callable(std::sync::Arc<crate::types::CallableShape>),
    Members(Vec<TypeId>),
    Other,
}

/// Classify a type into a property traversal shape for checker diagnostics.
pub fn classify_property_traversal(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> PropertyTraversalKind {
    match classify_for_traversal(db, type_id) {
        TypeTraversalKind::Object(_) => get_object_shape(db, type_id)
            .map_or(PropertyTraversalKind::Other, PropertyTraversalKind::Object),
        TypeTraversalKind::Callable(_) => get_callable_shape(db, type_id).map_or(
            PropertyTraversalKind::Other,
            PropertyTraversalKind::Callable,
        ),
        TypeTraversalKind::Members(members) => PropertyTraversalKind::Members(members),
        _ => PropertyTraversalKind::Other,
    }
}

/// Collect property names reachable from a type for diagnostics/suggestions.
///
/// Traversal shape decisions stay in solver so checker can remain orchestration-only.
pub fn collect_property_name_atoms_for_diagnostics(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    max_depth: usize,
) -> Vec<Atom> {
    fn collect_inner(
        db: &dyn TypeDatabase,
        type_id: TypeId,
        out: &mut Vec<Atom>,
        depth: usize,
        max_depth: usize,
    ) {
        if depth > max_depth {
            return;
        }
        match classify_property_traversal(db, type_id) {
            PropertyTraversalKind::Object(shape) => {
                for prop in &shape.properties {
                    out.push(prop.name);
                }
            }
            PropertyTraversalKind::Callable(shape) => {
                for prop in &shape.properties {
                    out.push(prop.name);
                }
            }
            PropertyTraversalKind::Members(members) => {
                for member in members {
                    collect_inner(db, member, out, depth + 1, max_depth);
                }
            }
            PropertyTraversalKind::Other => {}
        }
    }

    let mut atoms = Vec::new();
    collect_inner(db, type_id, &mut atoms, 0, max_depth);
    atoms.sort_unstable();
    atoms.dedup();
    atoms
}
