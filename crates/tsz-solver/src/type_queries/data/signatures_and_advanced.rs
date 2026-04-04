//! Type parameter queries, signature helpers, function rewrites,
//! conditional/mapped type accessors, literal property key collection,
//! impossible-member pruning, private brand/field queries, enum helpers,
//! and base-type validity checks.

use super::accessors::get_object_shape;
use super::content_predicates::{
    contains_infer_types_db, contains_type_parameters_db, get_intersection_members,
};
use crate::TypeDatabase;
use crate::evaluation::evaluate::TypeEvaluator;
use crate::relations::subtype::SubtypeChecker;
use crate::types::{IntrinsicKind, LiteralValue, TypeData, TypeId};
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_common::Atom;

use crate::type_queries::traversal::collect_property_name_atoms_for_diagnostics;

/// Get the type parameter info if this is a type parameter.
///
/// Returns None if not a type parameter.
pub fn get_type_parameter_info(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<crate::types::TypeParamInfo> {
    match db.lookup(type_id) {
        Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => Some(info),
        _ => None,
    }
}

/// Check if a type is a type parameter (`TypeParameter` or Infer).
pub fn is_type_parameter(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        db.lookup(type_id),
        Some(TypeData::TypeParameter(_) | TypeData::Infer(_))
    )
}

/// Get the constraint of a type parameter.
///
/// Returns None if not a type parameter or has no constraint.
pub fn get_type_parameter_constraint(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    match db.lookup(type_id) {
        Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => info.constraint,
        _ => None,
    }
}

/// Resolve a type parameter to its base constraint for TS2344 checking.
///
/// If the type IS a `TypeParameter` with a constraint, returns the constraint.
/// If it IS a `TypeParameter` without a constraint, returns `unknown`.
/// Returns the type unchanged for anything else (including `Infer` types,
/// composite types, etc.).
///
/// This is used for TS2344 constraint checking: when a type parameter `U extends number`
/// is used as `T extends string`, tsc resolves `U` to `number` and checks `number <: string`.
/// `Infer` types inside conditional types should NOT be resolved here — they are checked
/// during conditional type evaluation, not at type argument validation time.
pub fn get_base_constraint_of_type(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    match db.lookup(type_id) {
        Some(TypeData::TypeParameter(info)) => info.constraint.unwrap_or(TypeId::UNKNOWN),
        _ => type_id,
    }
}

/// Compute the "constituent count" of a type for relation complexity estimation.
///
/// Mirrors tsc's `getConstituentCount` used to detect TS2859 before
/// performing expensive structural comparisons:
/// - Union: sum of constituent counts of all members (additive)
/// - Intersection: product of constituent counts of all members (multiplicative)
/// - Everything else: 1
///
/// The caller compares `source_count * target_count` against a threshold
/// (tsc uses 1,000,000) to decide if the comparison is too complex.
pub fn constituent_count(db: &dyn TypeDatabase, type_id: TypeId) -> u64 {
    match db.lookup(type_id) {
        Some(TypeData::Union(members_id)) => {
            let members = db.type_list(members_id);
            members
                .iter()
                .map(|m| constituent_count(db, *m))
                .sum::<u64>()
                .max(1)
        }
        Some(TypeData::Intersection(members_id)) => {
            let members = db.type_list(members_id);
            members
                .iter()
                .map(|m| constituent_count(db, *m))
                .product::<u64>()
                .max(1)
        }
        _ => 1,
    }
}

