//! Additional type query classifiers.
//!
//! Contains classification enums and functions for specific checker scenarios:
//! - Excess property checking
//! - Constructor access levels
//! - Assignability evaluation
//! - Binding element type extraction
//! - Type identity/accessor helpers
//! - Symbol resolution traversal

use crate::{TypeData, TypeDatabase, TypeId};

// =============================================================================
// Excess Properties Classification
// =============================================================================

/// Classification for checking excess properties.
#[derive(Debug, Clone)]
pub enum ExcessPropertiesKind {
    /// Object type (without index signature) - check for excess
    Object(crate::types::ObjectShapeId),
    /// Object with index signature - accepts any property
    ObjectWithIndex(crate::types::ObjectShapeId),
    /// Union - check all members
    Union(Vec<TypeId>),
    /// Intersection - merge known members from all object constituents
    Intersection(Vec<TypeId>),
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
        TypeData::Object(shape_id) => ExcessPropertiesKind::Object(shape_id),
        TypeData::ObjectWithIndex(shape_id) => ExcessPropertiesKind::ObjectWithIndex(shape_id),
        TypeData::Union(list_id) => {
            let members = db.type_list(list_id);
            ExcessPropertiesKind::Union(members.to_vec())
        }
        TypeData::Intersection(list_id) => {
            let members = db.type_list(list_id);
            ExcessPropertiesKind::Intersection(members.to_vec())
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
    /// Ref or `TypeQuery` - resolve symbol
    SymbolRef(crate::types::SymbolRef),
    /// Application - check base
    Application(crate::types::TypeApplicationId),
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
        TypeData::TypeQuery(sym_ref) => ConstructorAccessKind::SymbolRef(sym_ref),
        TypeData::Application(app_id) => ConstructorAccessKind::Application(app_id),
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
        TypeData::Application(_) | TypeData::Lazy(_) => AssignabilityEvalKind::Application,
        TypeData::IndexAccess(_, _)
        | TypeData::KeyOf(_)
        | TypeData::Mapped(_)
        | TypeData::Conditional(_) => AssignabilityEvalKind::NeedsEnvEval,
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
    Tuple(crate::types::TupleListId),
    /// Object type - use property type
    Object(crate::types::ObjectShapeId),
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
        TypeData::Array(elem) => BindingElementTypeKind::Array(elem),
        TypeData::Tuple(list_id) => BindingElementTypeKind::Tuple(list_id),
        TypeData::Object(shape_id) => BindingElementTypeKind::Object(shape_id),
        _ => BindingElementTypeKind::Other,
    }
}

// =============================================================================
// Additional Accessor Helpers
// =============================================================================

/// Get the `DefId` from a Lazy type.
pub fn get_lazy_def_id(db: &dyn TypeDatabase, type_id: TypeId) -> Option<crate::def::DefId> {
    match db.lookup(type_id) {
        Some(TypeData::Lazy(def_id)) => Some(def_id),
        _ => None,
    }
}

/// Get the `DefId` from a Lazy type.
pub fn get_def_id(db: &dyn TypeDatabase, type_id: TypeId) -> Option<crate::def::DefId> {
    match db.lookup(type_id) {
        Some(TypeData::Lazy(def_id)) => Some(def_id),
        _ => None,
    }
}

