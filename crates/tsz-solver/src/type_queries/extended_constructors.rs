//! Constructor, Class, and Instance Type Classifiers
//!
//! This module contains type classification functions related to constructors,
//! class declarations, instance types, and abstract class handling.
//! Extracted from `extended.rs` to keep individual files under the 2000 LOC limit.

use crate::def::DefId;
use crate::{TypeData, TypeDatabase, TypeId};
use rustc_hash::FxHashSet;

// =============================================================================
// Abstract Class Type Classification
// =============================================================================

/// Classification for checking if a type contains abstract classes.
#[derive(Debug, Clone)]
pub enum AbstractClassCheckKind {
    /// `TypeQuery` - check if symbol is abstract
    TypeQuery(crate::types::SymbolRef),
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
        TypeData::TypeQuery(sym_ref) => AbstractClassCheckKind::TypeQuery(sym_ref),
        TypeData::Union(list_id) => {
            let members = db.type_list(list_id);
            AbstractClassCheckKind::Union(members.to_vec())
        }
        TypeData::Intersection(list_id) => {
            let members = db.type_list(list_id);
            AbstractClassCheckKind::Intersection(members.to_vec())
        }
        _ => AbstractClassCheckKind::NotAbstract,
    }
}

// =============================================================================
// Class Declaration from Type
// =============================================================================

/// Classification for extracting class declarations from types.
#[derive(Debug, Clone)]
pub enum ClassDeclTypeKind {
    /// Object type with properties (may have brand)
    Object(crate::types::ObjectShapeId),
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
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            ClassDeclTypeKind::Object(shape_id)
        }
        TypeData::Union(list_id) | TypeData::Intersection(list_id) => {
            let members = db.type_list(list_id);
            ClassDeclTypeKind::Members(members.to_vec())
        }
        _ => ClassDeclTypeKind::NotObject,
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
    /// Lazy reference (`DefId`) - resolve to check if it's a class/interface
    Lazy(DefId),
    /// `TypeQuery` (typeof) - check referenced symbol
    TypeQuery(crate::types::SymbolRef),
    /// Conditional type - check true branch for constructability
    Conditional {
        true_type: TypeId,
        false_type: TypeId,
    },
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
        TypeData::TypeParameter(info) | TypeData::Infer(info) => {
            ConstructorCheckKind::TypeParameter {
                constraint: info.constraint,
            }
        }
        TypeData::Intersection(members_id) => {
            let members = db.type_list(members_id);
            ConstructorCheckKind::Intersection(members.to_vec())
        }
        TypeData::Union(members_id) => {
            let members = db.type_list(members_id);
            ConstructorCheckKind::Union(members.to_vec())
        }
        TypeData::Application(app_id) => {
            let app = db.type_application(app_id);
            ConstructorCheckKind::Application { base: app.base }
        }
        TypeData::Lazy(def_id) => ConstructorCheckKind::Lazy(def_id),
        TypeData::TypeQuery(sym_ref) => ConstructorCheckKind::TypeQuery(sym_ref),
        TypeData::Conditional(cond_id) => {
            let cond = db.conditional_type(cond_id);
            ConstructorCheckKind::Conditional {
                true_type: cond.true_type,
                false_type: cond.false_type,
            }
        }
        _ => ConstructorCheckKind::Other,
    }
}

// =============================================================================
// Instance Type from Constructor Classification
// =============================================================================

/// Classification for extracting instance types from constructor types.
#[derive(Debug, Clone)]
pub enum InstanceTypeKind {
    /// Callable type - extract from `construct_signatures` return types
    Callable(crate::types::CallableShapeId),
    /// Function type - check `is_constructor` flag
    Function(crate::types::FunctionShapeId),
    /// Intersection type - recursively extract instance types from members
    Intersection(Vec<TypeId>),
    /// Union type - recursively extract instance types from members
    Union(Vec<TypeId>),
    /// `ReadonlyType` - unwrap and recurse
    Readonly(TypeId),
    /// Type parameter with constraint - follow constraint
    TypeParameter { constraint: Option<TypeId> },
    /// Symbol reference (Ref or `TypeQuery`) - needs resolution to class instance type
    SymbolRef(crate::types::SymbolRef),
    /// Complex types (Conditional, Mapped, `IndexAccess`, `KeyOf`) - need evaluation
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
        TypeData::Callable(shape_id) => InstanceTypeKind::Callable(shape_id),
        TypeData::Function(shape_id) => InstanceTypeKind::Function(shape_id),
        TypeData::Intersection(list_id) => {
            let members = db.type_list(list_id);
            InstanceTypeKind::Intersection(members.to_vec())
        }
        TypeData::Union(list_id) => {
            let members = db.type_list(list_id);
            InstanceTypeKind::Union(members.to_vec())
        }
        TypeData::ReadonlyType(inner) => InstanceTypeKind::Readonly(inner),
        TypeData::TypeParameter(info) | TypeData::Infer(info) => InstanceTypeKind::TypeParameter {
            constraint: info.constraint,
        },
        // TypeQuery (typeof expressions) needs resolution to instance type
        TypeData::TypeQuery(sym_ref) => InstanceTypeKind::SymbolRef(sym_ref),
        TypeData::Conditional(_)
        | TypeData::Mapped(_)
        | TypeData::IndexAccess(_, _)
        | TypeData::KeyOf(_)
        | TypeData::Application(_) => InstanceTypeKind::NeedsEvaluation,
        _ => InstanceTypeKind::NotConstructor,
    }
}