/// Get the callable shape for a callable type.
///
/// Returns None if the type is not a Callable.
pub fn get_callable_shape(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<crate::types::CallableShape>> {
    match db.lookup(type_id) {
        Some(TypeData::Callable(shape_id)) => Some(db.callable_shape(shape_id)),
        _ => None,
    }
}

/// Get call signatures from a type.
///
/// For `Callable` types, returns their call signatures directly.
/// For intersection types, collects call signatures from all callable members.
/// Returns None if no call signatures are found.
pub fn get_call_signatures(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<crate::CallSignature>> {
    if let Some(shape) = get_callable_shape(db, type_id) {
        return Some(shape.call_signatures.clone());
    }
    // For intersection types, collect call signatures from all members
    if let Some(members) = get_intersection_members(db, type_id) {
        let mut all_sigs = Vec::new();
        for member in &members {
            if let Some(shape) = get_callable_shape(db, *member) {
                all_sigs.extend(shape.call_signatures.iter().cloned());
            }
        }
        if !all_sigs.is_empty() {
            return Some(all_sigs);
        }
    }
    None
}

/// Get construct signatures from a type.
///
/// For `Callable` types, returns their construct signatures directly.
/// For intersection types, collects construct signatures from all callable members.
/// Returns None if no construct signatures are found.
pub fn get_construct_signatures(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<crate::CallSignature>> {
    if let Some(shape) = get_callable_shape(db, type_id) {
        return Some(shape.construct_signatures.clone());
    }
    // For intersection types, collect construct signatures from all members
    if let Some(members) = get_intersection_members(db, type_id) {
        let mut all_sigs = Vec::new();
        for member in &members {
            if let Some(shape) = get_callable_shape(db, *member) {
                all_sigs.extend(shape.construct_signatures.iter().cloned());
            }
        }
        if !all_sigs.is_empty() {
            return Some(all_sigs);
        }
    }
    None
}

/// Get the union of all construct signature return types from a callable shape.
///
/// Returns `Some(TypeId)` for the union of all construct signature return types,
/// or `None` if the shape has no construct signatures. This encapsulates the common
/// pattern of iterating construct signatures to collect instance types.
pub fn get_construct_return_type_union(
    db: &dyn TypeDatabase,
    shape_id: crate::types::CallableShapeId,
) -> Option<TypeId> {
    let shape = db.callable_shape(shape_id);
    if shape.construct_signatures.is_empty() {
        return None;
    }
    let returns: Vec<TypeId> = shape
        .construct_signatures
        .iter()
        .map(|sig| sig.return_type)
        .collect();
    Some(crate::utils::union_or_single(db, returns))
}

/// Get the construct return type from any type (Callable or Function constructor).
///
/// For Callable types, returns the union of all construct signature return types.
/// For Function types marked as constructors, returns the return type.
/// Returns None for non-constructable types.
pub fn construct_return_type_for_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    use crate::type_queries::extended_constructors::InstanceTypeKind;
    match crate::type_queries::classify_for_instance_type(db, type_id) {
        InstanceTypeKind::Callable(shape_id) => get_construct_return_type_union(db, shape_id),
        InstanceTypeKind::Function(shape_id) => {
            let shape = db.function_shape(shape_id);
            if shape.is_constructor {
                Some(shape.return_type)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Get the function shape for a function type.
///
/// Returns None if the type is not a Function.
pub fn get_function_shape(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<crate::types::FunctionShape>> {
    match db.lookup(type_id) {
        Some(TypeData::Function(shape_id)) => Some(db.function_shape(shape_id)),
        _ => None,
    }
}

/// Return a function type with all `ERROR` parameter and return positions rewritten to `ANY`.
///
/// Returns the original `type_id` when:
/// - it is not a function type
/// - the function shape does not contain `ERROR` in parameter or return positions
pub fn rewrite_function_error_slots_to_any(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    let Some(shape) = get_function_shape(db, type_id) else {
        return type_id;
    };

    let has_error = shape.params.iter().any(|p| p.type_id == TypeId::ERROR)
        || shape.return_type == TypeId::ERROR;
    if !has_error {
        return type_id;
    }

    let params = shape
        .params
        .iter()
        .map(|p| crate::types::ParamInfo {
            type_id: if p.type_id == TypeId::ERROR {
                TypeId::ANY
            } else {
                p.type_id
            },
            ..*p
        })
        .collect();
    let return_type = if shape.return_type == TypeId::ERROR {
        TypeId::ANY
    } else {
        shape.return_type
    };

    db.function(crate::types::FunctionShape {
        type_params: shape.type_params.clone(),
        params,
        this_type: shape.this_type,
        return_type,
        type_predicate: shape.type_predicate,
        is_constructor: shape.is_constructor,
        is_method: shape.is_method,
    })
}

/// Return a function type with the same signature but a replaced return type.
///
/// Returns the original `type_id` when:
/// - it is not a function type
/// - the existing return type already equals `new_return`
pub fn replace_function_return_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    new_return: TypeId,
) -> TypeId {
    let Some(shape) = get_function_shape(db, type_id) else {
        return type_id;
    };
    if shape.return_type == new_return {
        return type_id;
    }

    db.function(crate::types::FunctionShape {
        type_params: shape.type_params.clone(),
        params: shape.params.clone(),
        this_type: shape.this_type,
        return_type: new_return,
        type_predicate: shape.type_predicate,
        is_constructor: shape.is_constructor,
        is_method: shape.is_method,
    })
}

/// Erase a generic function's type parameters by replacing them with `any`.
///
/// This mirrors TSC's `getErasedSignature` used in `isImplementationCompatibleWithOverload`.
/// Returns the original type when it is not a function or has no type parameters.
pub fn erase_function_type_params_to_any(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    let Some(shape) = get_function_shape(db, type_id) else {
        return type_id;
    };
    if shape.type_params.is_empty() {
        return type_id;
    }

    use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};

    let mut subst = TypeSubstitution::new();
    for tp in &shape.type_params {
        subst.insert(tp.name, TypeId::ANY);
    }

    let params = shape
        .params
        .iter()
        .map(|p| crate::types::ParamInfo {
            type_id: instantiate_type(db, p.type_id, &subst),
            ..*p
        })
        .collect();
    let return_type = instantiate_type(db, shape.return_type, &subst);
    let this_type = shape.this_type.map(|t| instantiate_type(db, t, &subst));

    db.function(crate::types::FunctionShape {
        type_params: Vec::new(), // erased
        params,
        this_type,
        return_type,
        type_predicate: shape.type_predicate,
        is_constructor: shape.is_constructor,
        is_method: shape.is_method,
    })
}

/// Get the conditional type info for a conditional type.
///
/// Returns None if the type is not a Conditional.
pub fn get_conditional_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<crate::types::ConditionalType>> {
    match db.lookup(type_id) {
        Some(TypeData::Conditional(cond_id)) => Some(db.conditional_type(cond_id)),
        _ => None,
    }
}

/// Classify a type body for argument preservation during application evaluation.
///
/// When instantiating `type Foo<T> = T extends Bar<infer U> ? U : never` with
/// `Foo<App<number>>`, the checker must decide whether to eagerly evaluate the
/// type argument `App<number>` to its structural form. If the body is a conditional
/// with `infer` patterns, evaluating Application-form args would destroy the
/// structure needed by `try_application_infer_match`.
///
/// Returns a classification that the checker uses to decide arg preservation policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyArgPreservation {
    /// No special preservation needed — evaluate args normally.
    EvaluateAll,
    /// Body is a conditional with `infer` in extends — preserve type-parameter
    /// and Application-form args so the solver's infer matching works correctly.
    ConditionalInfer,
    /// Body is a conditional with an Application containing `infer` in extends —
    /// preserve Application-form args specifically for Application-level infer matching.
    ConditionalApplicationInfer,
}

pub fn classify_body_for_arg_preservation(
    db: &dyn TypeDatabase,
    body_type: TypeId,
) -> BodyArgPreservation {
    let Some(cond) = get_conditional_type(db, body_type) else {
        return BodyArgPreservation::EvaluateAll;
    };
    if contains_infer_types_db(db, cond.extends_type) {
        // Check if extends type is an Application with infer (more specific case)
        if matches!(db.lookup(cond.extends_type), Some(TypeData::Application(_))) {
            return BodyArgPreservation::ConditionalApplicationInfer;
        }
        return BodyArgPreservation::ConditionalInfer;
    }
    BodyArgPreservation::EvaluateAll
}

/// Get the mapped type info for a mapped type.
///
/// Returns None if the type is not a Mapped type.
pub fn get_mapped_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<crate::types::MappedType>> {
    match db.lookup(type_id) {
        Some(TypeData::Mapped(mapped_id)) => Some(db.mapped_type(mapped_id)),
        _ => None,
    }
}

/// Get the mapped type id together with the mapped type info.
///
/// Returns None if the type is not a Mapped type.
pub fn get_mapped_type_with_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<(
    crate::types::MappedTypeId,
    std::sync::Arc<crate::types::MappedType>,
)> {
    match db.lookup(type_id) {
        Some(TypeData::Mapped(mapped_id)) => Some((mapped_id, db.mapped_type(mapped_id))),
        _ => None,
    }
}

/// Get the default type for a type-parameter-like type.
///
/// Returns None if the type is not a `TypeParameter` or `Infer`, or if it has no default.
pub fn get_type_parameter_default(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    match db.lookup(type_id) {
        Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => info.default,
        _ => None,
    }
}

/// Get the type application info for a generic application type.
///
/// Returns None if the type is not an Application.
pub fn get_type_application(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<crate::types::TypeApplication>> {
    match db.lookup(type_id) {
        Some(TypeData::Application(app_id)) => Some(db.type_application(app_id)),
        _ => None,
    }
}

/// Get the index access components (object type and index type).
///
/// Returns None if the type is not an `IndexAccess`.
pub fn get_index_access_types(db: &dyn TypeDatabase, type_id: TypeId) -> Option<(TypeId, TypeId)> {
    match db.lookup(type_id) {
        Some(TypeData::IndexAccess(obj, idx)) => Some((obj, idx)),
        _ => None,
    }
}

/// Instantiate a mapped type template for a specific property key, handling
/// name collisions between the mapped key parameter and outer type parameters.
///
/// When a mapped type template is `IndexAccess(T, K)` and the object type `T`
/// is a `TypeParameter` with the **same name atom** as the mapped key parameter,
/// name-based `TypeSubstitution` would incorrectly replace both `T` and `K`
/// with the key literal.  This happens with e.g. `Readonly<P>` where the lib
/// defines `type Readonly<T> = { readonly [P in keyof T]: T[P] }` and the user
/// has a type parameter also named `P`.
///
/// Returns `IndexAccess(T, key_literal)` when a collision is detected (bypassing
/// substitution), or the normally-substituted template otherwise.
pub fn instantiate_mapped_template_for_property(
    db: &dyn TypeDatabase,
    template: TypeId,
    key_param_name: Atom,
    key_literal: TypeId,
) -> TypeId {
    use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};

    // Check if template is IndexAccess(obj, key) where:
    // Case 1: The key is a TypeParameter matching the mapped key param.
    //   Construct Source[key_literal] directly to avoid name-based substitution
    //   corrupting the source when it contains a same-named outer type parameter
    //   (e.g., `Readonly<Props<P> & P>` where mapped key is also "P").
    // Case 2 (original): The object is a TypeParameter with the same name as the
    //   mapped key parameter (e.g., `Readonly<P>` where T=P from outer scope).
    if let Some((idx_obj, idx_key)) = get_index_access_types(db, template)
        && idx_obj != idx_key
    {
        if let Some(info) = get_type_parameter_info(db, idx_key)
            && info.name == key_param_name
        {
            return db.index_access(idx_obj, key_literal);
        }
        if let Some(info) = get_type_parameter_info(db, idx_obj)
            && info.name == key_param_name
        {
            return db.index_access(idx_obj, key_literal);
        }
    }

    // Normal path: substitute the key parameter name with the key literal
    let mut subst = TypeSubstitution::new();
    subst.insert(key_param_name, key_literal);
    instantiate_type(db, template, &subst)
}

fn collect_exact_literal_property_keys_inner(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    keys: &mut FxHashSet<Atom>,
    visited: &mut FxHashSet<TypeId>,
) -> Option<()> {
    if !visited.insert(type_id) {
        return Some(());
    }

    let evaluated = crate::evaluation::evaluate::evaluate_type(db, type_id);
    if evaluated != type_id {
        return collect_exact_literal_property_keys_inner(db, evaluated, keys, visited);
    }

    match db.lookup(type_id) {
        Some(TypeData::Literal(LiteralValue::String(atom))) => {
            keys.insert(atom);
            Some(())
        }
        Some(TypeData::Literal(LiteralValue::Number(n))) => {
            let atom = db.intern_string(
                &crate::relations::subtype::rules::literals::format_number_for_template(n.0),
            );
            keys.insert(atom);
            Some(())
        }
        Some(TypeData::UniqueSymbol(sym)) => {
            let atom = db.intern_string(&format!("__unique_{}", sym.0));
            keys.insert(atom);
            Some(())
        }
        Some(TypeData::Union(members)) => {
            for &member in db.type_list(members).iter() {
                collect_exact_literal_property_keys_inner(db, member, keys, visited)?;
            }
            Some(())
        }
        Some(TypeData::Intersection(members)) => {
            let mut saw_precise_member = false;
            for &member in db.type_list(members).iter() {
                if collect_exact_literal_property_keys_inner(db, member, keys, visited).is_some() {
                    saw_precise_member = true;
                    continue;
                }
                if intersection_member_preserves_literal_keys(db, member) {
                    continue;
                }
                return None;
            }
            saw_precise_member.then_some(())
        }
        Some(TypeData::Enum(_, members)) => {
            collect_exact_literal_property_keys_inner(db, members, keys, visited)
        }
        Some(TypeData::Conditional(cond_id)) => {
            let cond = db.conditional_type(cond_id);
            let branch = resolve_concrete_conditional_branch(db, &cond)?;
            collect_exact_literal_property_keys_inner(db, branch, keys, visited)
        }
        Some(TypeData::KeyOf(operand)) => {
            collect_exact_literal_property_keys_from_keyof_operand(db, operand, keys, visited)
        }
        Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => {
            info.constraint.and_then(|constraint| {
                collect_exact_literal_property_keys_inner(db, constraint, keys, visited)
            })
        }
        Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => {
            collect_exact_literal_property_keys_inner(db, inner, keys, visited)
        }
        Some(TypeData::Intrinsic(crate::types::IntrinsicKind::Never)) => Some(()),
        _ => None,
    }
}

pub fn collect_exact_literal_property_keys(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<FxHashSet<Atom>> {
    let mut keys = FxHashSet::default();
    let mut visited = FxHashSet::default();
    collect_exact_literal_property_keys_inner(db, type_id, &mut keys, &mut visited)?;
    Some(keys)
}

fn collect_exact_literal_property_keys_from_keyof_operand(
    db: &dyn TypeDatabase,
    operand: TypeId,
    keys: &mut FxHashSet<Atom>,
    visited: &mut FxHashSet<TypeId>,
) -> Option<()> {
    let evaluated_operand = crate::evaluation::evaluate::evaluate_type(db, operand);
    let operand = if evaluated_operand != operand {
        evaluated_operand
    } else {
        operand
    };

    match db.lookup(operand) {
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            let shape = db.object_shape(shape_id);
            if shape.string_index.is_some() || shape.number_index.is_some() {
                return None;
            }
            for prop in &shape.properties {
                keys.insert(prop.name);
            }
            Some(())
        }
        Some(TypeData::Callable(shape_id)) => {
            let shape = db.callable_shape(shape_id);
            if shape.string_index.is_some() || shape.number_index.is_some() {
                return None;
            }
            for prop in &shape.properties {
                keys.insert(prop.name);
            }
            Some(())
        }
        Some(TypeData::Union(_members)) => {
            let narrowed_operand = prune_impossible_object_union_members(db, operand);
            let members = match db.lookup(narrowed_operand) {
                Some(TypeData::Union(members)) => db.type_list(members).to_vec(),
                _ => {
                    return collect_exact_literal_property_keys_from_keyof_operand(
                        db,
                        narrowed_operand,
                        keys,
                        visited,
                    );
                }
            };
            for member in members {
                collect_exact_literal_property_keys_from_keyof_operand(db, member, keys, visited)?;
            }
            Some(())
        }
        Some(TypeData::Intersection(members)) => {
            let members = db.type_list(members);
            let mut saw_precise_member = false;
            for (member_idx, &member) in members.iter().enumerate() {
                let narrowed_member = narrow_keyof_intersection_member_by_literal_discriminants(
                    db, member, &members, member_idx,
                );
                if collect_exact_literal_property_keys_from_keyof_operand(
                    db,
                    narrowed_member,
                    keys,
                    visited,
                )
                .is_some()
                {
                    saw_precise_member = true;
                    continue;
                }
                if intersection_member_preserves_literal_keys(db, narrowed_member) {
                    continue;
                }
                return None;
            }
            saw_precise_member.then_some(())
        }
        Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => {
            info.constraint.and_then(|constraint| {
                collect_exact_literal_property_keys_inner(db, constraint, keys, visited)
            })
        }
        Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => {
            collect_exact_literal_property_keys_from_keyof_operand(db, inner, keys, visited)
        }
        _ => {
            let atoms = collect_property_name_atoms_for_diagnostics(db, operand, 8);
            if atoms.is_empty() {
                None
            } else {
                for atom in atoms {
                    keys.insert(atom);
                }
                Some(())
            }
        }
    }
}

pub(crate) fn narrow_keyof_intersection_member_by_literal_discriminants(
    db: &dyn TypeDatabase,
    member: TypeId,
    intersection_members: &[TypeId],
    member_idx: usize,
) -> TypeId {
    let evaluated_member = crate::evaluation::evaluate::evaluate_type(db, member);
    let member = if evaluated_member != member {
        evaluated_member
    } else {
        member
    };

    let Some(TypeData::Union(list_id)) = db.lookup(member) else {
        return member;
    };

    let mut discriminants = Vec::new();
    for (other_idx, &other_member) in intersection_members.iter().enumerate() {
        if other_idx == member_idx {
            continue;
        }
        let evaluated_other = crate::evaluation::evaluate::evaluate_type(db, other_member);
        let other_member = if evaluated_other != other_member {
            evaluated_other
        } else {
            other_member
        };
        let Some(shape) = get_object_shape(db, other_member) else {
            continue;
        };
        for prop in &shape.properties {
            if crate::type_queries::is_unit_type(db, prop.type_id) {
                discriminants.push((prop.name, prop.type_id));
            }
        }
    }

    if discriminants.is_empty() {
        return member;
    }

    let union_members = db.type_list(list_id);
    let retained: Vec<_> = union_members
        .iter()
        .copied()
        .filter(|&branch| {
            let Some(shape) = get_object_shape(db, branch) else {
                return true;
            };

            discriminants.iter().all(|&(disc_name, disc_type)| {
                let Some(prop) = shape.properties.iter().find(|prop| prop.name == disc_name) else {
                    return true;
                };
                !crate::type_queries::is_unit_type(db, prop.type_id)
                    || crate::is_subtype_of(db, disc_type, prop.type_id)
            })
        })
        .collect();

    if retained.is_empty() || retained.len() == union_members.len() {
        member
    } else if retained.len() == 1 {
        retained[0]
    } else {
        db.union_preserve_members(retained)
    }
}

fn intersection_has_impossible_literal_discriminants(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    let Some(TypeData::Intersection(list_id)) = db.lookup(type_id) else {
        return false;
    };

    let mut discriminants: FxHashMap<Atom, Vec<TypeId>> = FxHashMap::default();
    for &member in db.type_list(list_id).iter() {
        let evaluated_member = crate::evaluation::evaluate::evaluate_type(db, member);
        let member = if evaluated_member != member {
            evaluated_member
        } else {
            member
        };
        let Some(shape) = get_object_shape(db, member) else {
            continue;
        };

        for prop in &shape.properties {
            if !crate::type_queries::is_unit_type(db, prop.type_id) {
                continue;
            }

            let seen = discriminants.entry(prop.name).or_default();
            if seen.iter().any(|&other| {
                !crate::is_subtype_of(db, prop.type_id, other)
                    && !crate::is_subtype_of(db, other, prop.type_id)
            }) {
                return true;
            }
            if !seen.contains(&prop.type_id) {
                seen.push(prop.type_id);
            }
        }
    }

    false
}

fn object_member_has_impossible_required_property(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    let evaluated_type = crate::evaluation::evaluate::evaluate_type(db, type_id);
    let type_id = if evaluated_type != type_id {
        evaluated_type
    } else {
        type_id
    };
    let Some(shape) = get_object_shape(db, type_id) else {
        return false;
    };

    shape.properties.iter().any(|prop| {
        !prop.optional
            && (crate::evaluation::evaluate::evaluate_type(db, prop.type_id) == TypeId::NEVER
                || unit_intersection_is_impossible(db, prop.type_id))
    })
}

fn unit_intersection_is_impossible(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    let evaluated = crate::evaluation::evaluate::evaluate_type(db, type_id);
    let type_id = if evaluated != type_id {
        evaluated
    } else {
        type_id
    };
    let Some(TypeData::Intersection(list_id)) = db.lookup(type_id) else {
        return false;
    };

    let mut units = Vec::new();
    for &member in db.type_list(list_id).iter() {
        let evaluated_member = crate::evaluation::evaluate::evaluate_type(db, member);
        let member = if evaluated_member != member {
            evaluated_member
        } else {
            member
        };
        if !crate::type_queries::is_unit_type(db, member) {
            continue;
        }
        if units.iter().any(|&other| {
            !crate::is_subtype_of(db, member, other) && !crate::is_subtype_of(db, other, member)
        }) {
            return true;
        }
        if !units.contains(&member) {
            units.push(member);
        }
    }

    false
}

pub fn prune_impossible_object_union_members(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    let Some(TypeData::Union(list_id)) = db.lookup(type_id) else {
        return type_id;
    };

    let members = db.type_list(list_id);
    let retained: Vec<_> = members
        .iter()
        .copied()
        .filter(|&member| {
            !intersection_has_impossible_literal_discriminants(db, member)
                && !object_member_has_impossible_required_property(db, member)
        })
        .collect();

    match retained.len() {
        0 => TypeId::NEVER,
        len if len == members.len() => type_id,
        1 => retained[0],
        _ => db.union_preserve_members(retained),
    }
}

fn intersection_member_preserves_literal_keys(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        db.lookup(type_id),
        Some(
            TypeData::Intrinsic(crate::types::IntrinsicKind::String)
                | TypeData::Intrinsic(crate::types::IntrinsicKind::Number)
        )
    )
}

fn resolve_concrete_conditional_branch(
    db: &dyn TypeDatabase,
    cond: &crate::types::ConditionalType,
) -> Option<TypeId> {
    resolve_concrete_conditional_result(db, cond, cond.check_type)
}

fn resolve_concrete_conditional_result(
    db: &dyn TypeDatabase,
    cond: &crate::types::ConditionalType,
    check_input: TypeId,
) -> Option<TypeId> {
    let check_type = crate::evaluation::evaluate::evaluate_type(db, check_input);
    let extends_type = crate::evaluation::evaluate::evaluate_type(db, cond.extends_type);

    if let Some(TypeData::Union(members)) = db.lookup(check_type) {
        let members = db.type_list(members);
        let mut results = Vec::new();
        for &member in members.iter() {
            results.push(resolve_concrete_conditional_result(db, cond, member)?);
        }
        return Some(crate::utils::union_or_single(db, results));
    }

    if contains_type_parameters_db(db, check_type)
        || matches!(check_type, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR)
        || matches!(extends_type, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR)
    {
        return None;
    }

    if let Some(TypeData::StringIntrinsic { kind, type_arg }) = db.lookup(extends_type)
        && type_arg == TypeId::STRING
    {
        let transformed =
            crate::evaluation::evaluate::evaluate_type(db, db.string_intrinsic(kind, check_type));
        return Some(if transformed == check_type {
            cond.true_type
        } else {
            cond.false_type
        });
    }

    if contains_type_parameters_db(db, extends_type)
        && !contains_type_parameters_db(db, cond.check_type)
    {
        let evaluator = TypeEvaluator::new(db);
        if evaluator.type_contains_infer(cond.extends_type) {
            let mut bindings = rustc_hash::FxHashMap::default();
            let mut visited = FxHashSet::default();
            let mut checker = SubtypeChecker::new(db);
            if evaluator.match_infer_pattern(
                check_type,
                cond.extends_type,
                &mut bindings,
                &mut visited,
                &mut checker,
            ) {
                let substituted = evaluator.substitute_infer(cond.true_type, &bindings);
                let evaluated = crate::evaluation::evaluate::evaluate_type(db, substituted);
                return Some(evaluated);
            }
            return Some(cond.false_type);
        }
        return None;
    }

    Some(if crate::is_subtype_of(db, check_type, extends_type) {
        cond.true_type
    } else {
        cond.false_type
    })
}

/// Find the private brand name for a type.
///
/// Private members in TypeScript classes use a "brand" property for nominal typing.
/// The brand is a property named like `__private_brand_#className`.
///
/// Returns the full brand property name (e.g., `"__private_brand_#Foo"`) if found,
/// or None if the type has no private brand.
pub fn get_private_brand_name(db: &dyn TypeDatabase, type_id: TypeId) -> Option<String> {
    match db.lookup(type_id)? {
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            let shape = db.object_shape(shape_id);
            for prop in &shape.properties {
                let name = db.resolve_atom(prop.name);
                if name.starts_with("__private_brand_") {
                    return Some(name);
                }
            }
            None
        }
        TypeData::Callable(shape_id) => {
            let shape = db.callable_shape(shape_id);
            for prop in &shape.properties {
                let name = db.resolve_atom(prop.name);
                if name.starts_with("__private_brand_") {
                    return Some(name);
                }
            }
            None
        }
        _ => None,
    }
}

/// Find the private field name from a type's properties.
///
/// Given a type with private members, returns the name of the first private field
/// (a property starting with `#` that is not a brand marker).
///
/// Returns `Some(field_name)` (e.g., `"#foo"`) if found, None otherwise.
pub fn get_private_field_name(db: &dyn TypeDatabase, type_id: TypeId) -> Option<String> {
    match db.lookup(type_id)? {
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            let shape = db.object_shape(shape_id);
            for prop in &shape.properties {
                let name = db.resolve_atom(prop.name);
                if name.starts_with('#') && !name.starts_with("__private_brand_") {
                    return Some(name);
                }
            }
            None
        }
        TypeData::Callable(shape_id) => {
            let shape = db.callable_shape(shape_id);
            for prop in &shape.properties {
                let name = db.resolve_atom(prop.name);
                if name.starts_with('#') && !name.starts_with("__private_brand_") {
                    return Some(name);
                }
            }
            None
        }
        _ => None,
    }
}

/// Get the symbol associated with a type's shape.
///
/// Checks object, object-with-index, and callable shapes for their `symbol` field.
/// Returns the first `SymbolId` found, or None if the type has no shape with a symbol.
pub fn get_type_shape_symbol(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_binder::SymbolId> {
    match db.lookup(type_id)? {
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            db.object_shape(shape_id).symbol
        }
        TypeData::Callable(shape_id) => db.callable_shape(shape_id).symbol,
        _ => None,
    }
}

/// Get the `DefId` from an Enum type.
///
/// Returns None if the type is not an Enum type.
pub fn get_enum_def_id(db: &dyn TypeDatabase, type_id: TypeId) -> Option<crate::def::DefId> {
    match db.lookup(type_id) {
        Some(TypeData::Enum(def_id, _)) => Some(def_id),
        _ => None,
    }
}

/// Get the structural member type from an Enum type.
///
/// Returns None if the type is not an Enum type.
pub fn get_enum_member_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    match db.lookup(type_id) {
        Some(TypeData::Enum(_, member_type)) => Some(member_type),
        _ => None,
    }
}

