use tsz_binder::SymbolId;
use tsz_common::Atom;
use tsz_solver::TypeId;
use tsz_solver::construction::{QueryDatabase, TypeDatabase};
use tsz_solver::{
    CallSignature, CallableShape, FunctionShape, ParamInfo, PropertyInfo, TypeParamInfo, Visibility,
};

pub(crate) use super::super::common::{callable_shape_for_type, intersection_members, lazy_def_id};
pub(crate) use tsz_solver::type_queries::{
    AbstractClassCheckKind, CallSignaturesKind, ClassDeclTypeKind, LazyTypeKind,
};

pub(crate) fn classify_for_abstract_check(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> AbstractClassCheckKind {
    tsz_solver::type_queries::classify_for_abstract_check(db, type_id)
}

pub(crate) fn classify_for_lazy_resolution(db: &dyn TypeDatabase, type_id: TypeId) -> LazyTypeKind {
    tsz_solver::type_queries::classify_for_lazy_resolution(db, type_id)
}

pub(crate) fn type_parameter_info(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::TypeParamInfo> {
    tsz_solver::type_queries::get_type_parameter_info(db, type_id)
}

pub(crate) fn get_application_info(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<(TypeId, Vec<TypeId>)> {
    tsz_solver::type_queries::get_application_info(db, type_id)
}

pub(crate) fn application_infos_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Vec<(TypeId, Vec<TypeId>)> {
    let mut applications = Vec::with_capacity(2);
    if let Some(app) = get_application_info(db, type_id) {
        applications.push(app);
    }
    if let Some(alias_app) = db
        .get_display_alias(type_id)
        .and_then(|alias| get_application_info(db, alias))
        && !applications.contains(&alias_app)
    {
        applications.push(alias_app);
    }
    applications
}

/// Explicit `new D<T>()` display aliases are only needed when the return type is
/// still a bare, alias-free reference. Generic construct signatures already
/// return an applied type, and wrapping those again double-prints type args.
pub(crate) fn should_synthesize_explicit_new_display_alias(
    db: &dyn TypeDatabase,
    return_type: TypeId,
) -> bool {
    lazy_def_id(db, return_type).is_none()
        && db.get_display_alias(return_type).is_none()
        && !tsz_solver::query::is_generic_application(db, return_type)
}

pub(crate) fn instantiate_type_params_to_constraints(
    db: &dyn QueryDatabase,
    type_id: TypeId,
) -> TypeId {
    tsz_solver::computation::instantiate_type_params_to_constraints(db, type_id)
}

pub(crate) fn get_function_shape(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<tsz_solver::FunctionShape>> {
    tsz_solver::type_queries::get_function_shape(db, type_id)
}

pub(crate) use get_function_shape as function_shape_for_type;

pub(crate) fn classify_for_class_decl(db: &dyn TypeDatabase, type_id: TypeId) -> ClassDeclTypeKind {
    tsz_solver::type_queries::classify_for_class_decl(db, type_id)
}

pub(crate) fn classify_for_call_signatures(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> CallSignaturesKind {
    tsz_solver::type_queries::classify_for_call_signatures(db, type_id)
}

pub(crate) fn shallow_js_method_callable_type(
    db: &dyn TypeDatabase,
    type_params: Vec<TypeParamInfo>,
    params: Vec<ParamInfo>,
    return_type: TypeId,
) -> TypeId {
    db.callable(CallableShape {
        call_signatures: vec![CallSignature {
            type_params,
            params,
            this_type: None,
            return_type,
            type_predicate: None,
            is_method: true,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    })
}

pub(crate) fn constructor_shape_with_mapped_parameter_types(
    shape: &FunctionShape,
    mut map_type: impl FnMut(TypeId) -> TypeId,
) -> FunctionShape {
    let params = shape
        .params
        .iter()
        .map(|param| ParamInfo {
            suppress_display_optional: false,
            type_id: map_type(param.type_id),
            ..*param
        })
        .collect();
    FunctionShape {
        params,
        return_type: shape.return_type,
        this_type: shape.this_type,
        type_params: shape.type_params.clone(),
        type_predicate: shape.type_predicate,
        is_constructor: shape.is_constructor,
        is_method: shape.is_method,
    }
}

pub(crate) fn constructor_contextual_promise_union(
    db: &dyn TypeDatabase,
    inner: TypeId,
    promise_like: TypeId,
    promise: Option<TypeId>,
) -> TypeId {
    let mut members = vec![inner, promise_like];
    if let Some(promise) = promise {
        members.push(promise);
    }
    db.union(members)
}

pub(crate) fn constructor_promise_resolve_value_union(
    db: &dyn TypeDatabase,
    inner: TypeId,
    promise_like: TypeId,
) -> TypeId {
    db.union2(inner, promise_like)
}

pub(crate) fn typed_array_length_constructor_return_application(
    db: &dyn TypeDatabase,
    base: TypeId,
    array_buffer: TypeId,
) -> TypeId {
    db.application(base, vec![array_buffer])
}

pub(crate) fn record_explicit_new_display_alias(
    db: &dyn TypeDatabase,
    return_type: TypeId,
    application: TypeId,
) {
    db.store_display_alias(return_type, application);
}

pub(crate) fn record_synthetic_explicit_new_display_alias(
    db: &dyn TypeDatabase,
    return_type: TypeId,
    type_args: Vec<TypeId>,
) {
    let application = db.application(return_type, type_args);
    db.store_display_alias(return_type, application);
}

pub(crate) fn evaluated_intersection_members(
    db: &dyn TypeDatabase,
    members: Vec<TypeId>,
) -> TypeId {
    db.intersection(members)
}

pub(crate) const fn js_surface_property(
    name: Atom,
    type_id: TypeId,
    parent_id: Option<SymbolId>,
    is_method: bool,
) -> PropertyInfo {
    PropertyInfo {
        name,
        type_id,
        write_type: type_id,
        optional: false,
        readonly: false,
        is_method,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
        non_widening: false,
    }
}

pub(crate) fn js_instance_object_with_symbol(
    db: &dyn TypeDatabase,
    properties: Vec<PropertyInfo>,
    symbol: Option<SymbolId>,
) -> TypeId {
    db.object_with_flags_and_symbol(properties, tsz_solver::ObjectFlags::empty(), symbol)
}

pub(crate) fn is_readonly_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_readonly_type(db, type_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsz_solver::PropertyInfo;
    use tsz_solver::construction::TypeInterner;

    fn fresh_object(db: &TypeInterner, name: &str, ty: TypeId) -> TypeId {
        db.object_fresh(vec![PropertyInfo::new(db.intern_string(name), ty)])
    }

    #[test]
    fn application_infos_for_type_returns_direct_application() {
        let db = TypeInterner::new();
        let app = db.application(TypeId::STRING, vec![TypeId::NUMBER]);

        let applications = application_infos_for_type(&db, app);

        assert_eq!(applications, vec![(TypeId::STRING, vec![TypeId::NUMBER])]);
    }

    #[test]
    fn application_infos_for_type_returns_display_alias_application() {
        let db = TypeInterner::new();
        let evaluated = fresh_object(&db, "value", TypeId::NUMBER);
        let alias_app = db.application(TypeId::STRING, vec![TypeId::NUMBER]);
        db.store_display_alias(evaluated, alias_app);

        let applications = application_infos_for_type(&db, evaluated);

        assert_eq!(applications, vec![(TypeId::STRING, vec![TypeId::NUMBER])]);
    }

    #[test]
    fn application_infos_for_type_includes_direct_and_distinct_alias_application() {
        let db = TypeInterner::new();
        let direct_app = db.application(TypeId::STRING, vec![TypeId::NUMBER]);
        let alias_app = db.application(TypeId::NUMBER, vec![TypeId::STRING]);
        db.store_display_alias(direct_app, alias_app);

        let applications = application_infos_for_type(&db, direct_app);

        assert_eq!(
            applications,
            vec![
                (TypeId::STRING, vec![TypeId::NUMBER]),
                (TypeId::NUMBER, vec![TypeId::STRING]),
            ]
        );
    }
}
