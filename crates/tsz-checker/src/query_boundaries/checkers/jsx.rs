//! JSX checker query boundaries.

use crate::state::CheckerState;
use tsz_solver::construction::{QueryDatabase, TypeDatabase};
use tsz_solver::{DefinitionStore, TypeId, TypeParamInfo};

pub(crate) struct SingleArgTypeApplication {
    pub(crate) base: TypeId,
    pub(crate) arg: TypeId,
}

pub(crate) fn contains_index_access_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::contains_index_access_type(db, type_id)
}

/// Check whether a type surface contains an explicit-readonly mapped type.
pub(crate) fn contains_mapped_type_with_readonly_modifier(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::operations::property::contains_mapped_type_with_readonly_modifier(db, type_id)
}

pub(crate) fn is_exact_readonly_mapped_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    crate::query_boundaries::common::is_mapped_type(db, type_id)
        && contains_mapped_type_with_readonly_modifier(db, type_id)
}

pub(crate) fn instantiate_single_arg_type_alias_body(
    db: &dyn QueryDatabase,
    base_body: TypeId,
    base_params: &[TypeParamInfo],
    arg: TypeId,
) -> Option<TypeId> {
    if base_body == TypeId::ANY || base_body == TypeId::ERROR || base_params.len() != 1 {
        return None;
    }
    let substitution =
        crate::query_boundaries::common::TypeSubstitution::from_args(db, base_params, &[arg]);
    Some(crate::query_boundaries::common::instantiate_type(
        db,
        base_body,
        &substitution,
    ))
}

pub(crate) fn instantiate_type_alias_body(
    db: &dyn QueryDatabase,
    body: TypeId,
    type_params: &[TypeParamInfo],
    type_args: &[TypeId],
) -> TypeId {
    let substitution =
        crate::query_boundaries::common::TypeSubstitution::from_args(db, type_params, type_args);
    crate::query_boundaries::common::instantiate_type(db, body, &substitution)
}

pub(crate) const fn property_access_success_type(
    result: crate::query_boundaries::common::PropertyAccessResult,
) -> Option<TypeId> {
    match result {
        crate::query_boundaries::common::PropertyAccessResult::Success { type_id, .. } => {
            Some(type_id)
        }
        _ => None,
    }
}

pub(crate) const fn property_access_is_success(
    result: crate::query_boundaries::common::PropertyAccessResult,
) -> bool {
    matches!(
        result,
        crate::query_boundaries::common::PropertyAccessResult::Success { .. }
    )
}

pub(crate) fn contains_type_parameters(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    crate::query_boundaries::common::contains_type_parameters(db, type_id)
}

pub(crate) fn contains_error_type_in_args(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    crate::query_boundaries::common::contains_error_type_in_args(db, type_id)
}

pub(crate) fn types_are_assignable(
    checker: &mut CheckerState<'_>,
    source: TypeId,
    target: TypeId,
) -> bool {
    checker.is_assignable_to(source, target)
}

pub(crate) fn has_object_shape(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    crate::query_boundaries::common::object_shape_for_type(db, type_id).is_some()
}

pub(crate) fn type_parameter_constraint(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    crate::query_boundaries::common::type_parameter_constraint(db, type_id)
}

pub(crate) fn union_members(db: &dyn TypeDatabase, type_id: TypeId) -> Option<Vec<TypeId>> {
    crate::query_boundaries::common::union_members(db, type_id)
}

