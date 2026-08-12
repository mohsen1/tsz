//! JSX checker query boundaries.

use crate::state::CheckerState;
use tsz_common::Atom;
use tsz_solver::computation::TypeSubstitution;
use tsz_solver::construction::{QueryDatabase, TypeDatabase};
use tsz_solver::{
    CallSignature, DefinitionStore, FunctionShape, ParamInfo, PropertyInfo, TupleElement, TypeId,
    TypeParamInfo,
};

pub(crate) struct SingleArgTypeApplication {
    pub(crate) base: TypeId,
    pub(crate) arg: TypeId,
}

pub(crate) fn contains_index_access_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::contains_index_access_type(db, type_id)
}

/// Whether a JSX attribute relation operand carries a conditional type that is
/// still deferred over a type parameter, making a structural assignability
/// answer unreliable.
///
/// This covers both whole-object spread sources (`{...props}` where `props` is
/// `Omit`/`Pick`/`Exclude`/`Overwrite`/a user conditional over an unresolved
/// `T`) and individual attribute operand types (e.g. `value={props.value}`
/// whose type is `Option<C<T>> | C<T>`). `tsc` treats such instantiable types
/// as *comparable* and does not emit `TS2322`; `tsz`'s structural relation
/// cannot soundly decide them and conservatively reports "not assignable",
/// producing a false positive.
///
/// Keyed purely on type structure (a deferred conditional member, or a
/// conditional surface that still mentions a type parameter) so it is
/// independent of the conditional helper's spelling or the type-parameter
/// name.
pub(crate) fn jsx_relation_operand_is_deferred_conditional(
    db: &dyn TypeDatabase,
    def_store: &DefinitionStore,
    type_id: TypeId,
) -> bool {
    fn one(db: &dyn TypeDatabase, def_store: &DefinitionStore, type_id: TypeId) -> bool {
        if !crate::query_boundaries::common::contains_type_parameters(db, type_id) {
            return false;
        }
        crate::query_boundaries::common::has_deferred_conditional_member(db, type_id)
            || crate::query_boundaries::common::contains_conditional_type(db, type_id)
            || crate::query_boundaries::diagnostics::application_base_has_conditional_alias_body(
                db, def_store, type_id,
            )
    }

    if one(db, def_store, type_id) {
        return true;
    }
    // The operand is frequently an intersection (`Omit<T, K> & Extra`);
    // a conditional-bodied member anywhere in it defers the relation.
    crate::query_boundaries::common::intersection_members(db, type_id)
        .is_some_and(|members| members.iter().any(|&m| one(db, def_store, m)))
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

pub(crate) fn type_has_displayable_name(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    crate::query_boundaries::common::type_has_displayable_name(db, type_id)
}

pub(crate) fn missing_props_are_iterator_protocol_noise(
    db: &dyn TypeDatabase,
    props: &[&tsz_solver::PropertyInfo],
) -> bool {
    if props.len() != 2 {
        return false;
    }
    let mut has_iterator = false;
    let mut has_next = false;
    for prop in props {
        let name = db.resolve_atom_ref(prop.name);
        match (prop.is_symbol_named, name.as_ref()) {
            (true, "[Symbol.iterator]") => has_iterator = true,
            (false, "next") => has_next = true,
            _ => return false,
        }
    }
    has_iterator && has_next
}

pub(crate) fn missing_props_are_intrinsic_collection_protocol_noise(
    db: &dyn TypeDatabase,
    props: &[&tsz_solver::PropertyInfo],
) -> bool {
    if props.is_empty() {
        return false;
    }
    let mut has_iterator = false;
    let mut collection_member_count = 0;
    for prop in props {
        let name = db.resolve_atom_ref(prop.name);
        match (prop.is_symbol_named, name.as_ref()) {
            (true, "[Symbol.iterator]") => has_iterator = true,
            (false, "join" | "length" | "next" | "slice") => {
                collection_member_count += 1;
            }
            _ => return false,
        }
    }
    (has_iterator && collection_member_count > 0) || collection_member_count >= 2
}

pub(crate) fn contains_error_type_in_args(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    crate::query_boundaries::common::contains_error_type_in_args(db, type_id)
}

pub(crate) fn component_satisfies_element_type(
    checker: &mut CheckerState<'_>,
    source: TypeId,
    target: TypeId,
) -> bool {
    checker
        .jsx_element_type_relation_outcome(source, target)
        .related
}

pub(crate) fn props_are_assignable(
    checker: &mut CheckerState<'_>,
    source: TypeId,
    target: TypeId,
) -> bool {
    checker.jsx_props_relation_outcome(source, target).related
}

pub(crate) fn object_type_from_properties(
    db: &dyn TypeDatabase,
    properties: Vec<PropertyInfo>,
) -> TypeId {
    db.object(properties)
}

pub(crate) const fn property_info(name: Atom, type_id: TypeId) -> PropertyInfo {
    PropertyInfo::new(name, type_id)
}

pub(crate) const fn property_info_with_write_type(
    name: Atom,
    type_id: TypeId,
    write_type: TypeId,
) -> PropertyInfo {
    PropertyInfo {
        name,
        type_id,
        write_type,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: tsz_solver::Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
        non_widening: false,
    }
}

pub(crate) fn empty_props_object_type(db: &dyn TypeDatabase) -> TypeId {
    object_type_from_properties(db, Vec::new())
}

pub(crate) fn props_param_type_or_empty(db: &dyn TypeDatabase, params: &[ParamInfo]) -> TypeId {
    params
        .first()
        .map(|param| param.type_id)
        .unwrap_or_else(|| empty_props_object_type(db))
}

pub(crate) fn union_type_from_members(db: &dyn TypeDatabase, members: Vec<TypeId>) -> TypeId {
    db.union(members)
}

pub(crate) fn union_type_from_pair(db: &dyn TypeDatabase, left: TypeId, right: TypeId) -> TypeId {
    db.union2(left, right)
}

pub(crate) fn intersection_type_from_members(
    db: &dyn TypeDatabase,
    members: Vec<TypeId>,
) -> TypeId {
    db.intersection(members)
}

pub(crate) fn intersection_type_from_pair(
    db: &dyn TypeDatabase,
    left: TypeId,
    right: TypeId,
) -> TypeId {
    db.intersection2(left, right)
}

pub(crate) fn array_type_from_element(db: &dyn TypeDatabase, element: TypeId) -> TypeId {
    db.array(element)
}

pub(crate) fn tuple_type_from_elements(
    db: &dyn TypeDatabase,
    elements: Vec<TupleElement>,
) -> TypeId {
    db.tuple(elements)
}

pub(crate) fn tuple_type_from_required_element_types(
    db: &dyn TypeDatabase,
    element_types: Vec<TypeId>,
) -> TypeId {
    let elements = element_types
        .into_iter()
        .map(|type_id| TupleElement {
            type_id,
            name: None,
            optional: false,
            rest: false,
        })
        .collect();
    tuple_type_from_elements(db, elements)
}

pub(crate) fn type_application_from_args(
    db: &dyn TypeDatabase,
    base: TypeId,
    args: Vec<TypeId>,
) -> TypeId {
    db.application(base, args)
}

pub(crate) fn index_access_type(
    db: &dyn TypeDatabase,
    object_type: TypeId,
    index_type: TypeId,
) -> TypeId {
    db.index_access(object_type, index_type)
}

pub(crate) fn function_type_from_shape(db: &dyn TypeDatabase, shape: FunctionShape) -> TypeId {
    crate::query_boundaries::construct_signatures::function_type_from_shape(db, shape)
}

pub(crate) fn function_type_from_parts(
    db: &dyn TypeDatabase,
    params: Vec<ParamInfo>,
    return_type: TypeId,
) -> TypeId {
    function_type_from_shape(db, FunctionShape::new(params, return_type))
}

pub(crate) fn single_required_param_function_type(
    db: &dyn TypeDatabase,
    param_name: Atom,
    param_type: TypeId,
    return_type: TypeId,
) -> TypeId {
    function_type_from_parts(
        db,
        vec![ParamInfo::required(param_name, param_type)],
        return_type,
    )
}

pub(crate) fn function_type_with_mapped_component_types(
    db: &dyn TypeDatabase,
    shape: &FunctionShape,
    mut map_type: impl FnMut(TypeId) -> TypeId,
) -> TypeId {
    let mapped = crate::query_boundaries::construct_signatures::map_function_shape_types(
        shape,
        |_, type_id| map_type(type_id),
    )
    .unwrap_or_else(|| shape.clone());
    function_type_from_shape(db, mapped)
}

pub(crate) fn function_type_without_this(db: &dyn TypeDatabase, shape: &FunctionShape) -> TypeId {
    function_type_from_shape(
        db,
        FunctionShape {
            type_params: shape.type_params.clone(),
            params: shape.params.clone(),
            this_type: None,
            return_type: shape.return_type,
            type_predicate: shape.type_predicate,
            is_constructor: shape.is_constructor,
            is_method: false,
        },
    )
}

pub(crate) fn construct_signature_function_shape(sig: CallSignature) -> FunctionShape {
    FunctionShape {
        type_params: sig.type_params,
        params: sig.params,
        this_type: sig.this_type,
        return_type: sig.return_type,
        type_predicate: sig.type_predicate,
        is_constructor: true,
        is_method: sig.is_method,
    }
}

pub(crate) fn push_required_param(shape: &mut FunctionShape, name: Atom, type_id: TypeId) {
    shape.params.push(ParamInfo::required(name, type_id));
}

pub(crate) fn synthetic_single_param_function_shape(
    type_params: Vec<TypeParamInfo>,
    param_name: Atom,
    param_type: TypeId,
    return_type: TypeId,
) -> FunctionShape {
    FunctionShape {
        type_params,
        params: vec![ParamInfo {
            name: Some(param_name),
            type_id: param_type,
            optional: false,
            rest: false,
            arity_only_optional: false,
        }],
        this_type: None,
        return_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    }
}

pub(crate) fn instantiate_function_shape_preserving_unresolved_params(
    db: &dyn QueryDatabase,
    func: &FunctionShape,
    substitution: &TypeSubstitution,
) -> FunctionShape {
    // JSX Round 1 may produce an empty substitution before callback attributes
    // are contextually typed. Install the callable's exact ownership domain at
    // this boundary rather than relying on the producer to have collected a
    // value already; captured same-spelled binders must remain foreign even in
    // that empty bootstrap pass.
    let mut full_substitution = TypeSubstitution::for_signature_domain(&func.type_params);
    for (&name, &type_id) in substitution.map() {
        full_substitution.insert(name, type_id);
    }
    for type_param in &func.type_params {
        if full_substitution.get(type_param.name).is_none() {
            let preserved_type_param = db.as_type_database().type_param(*type_param);
            full_substitution.insert(type_param.name, preserved_type_param);
        }
    }
    crate::query_boundaries::generic_instantiation::instantiate_function_shape(
        db,
        func,
        &full_substitution,
    )
}

pub(crate) fn has_object_shape(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    crate::query_boundaries::common::object_shape_for_type(db, type_id).is_some()
}

pub(crate) fn type_parameter_constraint(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    crate::query_boundaries::common::type_parameter_constraint(db, type_id)
}

pub(crate) fn union_members(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<crate::query_boundaries::common::TypeIdList> {
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
        .unwrap_or_else(|| vec![element_type].into());
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

/// Return true when a type is a lazy alias/reference with the requested
/// declaration name. Presentation policy uses this to avoid asking the type
/// printer whether an alias happened to render with a particular spelling.
pub(crate) fn type_has_declaration_name(
    db: &dyn TypeDatabase,
    def_store: &DefinitionStore,
    type_id: TypeId,
    expected: &str,
) -> bool {
    let reference_type = crate::query_boundaries::common::type_application(db, type_id)
        .map_or(type_id, |app| app.base);
    let Some(def_id) = crate::query_boundaries::common::lazy_def_id(db, reference_type) else {
        return false;
    };
    def_store
        .get_name(def_id)
        .is_some_and(|name| db.resolve_atom_ref(name).as_ref() == expected)
}

pub(crate) fn library_managed_attributes_infer_surface(
    db: &dyn TypeDatabase,
    def_store: &DefinitionStore,
    type_id: TypeId,
) -> bool {
    if library_managed_attributes_application_surface(db, def_store, type_id) {
        return true;
    }

    db.get_display_alias(type_id)
        .is_some_and(|alias| library_managed_attributes_application_surface(db, def_store, alias))
}

fn library_managed_attributes_application_surface(
    db: &dyn TypeDatabase,
    def_store: &DefinitionStore,
    type_id: TypeId,
) -> bool {
    type_has_declaration_name(db, def_store, type_id, "LibraryManagedAttributes")
        && crate::query_boundaries::diagnostics::application_base_has_conditional_alias_body(
            db, def_store, type_id,
        )
}

pub(crate) fn library_managed_attributes_final_fallback_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<TypeId> {
    library_managed_attributes_final_fallback_type_inner(db, type_id, &mut Vec::new())
}

fn library_managed_attributes_final_fallback_type_inner(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    visited: &mut Vec<TypeId>,
) -> Option<TypeId> {
    if type_id.is_intrinsic() || visited.contains(&type_id) {
        return None;
    }
    visited.push(type_id);

    let cond_id = crate::query_boundaries::common::get_conditional_type_id(db, type_id)?;
    let cond = db.get_conditional(cond_id);

    library_managed_attributes_final_fallback_type_inner(db, cond.false_type, visited)
        .or(Some(cond.false_type))
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

/// Return true when a JSX spread source carries a target props type parameter
/// through a readonly wrapper such as `Readonly<P & Extra>`.
///
/// `React` class `.props` surfaces commonly look like
/// `Readonly<{ children?: ReactNode }> & Readonly<P>`. When such a value is
/// spread into a component whose props target is the bare `P`, the synthesized
/// JSX attrs object can lose the `P` identity and collapse to only enumerable
/// wrapper props such as `children`. This query keeps the type-parameter
/// containment decision structural instead of deriving it from rendered text.
pub(crate) fn spread_source_covers_readonly_wrapped_type_parameter(
    db: &dyn TypeDatabase,
    def_store: &DefinitionStore,
    spread_source: TypeId,
    target: TypeId,
) -> bool {
    if !crate::query_boundaries::common::is_type_parameter_like(db, target) {
        return false;
    }
    spread_source_covers_readonly_wrapped_type_parameter_inner(
        db,
        def_store,
        spread_source,
        target,
        &mut Vec::new(),
    )
}

fn spread_source_covers_readonly_wrapped_type_parameter_inner(
    db: &dyn TypeDatabase,
    def_store: &DefinitionStore,
    spread_source: TypeId,
    target: TypeId,
    visited: &mut Vec<TypeId>,
) -> bool {
    if spread_source.is_intrinsic() || visited.contains(&spread_source) {
        return false;
    }
    visited.push(spread_source);

    if let Some(members) = crate::query_boundaries::common::intersection_members(db, spread_source)
    {
        return members.iter().any(|&member| {
            spread_source_covers_readonly_wrapped_type_parameter_inner(
                db, def_store, member, target, visited,
            )
        });
    }

    let Some(app) = crate::query_boundaries::common::type_application(db, spread_source) else {
        return false;
    };
    if app.args.len() != 1 || !type_has_declaration_name(db, def_store, spread_source, "Readonly") {
        return false;
    }

    type_is_target_or_direct_intersection_member(db, def_store, app.args[0], target)
}

fn type_is_target_or_direct_intersection_member(
    db: &dyn TypeDatabase,
    def_store: &DefinitionStore,
    type_id: TypeId,
    target: TypeId,
) -> bool {
    type_parameter_identity_matches(def_store, type_id, target)
        || crate::query_boundaries::common::intersection_members(db, type_id).is_some_and(
            |members| {
                members
                    .iter()
                    .any(|&member| type_parameter_identity_matches(def_store, member, target))
            },
        )
}

fn type_parameter_identity_matches(
    def_store: &DefinitionStore,
    candidate: TypeId,
    target: TypeId,
) -> bool {
    candidate == target
        || def_store
            .find_def_for_type(candidate)
            .zip(def_store.find_def_for_type(target))
            .is_some_and(|(candidate_def, target_def)| candidate_def == target_def)
}

// ── Type-shape probe wrappers for children.rs ──
// All type-shape queries from JSX checker code route through here rather than
// calling query_boundaries::common directly.

pub(crate) use tsz_solver::operations::property::PropertyAccessResult;

pub(crate) fn intersection_members(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> Option<crate::query_boundaries::common::TypeIdList> {
    crate::query_boundaries::common::intersection_members(db, type_id)
}

pub(crate) fn object_shape_for_type(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<tsz_solver::ObjectShape>> {
    crate::query_boundaries::common::object_shape_for_type(db, type_id)
}

pub(crate) fn is_tuple_type(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> bool {
    crate::query_boundaries::common::is_tuple_type(db, type_id)
}

pub(crate) fn is_array_type(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> bool {
    crate::query_boundaries::common::is_array_type(db, type_id)
}

pub(crate) fn lazy_def_id(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::def::DefId> {
    crate::query_boundaries::common::lazy_def_id(db, type_id)
}

pub(crate) fn function_shape_for_type(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<tsz_solver::FunctionShape>> {
    crate::query_boundaries::common::function_shape_for_type(db, type_id)
}

pub(crate) fn call_signatures_for_type(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<tsz_solver::CallSignature>> {
    crate::query_boundaries::common::call_signatures_for_type(db, type_id)
}

pub(crate) fn type_application(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<tsz_solver::TypeApplication>> {
    crate::query_boundaries::common::type_application(db, type_id)
}

pub(crate) fn is_mapped_type_with_readonly_modifier(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> bool {
    crate::query_boundaries::common::is_mapped_type_with_readonly_modifier(db, type_id)
}

pub(crate) fn is_literal_type(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> bool {
    crate::query_boundaries::common::is_literal_type(db, type_id)
}

pub(crate) fn is_callable_type(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> bool {
    crate::query_boundaries::common::is_callable_type(db, type_id)
}

pub(crate) fn application_info(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> Option<(TypeId, Vec<TypeId>)> {
    crate::query_boundaries::common::application_info(db, type_id)
}

pub(crate) fn array_element_type(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> Option<TypeId> {
    crate::query_boundaries::common::array_element_type(db, type_id)
}

pub(crate) fn tuple_elements(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<tsz_solver::TupleElement>> {
    crate::query_boundaries::common::tuple_elements(db, type_id)
}

pub(crate) fn unwrap_readonly(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> TypeId {
    crate::query_boundaries::common::unwrap_readonly(db, type_id)
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

#[cfg(test)]
mod tests {
    use super::*;
    use tsz_solver::TypeParamOrigin;
    use tsz_solver::construction::TypeInterner;

    #[test]
    fn unresolved_jsx_signature_substitution_preserves_captured_same_named_binder() {
        let interner = TypeInterner::new();
        let file = interner.intern_string("jsx-exact-domain.tsx");
        let name = interner.intern_string("U");
        let captured = TypeParamInfo {
            name,
            constraint: None,
            default: None,
            is_const: false,
            origin: TypeParamOrigin::DeclScoped { file, node: 1 },
        };
        let local = TypeParamInfo {
            origin: TypeParamOrigin::DeclScoped { file, node: 2 },
            ..captured
        };
        let captured_type = interner.fresh_type_param(captured);
        let local_type = interner.fresh_type_param(local);
        let function = FunctionShape {
            type_params: vec![local],
            params: vec![
                ParamInfo::unnamed(captured_type),
                ParamInfo::unnamed(local_type),
            ],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        };

        let instantiated = instantiate_function_shape_preserving_unresolved_params(
            &interner,
            &function,
            &TypeSubstitution::new(),
        );

        assert_eq!(
            tsz_solver::type_param_info(&interner, instantiated.params[0].type_id),
            Some(captured),
        );
        assert_eq!(
            tsz_solver::type_param_info(&interner, instantiated.params[1].type_id),
            Some(local),
        );
    }

    #[test]
    fn component_element_type_check_uses_relation_outcome_boundary() {
        let source = include_str!("jsx.rs");
        let helper = source
            .split("pub(crate) fn component_satisfies_element_type")
            .nth(1)
            .and_then(|tail| tail.split("pub(crate) fn props_are_assignable").next())
            .expect("failed to isolate JSX ElementType relation helper");
        let compact_helper = helper.split_whitespace().collect::<String>();
        let legacy = concat!("diagnostic_relation", "_boolean_guard(");

        assert!(
            compact_helper
                .contains("checker.jsx_element_type_relation_outcome(source,target).related"),
            "JSX ElementType compatibility should route relation decisions through \
             the JSX element-type RelationRequest"
        );
        assert!(
            !compact_helper.contains("checker.assign_relation_outcome(source,target).related"),
            "JSX ElementType compatibility should not use generic assignment request routing"
        );
        assert!(
            !helper.contains(legacy),
            "JSX assignability boundary should not use raw diagnostic relation \
             boolean guards"
        );
    }

    #[test]
    fn props_are_assignable_uses_jsx_props_relation_outcome_boundary() {
        let source = include_str!("jsx.rs");

        assert!(
            source.contains("checker.jsx_props_relation_outcome(source, target).related"),
            "JSX props assignability boundary should route relation decisions \
             through the JSX props relation outcome boundary"
        );
    }
}
