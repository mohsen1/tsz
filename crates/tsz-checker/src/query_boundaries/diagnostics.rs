use super::state::checking as state_checking;
use tsz_solver::construction::{QueryDatabase, TypeDatabase};
use tsz_solver::def::{DefKind, DefinitionStore};
use tsz_solver::{
    CallSignature, CallableShape, FunctionShape, ObjectShape, ParamInfo, PropertyInfo, TypeId,
};

pub(crate) use super::common::{
    PropertyAccessResult, TypeResolver, TypeSubstitution, application_info, array_element_type,
    callable_shape_for_type, callable_shape_for_type_extended, collect_referenced_types,
    construct_signatures_for_type, contains_free_type_parameters,
    contains_generic_indexed_access_surface, contains_type_parameter_named,
    contains_type_parameters, enum_def_id, enum_member_type, function_shape_for_type,
    get_indexed_access_type, get_type_query_symbol_ref, has_call_signatures,
    has_construct_signatures, has_function_shape, index_access_types, instantiate_type,
    intersection_list_id, intersection_members, is_fresh_object_type, is_generic_mapped_type,
    is_intersection_type, is_merged_intersection_object, is_symbol_or_unique_symbol,
    is_template_literal_type, keyof_inner_type, lazy_def_id, literal_value, mapped_type_id,
    mapped_type_info, no_infer_inner_type, object_shape_for_type, readonly_inner_type,
    return_type_for_type, string_literal_value, tuple_elements, type_has_displayable_name,
    type_is_conditional_type_result_with_unresolved_inference, type_param_info,
    type_parameter_constraint, union_list_id, union_members, widen_literal_to_primitive,
    widen_type_deep,
};
// Display-only type-shape predicates routed off the catch-all `common` boundary
// so `error_reporter/` presentation code depends on `diagnostics` exclusively
// (issue #12947). These remain defined in `common` for non-display callers; the
// re-export keeps a single import surface for diagnostic render policy.
pub(crate) use super::common::{
    is_conditional_type, is_generic_application, is_literal_type, is_mapped_type,
    is_type_parameter, is_type_parameter_like, is_type_query_type, is_union_type, widen_type,
};
pub(crate) use tsz_solver::type_queries::AssignmentNumericDisplayChildren;

