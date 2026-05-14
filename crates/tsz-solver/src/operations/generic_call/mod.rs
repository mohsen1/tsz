//! Generic function call inference.
//!
//! Contains the core generic call resolution logic, including:
//! - Multi-pass type argument inference (Round 1 + Round 2)
//! - Contextual type computation for lambda arguments
//! - Trivial single-type-param fast path
//! - Placeholder normalization

use crate::instantiation::instantiate::{TypeInstantiator, TypeSubstitution, instantiate_generic};
use crate::types::{TypeData, TypeId};
use crate::{TypeDatabase, TypeResolver};
use std::sync::atomic::{AtomicU64, Ordering};
use tsz_common::Atom;

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

/// Check if a type constraint is a primitive type (string, number, boolean, bigint, symbol)
/// or a union containing a primitive. Used to preserve literal types during inference
/// when the constraint implies literals should be kept (e.g., `T extends string`).
fn constraint_is_primitive_type_with_resolver(
    interner: &dyn crate::QueryDatabase,
    resolver: &dyn TypeResolver,
    type_id: TypeId,
) -> bool {
    constraint_is_primitive_type_inner(interner, resolver, type_id, 0)
}

fn literal_preservation_constraint_target(
    interner: &dyn crate::QueryDatabase,
    resolver: &dyn TypeResolver,
    type_id: TypeId,
    depth: u32,
) -> Option<TypeId> {
    if depth >= 4 || type_id.is_intrinsic() {
        return None;
    }

    match interner.lookup(type_id) {
        Some(TypeData::Lazy(def_id)) => resolver
            .resolve_lazy(def_id, interner.as_type_database())
            .map(|resolved| interner.evaluate_type(resolved)),
        Some(TypeData::Application(app_id)) => {
            let app = interner.type_application(app_id);
            let TypeData::Lazy(def_id) = interner.lookup(app.base)? else {
                return None;
            };
            let type_params = resolver.get_lazy_type_params(def_id)?;
            let body = resolver.resolve_lazy(def_id, interner.as_type_database())?;
            let instantiated =
                instantiate_generic(interner.as_type_database(), body, &type_params, &app.args);
            Some(interner.evaluate_type(instantiated))
        }
        Some(
            TypeData::Conditional(_)
            | TypeData::IndexAccess(_, _)
            | TypeData::Mapped(_)
            | TypeData::StringIntrinsic { .. },
        ) => {
            let evaluated = interner.evaluate_type(type_id);
            (evaluated != type_id).then_some(evaluated)
        }
        _ => None,
    }
}

fn constraint_is_primitive_type_inner(
    interner: &dyn crate::QueryDatabase,
    resolver: &dyn TypeResolver,
    type_id: TypeId,
    depth: u32,
) -> bool {
    if type_id == TypeId::STRING
        || type_id == TypeId::NUMBER
        || type_id == TypeId::BOOLEAN
        || type_id == TypeId::BIGINT
        || type_id == TypeId::SYMBOL
    {
        return true;
    }
    if type_id.is_intrinsic() {
        return false;
    }
    if let Some(resolved) =
        literal_preservation_constraint_target(interner, resolver, type_id, depth)
        && resolved != type_id
        && constraint_is_primitive_type_inner(interner, resolver, resolved, depth + 1)
    {
        return true;
    }
    match interner.lookup(type_id) {
        // Literal unions and `keyof T` constraints preserve fresh literal candidates.
        Some(TypeData::Literal(_) | TypeData::KeyOf(_)) => true,
        Some(TypeData::Conditional(conditional_id)) => {
            conditional_infer_constraint_preserves_literals(
                interner,
                resolver,
                conditional_id,
                depth + 1,
            )
        }
        Some(TypeData::Union(list_id)) => {
            let members = interner.type_list(list_id);
            members
                .iter()
                .any(|&m| constraint_is_primitive_type_inner(interner, resolver, m, depth + 1))
        }
        // Intersections like `keyof T & string` — check if any member
        // implies literal preservation.
        Some(TypeData::Intersection(list_id)) => {
            let members = interner.type_list(list_id);
            members
                .iter()
                .any(|&m| constraint_is_primitive_type_inner(interner, resolver, m, depth + 1))
        }
        _ => false,
    }
}

