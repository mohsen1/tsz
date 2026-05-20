use tsz_solver::TypeId;
use tsz_solver::construction::{QueryDatabase, TypeDatabase};

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

pub(crate) fn is_readonly_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_readonly_type(db, type_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsz_solver::{PropertyInfo, TypeInterner};

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