/// Resolve the binder symbol backing an object type, for diagnostic
/// elaboration (spelling suggestions, missing-property anchors). Used only by
/// `error_reporter/` presentation code, so it is owned by the diagnostics
/// boundary rather than the catch-all `common` boundary (issue #12947).
pub(crate) fn get_object_symbol(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_binder::SymbolId> {
    tsz_solver::type_queries::get_object_symbol(db, type_id)
}

pub(crate) fn object_type_from_properties(
    db: &dyn TypeDatabase,
    properties: Vec<PropertyInfo>,
) -> TypeId {
    db.object(properties)
}

pub(crate) fn object_type_from_shape(db: &dyn TypeDatabase, shape: ObjectShape) -> TypeId {
    db.object_with_index(shape)
}

pub(crate) fn object_type_preserving_display_properties(
    db: &dyn TypeDatabase,
    source: TypeId,
    shape: ObjectShape,
) -> TypeId {
    let new_ty = object_type_from_shape(db, shape);
    if let Some(display_props) = db.get_display_properties(source) {
        db.store_display_properties(new_ty, display_props.as_ref().clone());
    }
    new_ty
}

pub(crate) fn function_type_from_shape(db: &dyn TypeDatabase, shape: FunctionShape) -> TypeId {
    crate::query_boundaries::construct_signatures::function_type_from_shape(db, shape)
}

pub(crate) fn function_type_with_params_replaced(
    db: &dyn TypeDatabase,
    shape: &FunctionShape,
    params: Vec<ParamInfo>,
) -> TypeId {
    function_type_from_shape(
        db,
        FunctionShape {
            type_params: shape.type_params.clone(),
            params,
            this_type: shape.this_type,
            return_type: shape.return_type,
            type_predicate: shape.type_predicate,
            is_constructor: shape.is_constructor,
            is_method: shape.is_method,
        },
    )
}

pub(crate) fn function_type_with_return_replaced(
    db: &dyn TypeDatabase,
    shape: &FunctionShape,
    return_type: TypeId,
) -> TypeId {
    function_type_from_shape(
        db,
        FunctionShape {
            type_params: shape.type_params.clone(),
            params: shape.params.clone(),
            this_type: shape.this_type,
            return_type,
            type_predicate: shape.type_predicate,
            is_constructor: shape.is_constructor,
            is_method: shape.is_method,
        },
    )
}

pub(crate) fn function_type_with_params_and_return_replaced(
    db: &dyn TypeDatabase,
    shape: &FunctionShape,
    params: Vec<ParamInfo>,
    return_type: TypeId,
) -> TypeId {
    function_type_from_shape(
        db,
        FunctionShape {
            type_params: shape.type_params.clone(),
            params,
            this_type: shape.this_type,
            return_type,
            type_predicate: shape.type_predicate,
            is_constructor: shape.is_constructor,
            is_method: shape.is_method,
        },
    )
}

pub(crate) fn callable_type_from_shape(db: &dyn TypeDatabase, shape: CallableShape) -> TypeId {
    db.callable(shape)
}

pub(crate) fn call_only_callable_type(
    db: &dyn TypeDatabase,
    call_signatures: Vec<CallSignature>,
) -> TypeId {
    crate::query_boundaries::construct_signatures::call_only_callable_type(db, call_signatures)
}

pub(crate) fn callable_type_with_signatures_replaced(
    db: &dyn TypeDatabase,
    base: &CallableShape,
    call_signatures: Vec<CallSignature>,
    construct_signatures: Vec<CallSignature>,
) -> TypeId {
    crate::query_boundaries::construct_signatures::callable_with_signatures_replaced(
        db,
        base,
        call_signatures,
        construct_signatures,
    )
}

pub(crate) fn assignment_numeric_display_children(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> AssignmentNumericDisplayChildren {
    tsz_solver::type_queries::assignment_numeric_display_children(db, type_id)
}

/// Apparent type of a non-union callee/constructor source for `tsc`'s
/// `invocationErrorDetails` note (`Type 'X' has no call/construct signatures.`),
/// mirroring `typeToString(getApparentType(type))`: a number-/string-/boolean-/
/// bigint-/symbol-like source (including a literal such as `1` or `"a"`) maps to
/// its boxed wrapper interface (`Number`, `String`, ...), the `object`
/// intrinsic maps to the empty object type `{}`, and every other type is
/// returned unchanged. The literal is widened to its base first so a literal
/// source resolves to the same wrapper `tsc` shows (`1` -> `Number`).
///
/// Returns `None` for `any`/error sources and for unions: `tsc` renders a union
/// callee through the distinct `Not all constituents of type 'U' are ...` /
/// `No constituent of type 'U' is ...` shapes, so the caller keeps the existing
/// union rendering untouched rather than mislabeling it.
pub(crate) fn invocation_signature_detail_apparent_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<TypeId> {
    if type_id == TypeId::ERROR || type_id == TypeId::ANY || is_union_type(db, type_id) {
        return None;
    }
    let widened = super::common::widen_literal_type(db, type_id);
    Some(index_receiver_apparent_type(db, widened))
}

/// Return the body of a non-generic alias shaped as `Foo[keyof Foo]`.
pub(crate) fn indexed_access_alias_body(
    db: &dyn TypeDatabase,
    def_store: &DefinitionStore,
    ty: TypeId,
) -> Option<TypeId> {
    let def_id = tsz_solver::type_queries::get_lazy_def_id(db, ty)?;
    let def = def_store.get(def_id)?;
    if def.kind != DefKind::TypeAlias || !def.type_params.is_empty() {
        return None;
    }
    let body = def.body?;
    tsz_solver::type_queries::indexed_access_self_keyof(db, body)?;
    Some(body)
}

/// Returns true if `ty` is still a deferred form after display reduction.
pub(crate) fn is_unresolved_for_display(db: &dyn TypeDatabase, ty: TypeId) -> bool {
    tsz_solver::type_queries::is_deferred_lazy_or_indexed_access(db, ty)
}

pub(crate) fn type_may_display_iterator_protocol(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::type_may_display_iterator_protocol(db, type_id)
}

/// Return `true` if a function/callable signature contains a `typeof` query.
pub(crate) fn function_signature_has_typeof(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if let Some(shape) = tsz_solver::type_queries::get_function_shape(db, type_id)
        && (tsz_solver::is_type_query_type(db, shape.return_type)
            || shape
                .params
                .iter()
                .any(|p| tsz_solver::is_type_query_type(db, p.type_id)))
    {
        return true;
    }
    if let Some(shape) = tsz_solver::type_queries::get_callable_shape(db, type_id) {
        return shape.call_signatures.iter().any(|sig| {
            tsz_solver::is_type_query_type(db, sig.return_type)
                || sig
                    .params
                    .iter()
                    .any(|p| tsz_solver::is_type_query_type(db, p.type_id))
        });
    }
    false
}

/// `true` when `type_id` is an anonymous object type, or a union / intersection
/// that contains one (recursing through nested unions / intersections).
pub(crate) fn union_or_intersection_mentions_object(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::type_queries::union_or_intersection_mentions_object(db, type_id)
}

pub(crate) fn union_or_intersection_with_object(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    union_or_intersection_mentions_object(db, type_id)
        && !tsz_solver::type_queries::is_object_type(db, type_id)
}

/// Check whether an application's *declared* alias body is a mapped type
/// (e.g. `Partial<X>`, `Readonly<X>`, or `type F<T> = { [K in keyof T]... }`),
/// even when the concrete instantiation is fully resolved. Diagnostic
/// elaboration uses this to elaborate mapped-alias mismatches structurally
/// rather than via type-argument variance, matching tsc.
pub(crate) fn application_base_is_mapped_type<R: TypeResolver>(
    db: &dyn QueryDatabase,
    resolver: &R,
    type_id: TypeId,
) -> bool {
    tsz_solver::type_queries::application_base_is_mapped_type_db(db, resolver, type_id)
}

/// See [`tsz_solver::application_reduces_to_displayable_shape`]: the evaluated
/// shapes a reduced alias application renders structurally (tsc drops the
/// alias symbol), with the non-converged-recursion carve-out.
pub(crate) fn application_reduces_to_displayable_shape(
    db: &dyn TypeDatabase,
    evaluated: TypeId,
) -> bool {
    tsz_solver::application_reduces_to_displayable_shape(db, evaluated)
}

/// See [`tsz_solver::type_queries::application_distributes_over_union_check_arg`].
pub(crate) fn application_distributes_over_union_check_arg(
    db: &dyn TypeDatabase,
    definitions: &DefinitionStore,
    type_id: TypeId,
) -> bool {
    tsz_solver::type_queries::application_distributes_over_union_check_arg(db, definitions, type_id)
}

pub(crate) fn alias_application_body_reduces_through_conditional_or_indexed(
    db: &dyn TypeDatabase,
    definitions: &DefinitionStore,
    type_id: TypeId,
) -> bool {
    use tsz_solver::type_queries::ReducingAliasBodyKind;
    matches!(
        tsz_solver::type_queries::application_base_reducing_alias_body_kind(
            db,
            definitions,
            type_id
        ),
        Some(ReducingAliasBodyKind::Conditional | ReducingAliasBodyKind::IndexAccess)
    )
}

pub(crate) fn generic_deferred_source_keeps_spelling_against_generic_target(
    db: &dyn TypeDatabase,
    definitions: &DefinitionStore,
    source: TypeId,
    target: TypeId,
) -> bool {
    super::common::contains_type_parameters(db, source)
        && super::common::contains_type_parameters(db, target)
        && (super::common::is_conditional_type(db, source)
            || super::common::is_index_access_type(db, source)
            || alias_application_body_reduces_through_conditional_or_indexed(
                db,
                definitions,
                source,
            ))
}

/// See [`tsz_solver::type_queries::type_carries_alias_symbol_surface`].
pub(crate) fn type_keeps_alias_symbol_surface(
    db: &dyn TypeDatabase,
    definitions: &DefinitionStore,
    ty: TypeId,
) -> bool {
    tsz_solver::type_queries::type_carries_alias_symbol_surface(db, definitions, ty)
}

pub(crate) fn evaluated_alias_application_has_concrete_display(
    db: &dyn TypeDatabase,
    candidate: TypeId,
    evaluated: TypeId,
) -> bool {
    candidate != evaluated
        && evaluated != TypeId::ERROR
        && !super::common::is_conditional_type(db, evaluated)
        && !super::common::is_index_access_type(db, evaluated)
        && !super::common::contains_type_parameters(db, evaluated)
}

pub(crate) fn is_object_or_mapped_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_object_or_mapped_type(db, type_id)
}

pub(crate) fn is_typeof_result_union(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    const STRING: u8 = 1 << 0;
    const NUMBER: u8 = 1 << 1;
    const BIGINT: u8 = 1 << 2;
    const BOOLEAN: u8 = 1 << 3;
    const SYMBOL: u8 = 1 << 4;
    const UNDEFINED: u8 = 1 << 5;
    const OBJECT: u8 = 1 << 6;
    const FUNCTION: u8 = 1 << 7;
    const ALL: u8 = STRING | NUMBER | BIGINT | BOOLEAN | SYMBOL | UNDEFINED | OBJECT | FUNCTION;

    let Some(members) = tsz_solver::type_queries::get_union_members(db, type_id) else {
        return false;
    };
    if members.len() != 8 {
        return false;
    }

    let mut seen = 0u8;
    for member in members {
        let Some(atom) = tsz_solver::type_queries::get_string_literal_value(db, member) else {
            return false;
        };
        let bit = match db.resolve_atom_ref(atom).as_ref() {
            "string" => STRING,
            "number" => NUMBER,
            "bigint" => BIGINT,
            "boolean" => BOOLEAN,
            "symbol" => SYMBOL,
            "undefined" => UNDEFINED,
            "object" => OBJECT,
            "function" => FUNCTION,
            _ => return false,
        };
        seen |= bit;
    }

    seen == ALL
}

pub(crate) fn object_shape_for_assignment_numeric_display(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<tsz_solver::ObjectShape>> {
    tsz_solver::type_queries::object_shape_for_assignment_numeric_display(db, type_id)
}

pub(crate) fn is_global_object_interface_for_diagnostic(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::type_queries::is_global_interface_by_identity(
        db,
        type_id,
        tsz_solver::IntrinsicKind::Object,
    )
}

pub(crate) fn simple_intersection_head_for_this_assignment_display(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> Option<TypeId> {
    let members = super::common::intersection_members(db, type_id)?;
    let head = members.first().copied()?;
    if super::common::type_application(db, head).is_some() {
        return None;
    }
    if super::common::object_shape_for_type(db, head).is_some()
        && !super::common::type_has_displayable_name(db, head)
    {
        return None;
    }
    Some(head)
}

pub(crate) fn distinct_type_parameters_share_declared_name(
    db: &dyn tsz_solver::construction::TypeDatabase,
    source_param: TypeId,
    target_param: TypeId,
) -> bool {
    if source_param == target_param {
        return false;
    }
    let Some(source_info) = super::common::type_param_info(db, source_param) else {
        return false;
    };
    let Some(target_info) = super::common::type_param_info(db, target_param) else {
        return false;
    };
    source_info.name == target_info.name
}

pub(crate) fn distinct_types_share_nominal_diagnostic_name(
    db: &dyn tsz_solver::construction::TypeDatabase,
    binder: &tsz_binder::BinderState,
    def_store: &tsz_solver::def::DefinitionStore,
    source: TypeId,
    target: TypeId,
) -> bool {
    if source == target {
        return false;
    }
    let Some(source_name) = nominal_diagnostic_name(db, binder, def_store, source) else {
        return false;
    };
    nominal_diagnostic_name(db, binder, def_store, target).is_some_and(|target_name| {
        target_name == source_name && !is_primitive_diagnostic_name(&target_name)
    })
}

fn nominal_diagnostic_name(
    db: &dyn tsz_solver::construction::TypeDatabase,
    binder: &tsz_binder::BinderState,
    def_store: &tsz_solver::def::DefinitionStore,
    type_id: TypeId,
) -> Option<String> {
    if let Some(app) = type_application(db, type_id)
        && let Some(name) = nominal_diagnostic_name(db, binder, def_store, app.base)
    {
        return Some(name);
    }
    if let Some(alias) = db.get_display_alias(type_id)
        && alias != type_id
        && let Some(name) = nominal_diagnostic_name(db, binder, def_store, alias)
    {
        return Some(name);
    }
    if let Some(def_id) = lazy_def_id(db, type_id)
        && let Some(def) = def_store.get(def_id)
    {
        return Some(db.resolve_atom_ref(def.name).to_string());
    }
    let shape = object_shape_for_type(db, type_id)?;
    let symbol = binder.get_symbol(shape.symbol?)?;
    (!symbol.escaped_name.is_empty()).then(|| symbol.escaped_name.clone())
}

fn is_primitive_diagnostic_name(name: &str) -> bool {
    matches!(
        name,
        "any"
            | "unknown"
            | "never"
            | "string"
            | "number"
            | "boolean"
            | "symbol"
            | "bigint"
            | "void"
            | "undefined"
            | "null"
            | "object"
    )
}

pub(crate) fn number_literal_bits(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> Option<u64> {
    tsz_solver::type_queries::number_literal_bits(db, type_id)
}

pub(crate) fn is_number_literal_union(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::type_queries::is_number_literal_union(db, type_id)
}

pub(crate) fn numeric_literal_union_origin_preserves_alias(
    db: &dyn tsz_solver::construction::TypeDatabase,
    def_store: &tsz_solver::def::DefinitionStore,
    type_id: TypeId,
) -> bool {
    tsz_solver::type_queries::numeric_literal_union_origin_preserves_alias(db, def_store, type_id)
}

pub(crate) fn collect_property_name_atoms_for_diagnostics(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
    max_depth: usize,
) -> Vec<tsz_common::Atom> {
    tsz_solver::type_queries::collect_property_name_atoms_for_diagnostics(db, type_id, max_depth)
}

/// Collect property names accessible on a type for spelling suggestions.
///
/// For union types, only properties present in ALL members are returned (intersection).
pub(crate) fn collect_accessible_property_names_for_suggestion(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
    max_depth: usize,
) -> Vec<tsz_common::Atom> {
    if state_checking::union_members(db, type_id).is_none() {
        return collect_property_name_atoms_for_diagnostics(db, type_id, max_depth);
    }

    tsz_solver::type_queries::collect_accessible_property_names_for_suggestion(
        db, type_id, max_depth,
    )
}

pub(crate) fn function_shape(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<tsz_solver::FunctionShape>> {
    tsz_solver::type_queries::get_function_shape(db, type_id)
}

pub(crate) fn mapped_type(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> Option<(
    tsz_solver::MappedTypeId,
    std::sync::Arc<tsz_solver::MappedType>,
)> {
    tsz_solver::type_queries::get_mapped_type_with_id(db, type_id)
}

pub(crate) fn finite_mapped_property_surface(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> bool {
    let Some((mapped_id, mapped)) = tsz_solver::type_queries::get_mapped_type_with_id(db, type_id)
    else {
        return false;
    };
    if mapped_key_constraint_has_named_origin(db, mapped.constraint) {
        return false;
    }
    tsz_solver::type_queries::collect_finite_mapped_property_names(db, mapped_id).is_some()
}

fn mapped_key_constraint_has_named_origin(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> bool {
    if tsz_solver::type_queries::get_enum_def_id(db, type_id).is_some() {
        return true;
    }
    if tsz_solver::type_queries::get_lazy_def_id(db, type_id).is_some() {
        return true;
    }
    tsz_solver::type_queries::get_union_members(db, type_id).is_some_and(|members| {
        members
            .iter()
            .copied()
            .any(|member| mapped_key_constraint_has_named_origin(db, member))
    })
}

pub(crate) fn type_application(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<tsz_solver::TypeApplication>> {
    tsz_solver::type_queries::get_type_application(db, type_id)
}

pub(crate) fn same_non_class_nominal_application_surface<R: tsz_solver::resolver::TypeResolver>(
    db: &dyn tsz_solver::construction::TypeDatabase,
    resolver: &R,
    def_store: &tsz_solver::def::DefinitionStore,
    source_candidates: &[TypeId],
    target_candidates: &[TypeId],
) -> bool {
    source_candidates.iter().any(|&source_candidate| {
        let Some(source) = non_class_nominal_application_surface(db, def_store, source_candidate)
        else {
            return false;
        };

        target_candidates
            .iter()
            .filter_map(|&candidate| {
                non_class_nominal_application_surface(db, def_store, candidate)
            })
            .any(|target| nominal_application_surfaces_match(db, resolver, &source, &target))
    })
}

struct NominalApplicationSurface {
    def_id: tsz_solver::DefId,
    args: Vec<TypeId>,
}

fn nominal_application_surfaces_match<R: tsz_solver::resolver::TypeResolver>(
    db: &dyn tsz_solver::construction::TypeDatabase,
    resolver: &R,
    source: &NominalApplicationSurface,
    target: &NominalApplicationSurface,
) -> bool {
    source.def_id == target.def_id
        && source.args.len() == target.args.len()
        && source
            .args
            .iter()
            .zip(&target.args)
            .all(|(&source, &target)| {
                tsz_solver::relations::subtype::are_types_structurally_identical(
                    db, resolver, source, target,
                )
            })
}

fn non_class_nominal_application_surface(
    db: &dyn tsz_solver::construction::TypeDatabase,
    def_store: &tsz_solver::def::DefinitionStore,
    type_id: TypeId,
) -> Option<NominalApplicationSurface> {
    if is_type_query_surface(db, type_id) {
        return None;
    }

    let app = type_application(db, type_id).or_else(|| {
        db.get_display_alias(type_id)
            .filter(|&alias| !is_type_query_surface(db, alias))
            .and_then(|alias| type_application(db, alias))
    })?;
    if app.args.is_empty() || is_type_query_surface(db, app.base) {
        return None;
    }

    let def_id = lazy_def_id(db, app.base)?;
    let def = def_store.get(def_id)?;
    (!matches!(
        def.kind,
        tsz_solver::def::DefKind::Class | tsz_solver::def::DefKind::ClassConstructor
    ))
    .then(|| NominalApplicationSurface {
        def_id,
        args: app.args.clone(),
    })
}

fn is_type_query_surface(db: &dyn tsz_solver::construction::TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::is_type_query_type(db, type_id)
        || db
            .get_display_alias(type_id)
            .is_some_and(|alias| tsz_solver::is_type_query_type(db, alias))
}

pub(crate) fn is_index_access_type(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::type_queries::is_index_access_type(db, type_id)
}

pub(crate) fn contains_index_access_type(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::type_queries::contains_index_access_type(db, type_id)
}

pub(crate) fn contains_never_index_access_surface(
    db: &dyn tsz_solver::construction::TypeDatabase,
    def_store: &tsz_solver::def::DefinitionStore,
    type_id: TypeId,
    max_depth: usize,
) -> bool {
    tsz_solver::type_queries::contains_never_index_access_surface(db, def_store, type_id, max_depth)
}

pub(crate) fn application_base_has_conditional_alias_body(
    db: &dyn tsz_solver::construction::TypeDatabase,
    def_store: &tsz_solver::def::DefinitionStore,
    type_id: TypeId,
) -> bool {
    tsz_solver::type_queries::application_base_has_conditional_alias_body(db, def_store, type_id)
}

pub(crate) fn preserves_named_application_base(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::type_queries::get_lazy_def_id(db, type_id).is_some()
        || !matches!(
            tsz_solver::type_queries::classify_type_query(db, type_id),
            tsz_solver::type_queries::TypeQueryKind::Other
        )
}

// ── Display widening / reduction ──
//
// These helpers encode diagnostic *display* policy: how a semantic type is
// widened or reduced purely so the printer renders the form `tsc` shows in a
// message. They are domain-owned here (not in `query_boundaries::common`) per
// the boundary ratchet — display rendering is a `diagnostics` concern.

/// Deeply reduce meta-type applications (e.g. `InstanceType<typeof Foo>`)
/// that appear inside `type_id` so the solver's type printer renders the
/// concrete form that `tsc` shows in heritage diagnostics. The generic
/// `TypeEvaluator` only visits the top-level node; this boundary helper
/// walks composite wrappers (`Intersection`, `Union`, `Object`) and
/// evaluates the inner `Application` / `Conditional` leaves using the
/// caller-supplied `TypeResolver`.
pub(crate) fn deep_reduce_for_display<R: TypeResolver>(
    db: &dyn TypeDatabase,
    resolver: &R,
    type_id: TypeId,
) -> TypeId {
    tsz_solver::deep_reduce_for_display(db, resolver, type_id)
}

/// Normalize the property order of an object shape for diagnostic display so
/// rendered members match `tsc`'s ordering.
pub(crate) fn normalize_display_property_order(props: &mut [tsz_solver::PropertyInfo]) {
    tsz_solver::normalize_display_property_order(props)
}

/// Widen a type for call-argument diagnostic display: widens boolean
/// literals inside compound shapes (tuples/objects) so TS2345 source-type
/// renders match tsc, e.g. `[number, number, boolean, boolean]` instead of
/// `[number, number, false, true]`. Function param types are still skipped.
pub(crate) fn widen_argument_type_for_display(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::operations::widening::widen_argument_type_for_display(db, type_id)
}

/// Generalize a fresh literal assignment/argument `source` to its base type for
/// diagnostic display, mirroring tsc's `reportRelationError`: a literal source
/// is widened to its base (`true` -> `boolean`, `1` -> `number`) when the
/// `target` could not hold a top-level singleton type, and preserved otherwise
/// (`true` vs `1`, `"a"` vs `"b"`). Non-literal sources are returned unchanged.
pub(crate) fn generalized_literal_source_for_display(
    db: &dyn TypeDatabase,
    source: TypeId,
    target: TypeId,
) -> TypeId {
    if tsz_solver::type_queries::is_literal_type(db, source)
        && !tsz_solver::type_queries::type_could_have_top_level_singleton_types(db, target)
    {
        tsz_solver::operations::widening::widen_type(db, source)
    } else {
        source
    }
}

/// Apparent type of an element-access receiver for the implicit-any index
/// diagnostic (`TS7053`). `tsc` renders `typeToString(getApparentType(objectType))`,
/// so the `object` intrinsic prints as its apparent type `{}` and a bare
/// primitive as its boxed wrapper interface (`string` -> `String`, ...); every
/// other type is the identity, letting the caller keep its annotation-aware
/// display path.
pub(crate) fn index_receiver_apparent_type(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::objects::index_receiver_apparent_type(db, type_id)
}

/// Display the boolean-literal-array element form (`boolean[]` rather than the
/// `(true | false)[]` fresh form) for argument-mismatch diagnostics.
pub(crate) fn boolean_literal_array_display_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<TypeId> {
    tsz_solver::type_queries::boolean_literal_array_display_type(db, type_id)
}

/// Recursively reduce a type to its base constraint for display purposes.
///
/// Handles type parameters, intersections, and unions: for an intersection
/// like `T & U` where the members have constraints, returns the intersection
/// of the constraints (further simplified via the interner). This matches
/// tsc's `getBaseConstraintOfType` for instantiable intersections and is used
/// in error messages to display the reduced form instead of the raw generic
/// intersection.
pub(crate) fn get_base_constraint_for_display(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::type_queries::get_base_constraint_for_display(db, type_id)
}

/// Display-widen a type for TS2403 redeclaration messages.
///
/// Thin boundary wrapper over `tsz_solver::operations::widening::display_widen_for_redeclaration`.
/// See the solver definition for semantics — preserves top-level literal /
/// literal-union types while deep-widening fresh literals nested inside
/// compound shapes.
pub(crate) fn display_widen_for_redeclaration(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::operations::widening::display_widen_for_redeclaration(db, type_id)
}

/// Policy selecting which literal annotation kinds
/// [`widen_object_property_literals_for_display`] rewrites.
pub(crate) use tsz_solver::operations::widening::AnnotationLiteralWideningPolicy;
/// Outcome of [`widen_object_property_literals_for_display`]: the widened
/// `TypeId` plus whether literal spellings remain in display provenance that
/// only a display-property-free formatter can hide.
pub(crate) use tsz_solver::operations::widening::AnnotationWideningOutcome;

/// Widen literal types in annotation positions (object property types, method
/// return types, function parameter annotations, index-signature value types,
/// labeled tuple elements) for diagnostic display, then let the caller reprint
/// the result.
///
/// Type-level replacement for the checker's former byte-walking display
/// rewriters (issue #13075): widening happens on the `TypeId` through the
/// solver's widening operations and the formatter prints the widened type
/// once. Top-level literals, bare union/intersection members, unlabeled tuple
/// elements, and non-method function return types are preserved, matching the
/// positions a `": <literal>"` text rewrite could reach.
pub(crate) fn widen_object_property_literals_for_display(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    policy: AnnotationLiteralWideningPolicy,
) -> AnnotationWideningOutcome {
    tsz_solver::operations::widening::widen_annotation_literals_for_display(db, type_id, policy)
}

/// Like [`widen_object_property_literals_for_display`], with a resolver so
/// leading annotation positions held by generic applications that evaluate
/// to literals (and render as such) widen too.
pub(crate) fn widen_object_property_literals_for_display_resolved<R: TypeResolver>(
    db: &dyn TypeDatabase,
    resolver: &R,
    type_id: TypeId,
    policy: AnnotationLiteralWideningPolicy,
) -> AnnotationWideningOutcome {
    tsz_solver::operations::widening::widen_annotation_literals_for_display_resolved(
        db, resolver, type_id, policy,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsz_solver::construction::TypeInterner;
    use tsz_solver::def::{DefinitionInfo, DefinitionStore};
    use tsz_solver::{PropertyInfo, SymbolRef, TypeParamInfo};

    fn register_interface_base(db: &TypeInterner, store: &DefinitionStore, name: &str) -> TypeId {
        let def_id = store.register(DefinitionInfo::interface(
            db.intern_string(name),
            vec![TypeParamInfo::simple(db.intern_string("T"))],
            vec![PropertyInfo::new(db.intern_string("value"), TypeId::STRING)],
        ));
        db.lazy(def_id)
    }

    #[test]
    fn non_class_nominal_application_surface_matches_by_def_id_for_renamed_interfaces() {
        for name in ["Carrier", "RenamedCarrier"] {
            let db = TypeInterner::new();
            let store = DefinitionStore::new();
            let base = register_interface_base(&db, &store, name);
            let source = db.application(base, vec![TypeId::STRING]);
            let target = db.application(base, vec![TypeId::STRING]);

            assert!(
                same_non_class_nominal_application_surface(&db, &db, &store, &[source], &[target],),
                "same interface application surface should match structurally for {name}"
            );
        }
    }

    #[test]
    fn non_class_nominal_application_surface_rejects_different_type_args() {
        let db = TypeInterner::new();
        let store = DefinitionStore::new();
        let base = register_interface_base(&db, &store, "Carrier");
        let source = db.application(base, vec![TypeId::STRING]);
        let target = db.application(base, vec![TypeId::NUMBER]);

        assert!(
            !same_non_class_nominal_application_surface(&db, &db, &store, &[source], &[target]),
            "same generic base with different type arguments must not suppress TS2345"
        );
    }

    #[test]
    fn class_and_type_query_application_surfaces_do_not_match() {
        let db = TypeInterner::new();
        let store = DefinitionStore::new();
        let class_def = store.register(DefinitionInfo::class(
            db.intern_string("Box"),
            vec![TypeParamInfo::simple(db.intern_string("T"))],
            vec![PropertyInfo::new(db.intern_string("value"), TypeId::STRING)],
            vec![],
        ));
        let class_app = db.application(db.lazy(class_def), vec![TypeId::STRING]);
        assert!(!same_non_class_nominal_application_surface(
            &db,
            &db,
            &store,
            &[class_app],
            &[class_app]
        ));

        let query_app = db.application(db.type_query(SymbolRef(7)), vec![TypeId::STRING]);
        assert!(!same_non_class_nominal_application_surface(
            &db,
            &db,
            &store,
            &[query_app],
            &[query_app]
        ));
    }
}
