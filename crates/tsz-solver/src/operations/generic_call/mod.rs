//! Generic function call inference.
//!
//! Contains the core generic call resolution logic, including:
//! - Multi-pass type argument inference (Round 1 + Round 2)
//! - Contextual type computation for lambda arguments
//! - Trivial single-type-param fast path
//! - Placeholder normalization

use crate::TypeDatabase;
use crate::instantiation::instantiate::{TypeInstantiator, TypeSubstitution};
use crate::types::{TypeData, TypeId};
use std::sync::atomic::{AtomicU64, Ordering};

/// Global counter for generating unique inference placeholder names.
/// Each `InferenceContext` starts its variable counter at 0, so placeholder
/// names like `__infer_0` collide across contexts when interned as Atoms.
/// This counter ensures every placeholder gets a globally unique name.
pub(crate) static PLACEHOLDER_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generate a unique placeholder name for an inference variable.
pub(crate) fn unique_placeholder_name(buf: &mut String) {
    use std::fmt::Write;
    let id = PLACEHOLDER_COUNTER.fetch_add(1, Ordering::Relaxed);
    buf.clear();
    write!(buf, "__infer_{id}").expect("write to String is infallible");
}

/// Check if a type constraint is a primitive type (string, number, boolean, bigint)
/// or a union containing a primitive. Used to preserve literal types during inference
/// when the constraint implies literals should be kept (e.g., `T extends string`).
fn constraint_is_primitive_type(interner: &dyn crate::QueryDatabase, type_id: TypeId) -> bool {
    if type_id == TypeId::STRING
        || type_id == TypeId::NUMBER
        || type_id == TypeId::BOOLEAN
        || type_id == TypeId::BIGINT
    {
        return true;
    }
    match interner.lookup(type_id) {
        Some(TypeData::Union(list_id)) => {
            let members = interner.type_list(list_id);
            members
                .iter()
                .any(|&m| constraint_is_primitive_type(interner, m))
        }
        // `keyof T` constraints produce string literal unions at runtime,
        // so literals should be preserved (not widened to `string`).
        Some(TypeData::KeyOf(_)) => true,
        // Intersections like `keyof T & string` — check if any member
        // implies literal preservation.
        Some(TypeData::Intersection(list_id)) => {
            let members = interner.type_list(list_id);
            members
                .iter()
                .any(|&m| constraint_is_primitive_type(interner, m))
        }
        _ => false,
    }
}

fn instantiate_call_type(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
    substitution: &TypeSubstitution,
    actual_this_type: Option<TypeId>,
) -> TypeId {
    if substitution.is_empty() || substitution.is_identity(interner) {
        if let Some(actual_this_type) = actual_this_type {
            let mut instantiator = TypeInstantiator::new(interner, substitution);
            instantiator.this_type = Some(actual_this_type);
            instantiator.instantiate(type_id)
        } else {
            type_id
        }
    } else {
        let mut instantiator = TypeInstantiator::new(interner, substitution);
        instantiator.this_type = actual_this_type;
        instantiator.instantiate(type_id)
    }
}

mod inference_helpers;
mod normalization;
mod resolve;
mod return_context;

/// Check if a type contains literal types — recursing into unions, intersections,
/// and object properties. Used to detect discriminated union constraints like
/// `{ kind: "a" } | { kind: "b" }` where the literal property types should
/// prevent widening of the corresponding argument properties.
fn type_implies_literals_deep(db: &dyn crate::TypeDatabase, type_id: TypeId) -> bool {
    match db.lookup(type_id) {
        Some(TypeData::Literal(_)) => true,
        Some(TypeData::Union(list_id)) => {
            let members = db.type_list(list_id);
            members.iter().any(|&m| type_implies_literals_deep(db, m))
        }
        Some(TypeData::Intersection(list_id)) => {
            let members = db.type_list(list_id);
            members.iter().any(|&m| type_implies_literals_deep(db, m))
        }
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            let shape = db.object_shape(shape_id);
            shape
                .properties
                .iter()
                .any(|prop| type_implies_literals_deep(db, prop.type_id))
        }
        _ => false,
    }
}

/// Check if a type structurally contains a reference to a specific placeholder TypeId.
/// Used to detect when a type parameter (e.g., `TContext`) is referenced inside another
/// type parameter's constraint (e.g., `TMethods` extends Record<string, (ctx: `TContext`) => unknown>).
fn type_references_placeholder(
    db: &dyn crate::TypeDatabase,
    type_id: TypeId,
    placeholder: TypeId,
) -> bool {
    if type_id == placeholder {
        return true;
    }
    match db.lookup(type_id) {
        Some(TypeData::Union(list_id) | TypeData::Intersection(list_id)) => {
            let members = db.type_list(list_id);
            members
                .iter()
                .any(|&m| type_references_placeholder(db, m, placeholder))
        }
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            let shape = db.object_shape(shape_id);
            shape.properties.iter().any(|prop| {
                type_references_placeholder(db, prop.type_id, placeholder)
                    || type_references_placeholder(db, prop.write_type, placeholder)
            })
        }
        Some(TypeData::Array(elem)) => type_references_placeholder(db, elem, placeholder),
        Some(TypeData::Tuple(list_id)) => {
            let elems = db.tuple_list(list_id);
            elems
                .iter()
                .any(|e| type_references_placeholder(db, e.type_id, placeholder))
        }
        Some(TypeData::Function(fn_id)) => {
            let func = db.function_shape(fn_id);
            func.params
                .iter()
                .any(|p| type_references_placeholder(db, p.type_id, placeholder))
                || type_references_placeholder(db, func.return_type, placeholder)
        }
        Some(TypeData::Application(app_id)) => {
            let app = db.type_application(app_id);
            type_references_placeholder(db, app.base, placeholder)
                || app
                    .args
                    .iter()
                    .any(|&a| type_references_placeholder(db, a, placeholder))
        }
        Some(TypeData::Conditional(cond_id)) => {
            let cond = db.conditional_type(cond_id);
            type_references_placeholder(db, cond.check_type, placeholder)
                || type_references_placeholder(db, cond.extends_type, placeholder)
                || type_references_placeholder(db, cond.true_type, placeholder)
                || type_references_placeholder(db, cond.false_type, placeholder)
        }
        Some(TypeData::IndexAccess(obj, idx)) => {
            type_references_placeholder(db, obj, placeholder)
                || type_references_placeholder(db, idx, placeholder)
        }
        Some(TypeData::KeyOf(inner)) => type_references_placeholder(db, inner, placeholder),
        Some(TypeData::Mapped(mapped_id)) => {
            let mapped = db.mapped_type(mapped_id);
            type_references_placeholder(db, mapped.template, placeholder)
                || type_references_placeholder(db, mapped.constraint, placeholder)
                || mapped
                    .name_type
                    .is_some_and(|n| type_references_placeholder(db, n, placeholder))
        }
        _ => false,
    }
}