fn infer_type_name(interner: &dyn TypeDatabase, type_id: TypeId) -> Option<Atom> {
    match interner.lookup(type_id) {
        Some(TypeData::Infer(info)) => Some(info.name),
        _ => None,
    }
}

fn type_implies_literals_for_constraint(
    interner: &dyn crate::QueryDatabase,
    resolver: &dyn TypeResolver,
    type_id: TypeId,
    depth: u32,
) -> bool {
    if type_id == TypeId::BOOLEAN_TRUE || type_id == TypeId::BOOLEAN_FALSE {
        return true;
    }
    if type_id.is_intrinsic() {
        return false;
    }
    if let Some(resolved) =
        literal_preservation_constraint_target(interner, resolver, type_id, depth)
        && resolved != type_id
        && type_implies_literals_for_constraint(interner, resolver, resolved, depth + 1)
    {
        return true;
    }
    match interner.lookup(type_id) {
        Some(TypeData::Literal(_)) => true,
        Some(TypeData::Union(list_id) | TypeData::Intersection(list_id)) => {
            interner.type_list(list_id).iter().any(|&member| {
                type_implies_literals_for_constraint(interner, resolver, member, depth + 1)
            })
        }
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => interner
            .object_shape(shape_id)
            .properties
            .iter()
            .any(|prop| {
                type_implies_literals_for_constraint(interner, resolver, prop.type_id, depth + 1)
            }),
        Some(TypeData::Array(elem) | TypeData::ReadonlyType(elem)) => {
            type_implies_literals_for_constraint(interner, resolver, elem, depth + 1)
        }
        Some(TypeData::Tuple(list_id)) => interner.tuple_list(list_id).iter().any(|elem| {
            type_implies_literals_for_constraint(interner, resolver, elem.type_id, depth + 1)
        }),
        Some(TypeData::Application(app_id)) => {
            interner.type_application(app_id).args.iter().any(|&arg| {
                type_implies_literals_for_constraint(interner, resolver, arg, depth + 1)
            })
        }
        _ => false,
    }
}

fn type_contains_infer_name(
    interner: &dyn crate::QueryDatabase,
    resolver: &dyn TypeResolver,
    type_id: TypeId,
    infer_name: Atom,
    depth: u32,
) -> bool {
    if depth >= 8 || type_id.is_intrinsic() {
        return false;
    }
    if infer_type_name(interner, type_id) == Some(infer_name) {
        return true;
    }
    if let Some(resolved) =
        literal_preservation_constraint_target(interner, resolver, type_id, depth)
        && resolved != type_id
        && type_contains_infer_name(interner, resolver, resolved, infer_name, depth + 1)
    {
        return true;
    }
    match interner.lookup(type_id) {
        Some(TypeData::Union(list_id) | TypeData::Intersection(list_id)) => {
            interner.type_list(list_id).iter().any(|&member| {
                type_contains_infer_name(interner, resolver, member, infer_name, depth + 1)
            })
        }
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => interner
            .object_shape(shape_id)
            .properties
            .iter()
            .any(|prop| {
                type_contains_infer_name(interner, resolver, prop.type_id, infer_name, depth + 1)
            }),
        Some(TypeData::Array(elem) | TypeData::ReadonlyType(elem)) => {
            type_contains_infer_name(interner, resolver, elem, infer_name, depth + 1)
        }
        Some(TypeData::Tuple(list_id)) => interner.tuple_list(list_id).iter().any(|elem| {
            type_contains_infer_name(interner, resolver, elem.type_id, infer_name, depth + 1)
        }),
        Some(TypeData::Application(app_id)) => {
            interner.type_application(app_id).args.iter().any(|&arg| {
                type_contains_infer_name(interner, resolver, arg, infer_name, depth + 1)
            })
        }
        Some(TypeData::Conditional(cond_id)) => {
            let cond = interner.conditional_type(cond_id);
            type_contains_infer_name(interner, resolver, cond.check_type, infer_name, depth + 1)
                || type_contains_infer_name(
                    interner,
                    resolver,
                    cond.extends_type,
                    infer_name,
                    depth + 1,
                )
                || type_contains_infer_name(
                    interner,
                    resolver,
                    cond.true_type,
                    infer_name,
                    depth + 1,
                )
                || type_contains_infer_name(
                    interner,
                    resolver,
                    cond.false_type,
                    infer_name,
                    depth + 1,
                )
        }
        Some(TypeData::IndexAccess(object, index)) => {
            type_contains_infer_name(interner, resolver, object, infer_name, depth + 1)
                || type_contains_infer_name(interner, resolver, index, infer_name, depth + 1)
        }
        Some(TypeData::KeyOf(inner) | TypeData::NoInfer(inner)) => {
            type_contains_infer_name(interner, resolver, inner, infer_name, depth + 1)
        }
        _ => false,
    }
}