pub(crate) fn union_and_intersection_members(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Vec<TypeId> {
    let mut members = Vec::new();
    if let Some(union_members) = crate::query_boundaries::common::union_members(db, type_id) {
        members.extend(union_members);
    }
    if let Some(intersection_members) =
        crate::query_boundaries::common::intersection_members(db, type_id)
    {
        members.extend(intersection_members);
    }
    members
}

pub(crate) fn element_type_allows_intrinsic_tag(
    db: &dyn TypeDatabase,
    element_type: TypeId,
    tag: &str,
) -> bool {
    let members = crate::query_boundaries::common::union_members(db, element_type)
        .unwrap_or_else(|| vec![element_type]);
    members.into_iter().any(|member| {
        if crate::query_boundaries::common::is_string_type(db, member) {
            return true;
        }
        if let Some(crate::query_boundaries::common::LiteralValue::String(atom)) =
            crate::query_boundaries::common::literal_value(db, member)
        {
            return db.resolve_atom_ref(atom).as_ref() == tag;
        }
        false
    })
}

pub(crate) fn is_type_parameter_like(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    crate::query_boundaries::common::is_type_parameter_like(db, type_id)
}

pub(crate) fn index_access_type_arg_alias_hint(
    db: &dyn TypeDatabase,
    def_store: &DefinitionStore,
    type_id: TypeId,
) -> Option<TypeId> {
    tsz_solver::type_queries::index_access_type_arg_alias_hint(db, def_store, type_id)
}

pub(crate) fn single_arg_type_application(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<SingleArgTypeApplication> {
    let app = crate::query_boundaries::common::type_application(db, type_id)?;
    (app.args.len() == 1).then_some(SingleArgTypeApplication {
        base: app.base,
        arg: app.args[0],
    })
}

pub(crate) fn contains_anonymous_object_surface(
    db: &dyn TypeDatabase,
    def_store: &DefinitionStore,
    type_id: TypeId,
) -> bool {
    contains_anonymous_object_surface_inner(db, def_store, type_id, &mut Vec::new())
}

/// Fetch construct signatures for a class-component-shaped type, falling back
/// to the evaluated form when the unevaluated query returns nothing. The JSX
/// `.props` extractor (`extraction_class_props.rs`) needs both branches but
/// must not grow its direct `query_boundaries::common` reference count
/// (#8225); folding both fetches into one boundary call keeps it at zero.
pub(crate) fn construct_signatures_with_env_fallback(
    db: &dyn TypeDatabase,
    component_type: TypeId,
    evaluated: TypeId,
) -> Option<Vec<tsz_solver::CallSignature>> {
    crate::query_boundaries::common::construct_signatures_for_type(db, component_type)
        .or_else(|| crate::query_boundaries::common::construct_signatures_for_type(db, evaluated))
}

/// Detect the `react16.d.ts` `.props` shape that the JSX TS2322
/// target-display takeover targets: an intersection containing at least one
/// `Application` whose base is a type alias named `Readonly`. The match
/// follows the same `"Readonly"` built-in-name convention used in
/// `tsz_solver::relations::subtype::core`, `…::generics`, and
/// `tsz_checker::state::type_resolution::core`. Used by the JSX validator
/// to restrict the display takeover to the wrapper shape `tsc`'s printer
/// abbreviates to `Readonly<...>`.
pub(crate) fn class_props_is_readonly_wrapper_intersection(
    db: &dyn TypeDatabase,
    def_store: &DefinitionStore,
    class_props: TypeId,
) -> bool {
    let Some(members) = crate::query_boundaries::common::intersection_members(db, class_props)
    else {
        return false;
    };
    members.iter().any(|&member| {
        let Some(app) = crate::query_boundaries::common::type_application(db, member) else {
            return false;
        };
        let Some(def_id) = crate::query_boundaries::common::lazy_def_id(db, app.base) else {
            return false;
        };
        let Some(name_atom) = def_store.get_name(def_id) else {
            return false;
        };
        db.resolve_atom_ref(name_atom).as_ref() == "Readonly"
    })
}

fn contains_anonymous_object_surface_inner(
    db: &dyn TypeDatabase,
    def_store: &DefinitionStore,
    type_id: TypeId,
    visited: &mut Vec<TypeId>,
) -> bool {
    if visited.contains(&type_id) {
        return false;
    }
    visited.push(type_id);

    if tsz_solver::type_queries::get_object_shape_id(db, type_id).is_some()
        && def_store.find_def_for_type(type_id).is_none()
    {
        return true;
    }
    if let Some(members) = tsz_solver::type_queries::get_intersection_members(db, type_id)
        && members
            .iter()
            .any(|&member| contains_anonymous_object_surface_inner(db, def_store, member, visited))
    {
        return true;
    }
    tsz_solver::type_queries::get_union_members(db, type_id).is_some_and(|members| {
        members
            .iter()
            .any(|&member| contains_anonymous_object_surface_inner(db, def_store, member, visited))
    })
}