/// Check if a type is a valid base type for a class `extends` clause.
///
/// In TypeScript, a valid base type must be:
/// - An object type (with properties/signatures) that is not a generic mapped type
/// - The `object` intrinsic (`NonPrimitive`)
/// - `any`
/// - An intersection where every member is a valid base type
/// - A union where every member is a valid base type (e.g. from overloaded constructors)
/// - A type parameter
///
/// Primitives, `never`, `void`, `undefined`, `null`, `unknown`, and literals
/// are NOT valid base types. Used for TS2509 checking.
#[allow(clippy::match_same_arms)]
pub fn is_valid_base_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    match db.lookup(type_id) {
        Some(TypeData::Intrinsic(IntrinsicKind::Any | IntrinsicKind::Object)) => true,
        Some(TypeData::Object(_) | TypeData::ObjectWithIndex(_)) => true,
        Some(TypeData::Callable(_) | TypeData::Function(_)) => true,
        Some(TypeData::Array(_) | TypeData::Tuple(_)) => true,
        Some(TypeData::TypeParameter(_)) => true,
        Some(TypeData::Intersection(list_id)) => {
            let members = db.type_list(list_id);
            members.iter().all(|&m| is_valid_base_type(db, m))
        }
        Some(TypeData::Union(list_id)) => {
            // Union can arise from construct-signature return-type merging
            // (get_construct_return_type_union). All members must be valid base types.
            let members = db.type_list(list_id);
            !members.is_empty() && members.iter().all(|&m| is_valid_base_type(db, m))
        }
        Some(TypeData::Lazy(_)) => true, // unresolved references are assumed valid
        Some(TypeData::Application(_)) => true, // generic applications are object-like
        Some(TypeData::Mapped(_)) => true, // mapped types are object-like
        Some(TypeData::ReadonlyType(inner)) => is_valid_base_type(db, inner),
        // Intrinsics (never, void, null, etc.), literals, None => not valid base types
        _ => false,
    }
}