fn source_type_preserves_literal_inference(
    interner: &dyn crate::QueryDatabase,
    resolver: &dyn TypeResolver,
    source: TypeId,
    depth: u32,
) -> bool {
    type_implies_literals_for_constraint(interner, resolver, source, depth + 1)
        || constraint_is_primitive_type_inner(interner, resolver, source, depth + 1)
}

fn conditional_infer_constraint_preserves_literals(
    interner: &dyn crate::QueryDatabase,
    resolver: &dyn TypeResolver,
    conditional_id: crate::types::ConditionalTypeId,
    depth: u32,
) -> bool {
    let cond = interner.conditional_type(conditional_id);
    let Some(infer_name) = infer_type_name(interner, cond.true_type) else {
        return false;
    };
    infer_match_preserves_literals(
        interner,
        resolver,
        cond.check_type,
        cond.extends_type,
        infer_name,
        depth,
    )
}

fn infer_match_preserves_literals(
    interner: &dyn crate::QueryDatabase,
    resolver: &dyn TypeResolver,
    source: TypeId,
    target: TypeId,
    infer_name: Atom,
    depth: u32,
) -> bool {
    if depth >= 8 {
        return false;
    }
    if infer_type_name(interner, target) == Some(infer_name) {
        return source_type_preserves_literal_inference(interner, resolver, source, depth);
    }
    if let Some(resolved_source) =
        literal_preservation_constraint_target(interner, resolver, source, depth)
        && resolved_source != source
    {
        return infer_match_preserves_literals(
            interner,
            resolver,
            resolved_source,
            target,
            infer_name,
            depth + 1,
        );
    }
    if let Some(resolved_target) =
        literal_preservation_constraint_target(interner, resolver, target, depth)
        && resolved_target != target
    {
        return infer_match_preserves_literals(
            interner,
            resolver,
            source,
            resolved_target,
            infer_name,
            depth + 1,
        );
    }

    match (interner.lookup(source), interner.lookup(target)) {
        (Some(TypeData::Union(source_list)), _) => {
            interner.type_list(source_list).iter().any(|&member| {
                infer_match_preserves_literals(
                    interner,
                    resolver,
                    member,
                    target,
                    infer_name,
                    depth + 1,
                )
            })
        }
        (_, Some(TypeData::Union(target_list) | TypeData::Intersection(target_list))) => {
            interner.type_list(target_list).iter().any(|&member| {
                infer_match_preserves_literals(
                    interner,
                    resolver,
                    source,
                    member,
                    infer_name,
                    depth + 1,
                )
            })
        }
        (
            Some(TypeData::Object(source_shape) | TypeData::ObjectWithIndex(source_shape)),
            Some(TypeData::Object(target_shape) | TypeData::ObjectWithIndex(target_shape)),
        ) => {
            let source_shape = interner.object_shape(source_shape);
            let target_shape = interner.object_shape(target_shape);
            target_shape.properties.iter().any(|target_prop| {
                type_contains_infer_name(
                    interner,
                    resolver,
                    target_prop.type_id,
                    infer_name,
                    depth + 1,
                ) && source_shape
                    .properties
                    .iter()
                    .find(|source_prop| source_prop.name == target_prop.name)
                    .is_some_and(|source_prop| {
                        infer_match_preserves_literals(
                            interner,
                            resolver,
                            source_prop.type_id,
                            target_prop.type_id,
                            infer_name,
                            depth + 1,
                        )
                    })
            })
        }
        (Some(TypeData::Array(source_elem)), Some(TypeData::Array(target_elem))) => {
            infer_match_preserves_literals(
                interner,
                resolver,
                source_elem,
                target_elem,
                infer_name,
                depth + 1,
            )
        }
        (Some(TypeData::Tuple(source_list)), Some(TypeData::Tuple(target_list))) => {
            let source_elems = interner.tuple_list(source_list);
            let target_elems = interner.tuple_list(target_list);
            source_elems
                .iter()
                .zip(target_elems.iter())
                .any(|(source_elem, target_elem)| {
                    infer_match_preserves_literals(
                        interner,
                        resolver,
                        source_elem.type_id,
                        target_elem.type_id,
                        infer_name,
                        depth + 1,
                    )
                })
        }
        _ => false,
    }
}

