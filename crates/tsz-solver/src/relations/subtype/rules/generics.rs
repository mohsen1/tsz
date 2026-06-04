use super::super::{SubtypeChecker, SubtypeResult, TypeResolver};

pub(crate) use super::mapped_chain::flatten_mapped_chain;

use crate::def::DefId;

use crate::instantiation::instantiate::fill_application_defaults;

use crate::types::{MappedModifier, MappedType, TypeData, TypeParamInfo};

use crate::types::{MappedTypeId, SymbolRef, TypeApplicationId, TypeId};

use crate::visitor::{
    application_id, array_element_type, contains_type_parameter_named, index_access_parts,
    intersection_list_id, is_empty_object_type, keyof_inner_type, mapped_type_id, object_shape_id,
    object_with_index_shape_id, tuple_list_id, type_param_info, union_list_id,
};

use crate::visitors::visitor_predicates::is_primitive_type;

#[path = "generics_application_helpers.rs"]
mod generics_application_helpers;

#[cfg(test)]
pub(crate) use generics_application_helpers::ONE_SIDED_APP_EXPANSION_MAX_DEPTH;

fn args_contain_type_parameters(
    interner: &dyn crate::construction::TypeDatabase,
    args: &[TypeId],
) -> bool {
    args.iter()
        .any(|arg| crate::visitor::contains_type_parameters(interner, *arg))
}

include!("generics_parts/part1.rs");
include!("generics_parts/part2.rs");

/// Check if a mapped type's `name_type` (as-clause) is a "filtering" conditional.
///
/// A filtering as-clause only produces either the iteration parameter P or `never`,
/// meaning it can only REMOVE keys from the source type, never rename them.
/// Example: `{ [P in keyof T as T[P] extends Function ? P : never]: T[P] }`
///
/// This is used by `check_source_to_homomorphic_mapped` to allow T to be assignable
/// to mapped types that filter keys via as-clauses, since all properties in the
/// result type are also properties of T with the same types.
pub(crate) fn is_filtering_name_type(
    interner: &dyn crate::construction::TypeDatabase,
    name_type: TypeId,
    mapped: &MappedType,
) -> bool {
    // The name_type must be a conditional type (C extends D ? X : Y)
    let Some(TypeData::Conditional(cond_id)) = interner.lookup(name_type) else {
        return false;
    };
    let cond = interner.conditional_type(cond_id);

    // One branch must be the iteration parameter P and the other must be `never`.
    // Pattern 1: C extends D ? P : never (filter-in pattern)
    // Pattern 2: C extends D ? never : P (filter-out/invert pattern)
    let iter_param_name = mapped.type_param.name;

    let true_is_param = is_type_param_with_name(interner, cond.true_type, iter_param_name);
    let false_is_param = is_type_param_with_name(interner, cond.false_type, iter_param_name);
    let true_is_never = cond.true_type == TypeId::NEVER;
    let false_is_never = cond.false_type == TypeId::NEVER;

    (true_is_param && false_is_never) || (false_is_param && true_is_never)
}

/// Check if a type is a type parameter with the given name.
fn is_type_param_with_name(
    interner: &dyn crate::construction::TypeDatabase,
    type_id: TypeId,
    name: tsz_common::interner::Atom,
) -> bool {
    matches!(
        type_param_info(interner, type_id),
        Some(info) if info.name == name
    )
}