// =============================================================================
// Constructor Return Merge Classification
// =============================================================================

/// Classification for merging base instance into constructor return.
#[derive(Debug, Clone)]
pub enum ConstructorReturnMergeKind {
    /// Callable type - update `construct_signatures`
    Callable(crate::types::CallableShapeId),
    /// Function type - check `is_constructor` flag
    Function(crate::types::FunctionShapeId),
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
        TypeData::Callable(shape_id) => ConstructorReturnMergeKind::Callable(shape_id),
        TypeData::Function(shape_id) => ConstructorReturnMergeKind::Function(shape_id),
        TypeData::Intersection(list_id) => {
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
    /// `TypeQuery` (typeof `AbstractClass`) - check if symbol is abstract
    TypeQuery(crate::types::SymbolRef),
    /// Callable - check if marked as abstract
    Callable(crate::types::CallableShapeId),
    /// Application - check base type
    Application(crate::types::TypeApplicationId),
    /// Not an abstract constructor type
    NotAbstract,
}

/// Fully-resolved abstract-constructor anchor after peeling applications.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbstractConstructorAnchor {
    /// `TypeQuery` (typeof `AbstractClass`) - checker resolves symbol flags.
    TypeQuery(crate::types::SymbolRef),
    /// Callable type id that checker can consult for abstract constructor metadata.
    CallableType(TypeId),
    /// Not an abstract constructor candidate.
    NotAbstract,
}

/// Classify a type for abstract constructor checking.
fn classify_for_abstract_constructor(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> AbstractConstructorKind {
    let Some(key) = db.lookup(type_id) else {
        return AbstractConstructorKind::NotAbstract;
    };

    match key {
        TypeData::TypeQuery(sym_ref) => AbstractConstructorKind::TypeQuery(sym_ref),
        TypeData::Callable(shape_id) => AbstractConstructorKind::Callable(shape_id),
        TypeData::Application(app_id) => AbstractConstructorKind::Application(app_id),
        _ => AbstractConstructorKind::NotAbstract,
    }
}

/// Resolve abstract-constructor candidates by unwrapping application types.
///
/// This keeps type-shape traversal in solver and lets checker only apply
/// source-context rules (e.g. symbol flags and diagnostics).
pub fn resolve_abstract_constructor_anchor(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> AbstractConstructorAnchor {
    let mut current = type_id;
    let mut visited = FxHashSet::default();

    while visited.insert(current) {
        match classify_for_abstract_constructor(db, current) {
            AbstractConstructorKind::TypeQuery(sym_ref) => {
                return AbstractConstructorAnchor::TypeQuery(sym_ref);
            }
            AbstractConstructorKind::Callable(_) => {
                return AbstractConstructorAnchor::CallableType(current);
            }
            AbstractConstructorKind::Application(app_id) => {
                let app = db.type_application(app_id);
                if app.base == current {
                    break;
                }
                current = app.base;
            }
            AbstractConstructorKind::NotAbstract => break,
        }
    }

    AbstractConstructorAnchor::NotAbstract
}

// =============================================================================
// Base Instance Properties Merge Classification
// =============================================================================

/// Classification for merging base instance properties.
#[derive(Debug, Clone)]
pub enum BaseInstanceMergeKind {
    /// Object type with shape
    Object(crate::types::ObjectShapeId),
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
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            BaseInstanceMergeKind::Object(shape_id)
        }
        TypeData::Intersection(list_id) => {
            let members = db.type_list(list_id);
            BaseInstanceMergeKind::Intersection(members.to_vec())
        }
        TypeData::Union(list_id) => {
            let members = db.type_list(list_id);
            BaseInstanceMergeKind::Union(members.to_vec())
        }
        _ => BaseInstanceMergeKind::Other,
    }
}

#[cfg(test)]
#[path = "../../tests/extended_constructors_tests.rs"]
mod tests;