/// Check whether a type constraint contains a type parameter whose own declared
/// constraint preserves primitive literals.
///
/// This covers dependent generic constraints like Object.freeze's
/// `T extends { [idx: string]: U | null | undefined | object }, U extends string | ...`.
/// `T`'s constraint is object-shaped, but its index value is governed by primitive-
/// constrained `U`, so fresh literal property values must not be widened away.
fn constraint_contains_primitive_constrained_type_param(
    interner: &dyn crate::QueryDatabase,
    resolver: &dyn TypeResolver,
    type_id: TypeId,
    depth: u32,
) -> bool {
    if depth > 4 {
        return false;
    }
    if type_id.is_intrinsic() {
        return false;
    }

    match interner.lookup(type_id) {
        Some(TypeData::TypeParameter(info)) => info.constraint.is_some_and(|constraint| {
            constraint_is_primitive_type_with_resolver(interner, resolver, constraint)
        }),
        Some(TypeData::Union(list_id) | TypeData::Intersection(list_id)) => {
            interner.type_list(list_id).iter().any(|&member| {
                constraint_contains_primitive_constrained_type_param(
                    interner,
                    resolver,
                    member,
                    depth + 1,
                )
            })
        }
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            shape.properties.iter().any(|prop| {
                constraint_contains_primitive_constrained_type_param(
                    interner,
                    resolver,
                    prop.type_id,
                    depth + 1,
                )
            }) || shape.string_index.as_ref().is_some_and(|index| {
                constraint_contains_primitive_constrained_type_param(
                    interner,
                    resolver,
                    index.value_type,
                    depth + 1,
                )
            }) || shape.number_index.as_ref().is_some_and(|index| {
                constraint_contains_primitive_constrained_type_param(
                    interner,
                    resolver,
                    index.value_type,
                    depth + 1,
                )
            })
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

mod contextual_signature_instantiation;
mod inference_helpers;
mod normalization;
mod resolve;
mod return_context;
mod return_context_feedback;

/// Check if a type contains literal types — recursing into unions, intersections,
/// and object properties. Used to detect discriminated union constraints like
/// `{ kind: "a" } | { kind: "b" }` where the literal property types should
/// prevent widening of the corresponding argument properties.
fn type_implies_literals_deep(db: &dyn crate::TypeDatabase, type_id: TypeId) -> bool {
    // Intrinsic IDs are not literal types EXCEPT for `BOOLEAN_TRUE` (14) /
    // `BOOLEAN_FALSE` (15) which are reserved intrinsic IDs that lookup as
    // `TypeData::Literal(Boolean(_))`. All other intrinsics fall to `_ => false`.
    if type_id.is_intrinsic() {
        return type_id == TypeId::BOOLEAN_TRUE || type_id == TypeId::BOOLEAN_FALSE;
    }
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
pub(super) fn type_references_placeholder(
    db: &dyn crate::TypeDatabase,
    type_id: TypeId,
    placeholder: TypeId,
) -> bool {
    if type_id == placeholder {
        return true;
    }
    // Fast path: after the `==` placeholder check above, intrinsics cannot
    // match any of the composite arms below (Union, Intersection, Object,
    // ObjectWithIndex, Array, Tuple, Function, Application, Conditional,
    // IndexAccess, KeyOf, Mapped) — they fall through to `_ => false`.
    // `TypeId::is_intrinsic` is a free `TypeId`-range check.
    if type_id.is_intrinsic() {
        return false;
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