/// Get the `DefId` from a Lazy type.
/// Returns (Option<SymbolRef>, Option<DefId>) - `DefId` will be Some for Lazy types.
pub fn get_type_identity(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> (Option<crate::types::SymbolRef>, Option<crate::def::DefId>) {
    match db.lookup(type_id) {
        Some(TypeData::Lazy(def_id)) => (None, Some(def_id)),
        _ => (None, None),
    }
}

/// Get the enum components (`DefId` and member type) if the type is an Enum type.
///
/// Returns `Some((def_id, member_type))` where:
/// - `def_id` is the unique identity of the enum for nominal checking
/// - `member_type` is the structural union of member types (e.g., 0 | 1)
pub fn get_enum_components(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<(crate::def::DefId, TypeId)> {
    match db.lookup(type_id) {
        Some(TypeData::Enum(def_id, member_type)) => Some((def_id, member_type)),
        _ => None,
    }
}

/// Get the mapped type ID if the type is a Mapped type.
pub fn get_mapped_type_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<crate::types::MappedTypeId> {
    match db.lookup(type_id) {
        Some(TypeData::Mapped(mapped_id)) => Some(mapped_id),
        _ => None,
    }
}

/// Get the conditional type ID if the type is a Conditional type.
pub fn get_conditional_type_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<crate::types::ConditionalTypeId> {
    match db.lookup(type_id) {
        Some(TypeData::Conditional(cond_id)) => Some(cond_id),
        _ => None,
    }
}

/// Get the keyof inner type if the type is a `KeyOf` type.
pub fn get_keyof_inner(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    match db.lookup(type_id) {
        Some(TypeData::KeyOf(inner)) => Some(inner),
        _ => None,
    }
}

// =============================================================================
// Symbol Resolution Traversal Classification
// =============================================================================

/// Classification for traversing types to resolve symbols.
/// Used by `ensure_application_symbols_resolved_inner`.
#[derive(Debug, Clone)]
pub enum SymbolResolutionTraversalKind {
    /// Application type - resolve base symbol and recurse
    Application {
        app_id: crate::types::TypeApplicationId,
        base: TypeId,
        args: Vec<TypeId>,
    },
    /// Lazy(DefId) type - resolve via `DefId`
    Lazy(crate::def::DefId),
    /// Type parameter - recurse into constraint/default
    TypeParameter {
        constraint: Option<TypeId>,
        default: Option<TypeId>,
    },
    /// Union or Intersection - recurse into members
    Members(Vec<TypeId>),
    /// Function type - recurse into signature components
    Function(crate::types::FunctionShapeId),
    /// Callable type - recurse into signatures
    Callable(crate::types::CallableShapeId),
    /// Object type - recurse into properties and index signatures
    Object(crate::types::ObjectShapeId),
    /// Array type - recurse into element
    Array(TypeId),
    /// Tuple type - recurse into elements
    Tuple(crate::types::TupleListId),
    /// Conditional type - recurse into all branches
    Conditional(crate::types::ConditionalTypeId),
    /// Mapped type - recurse into constraint, template, `name_type`
    Mapped(crate::types::MappedTypeId),
    /// Readonly wrapper - recurse into inner
    Readonly(TypeId),
    /// Index access - recurse into both types
    IndexAccess { object: TypeId, index: TypeId },
    /// `KeyOf` - recurse into inner
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
        TypeData::Application(app_id) => {
            let app = db.type_application(app_id);
            SymbolResolutionTraversalKind::Application {
                app_id,
                base: app.base,
                args: app.args.clone(),
            }
        }
        TypeData::Lazy(def_id) => SymbolResolutionTraversalKind::Lazy(def_id),
        TypeData::TypeParameter(param) | TypeData::Infer(param) => {
            SymbolResolutionTraversalKind::TypeParameter {
                constraint: param.constraint,
                default: param.default,
            }
        }
        TypeData::Union(members_id) | TypeData::Intersection(members_id) => {
            let members = db.type_list(members_id);
            SymbolResolutionTraversalKind::Members(members.to_vec())
        }
        TypeData::Function(shape_id) => SymbolResolutionTraversalKind::Function(shape_id),
        TypeData::Callable(shape_id) => SymbolResolutionTraversalKind::Callable(shape_id),
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            SymbolResolutionTraversalKind::Object(shape_id)
        }
        TypeData::Array(elem) => SymbolResolutionTraversalKind::Array(elem),
        TypeData::Tuple(elems_id) => SymbolResolutionTraversalKind::Tuple(elems_id),
        TypeData::Conditional(cond_id) => SymbolResolutionTraversalKind::Conditional(cond_id),
        TypeData::Mapped(mapped_id) => SymbolResolutionTraversalKind::Mapped(mapped_id),
        TypeData::ReadonlyType(inner) => SymbolResolutionTraversalKind::Readonly(inner),
        TypeData::IndexAccess(obj, idx) => SymbolResolutionTraversalKind::IndexAccess {
            object: obj,
            index: idx,
        },
        TypeData::KeyOf(inner) => SymbolResolutionTraversalKind::KeyOf(inner),
        _ => SymbolResolutionTraversalKind::Terminal,
    }
}
