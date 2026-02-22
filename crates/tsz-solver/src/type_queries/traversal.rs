//! Type traversal and property access classification helpers.
//!
//! This module provides classification enums and functions for traversing
//! type structures. These are used by the checker to determine how to walk
//! into nested types for property access resolution, symbol resolution,
//! and diagnostic property name collection — without directly matching on
//! `TypeData` variants.

use crate::TypeDatabase;
use crate::type_queries::data::{get_callable_shape, get_object_shape};
use crate::types::{IntrinsicKind, TemplateSpan, TypeData, TypeId};
use tsz_common::interner::Atom;

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
                    TemplateSpan::Type(id) => Some(*id),
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

/// Collect property names accessible on a type for spelling suggestions.
///
/// For union types, only properties present in ALL members are returned (intersection).
/// This matches tsc: "did you mean" for union access uses only common/accessible properties.
pub fn collect_accessible_property_names_for_suggestion(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    max_depth: usize,
) -> Vec<Atom> {
    if let Some(TypeData::Union(list_id)) = db.lookup(type_id) {
        let members = db.type_list(list_id).to_vec();
        if members.is_empty() {
            return vec![];
        }
        let mut common = collect_property_name_atoms_for_diagnostics(db, members[0], max_depth);
        common.sort_unstable();
        common.dedup();
        for &member in &members[1..] {
            let mut member_props =
                collect_property_name_atoms_for_diagnostics(db, member, max_depth);
            member_props.sort_unstable();
            member_props.dedup();
            common.retain(|a| member_props.binary_search(a).is_ok());
            if common.is_empty() {
                return vec![];
            }
        }
        return common;
    }
    collect_property_name_atoms_for_diagnostics(db, type_id, max_depth)
}

/// Checks if a type is exclusively `null`, `undefined`, or a union of both.
pub fn is_only_null_or_undefined(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id == TypeId::NULL || type_id == TypeId::UNDEFINED {
        return true;
    }
    match db.lookup(type_id) {
        Some(TypeData::Intrinsic(IntrinsicKind::Null | IntrinsicKind::Undefined)) => true,
        Some(TypeData::Union(list_id)) => {
            let members = db.type_list(list_id);
            members.iter().all(|&m| is_only_null_or_undefined(db, m))
        }
        _ => false,
    }
}
