use tsz_common::Atom;
use tsz_solver::construction::TypeDatabase;
use tsz_solver::{FunctionShape, ParamInfo, TypeId, TypeParamInfo, TypeParamOrigin};

pub(crate) use super::common::{
    callable_shape_for_type, has_construct_signatures, is_symbol_or_unique_symbol, union_members,
};
pub(crate) use tsz_solver::type_queries::ConstructorCheckKind;

pub(crate) fn classify_for_constructor_check(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> ConstructorCheckKind {
    tsz_solver::type_queries::classify_for_constructor_check(db, type_id)
}

pub(crate) fn has_function_shape(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::get_function_shape(db, type_id).is_some()
}

pub(crate) fn is_constructor_function_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::get_function_shape(db, type_id)
        .is_some_and(|shape| shape.is_constructor)
}

/// True when `type_id` is a surface type constructor over the canonical
/// polymorphic `this`, such as `this[]`, `readonly this[]`, `this | undefined`,
/// or `Foo & this`.
///
/// This deliberately walks only constructor surfaces. It does not inspect object
/// members, lazy class/interface bodies, or type-parameter constraints, because
/// those can mention `this` without making the receiver itself a `this`-relative
/// wrapper.
pub(crate) fn is_compound_this_relative_surface_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    this_type: TypeId,
) -> bool {
    tsz_solver::type_queries::is_compound_this_relative_surface_type(db, type_id, this_type)
}

pub(crate) fn type_checking_union(db: &dyn TypeDatabase, members: Vec<TypeId>) -> TypeId {
    db.union(members)
}

pub(crate) fn type_checking_index_access(
    db: &dyn TypeDatabase,
    object_type: TypeId,
    index_type: TypeId,
) -> TypeId {
    db.index_access(object_type, index_type)
}

pub(crate) fn type_checking_literal_number(db: &dyn TypeDatabase, value: f64) -> TypeId {
    db.literal_number(value)
}

pub(crate) const fn user_type_param_info(
    name: Atom,
    constraint: Option<TypeId>,
    default: Option<TypeId>,
    is_const: bool,
) -> TypeParamInfo {
    TypeParamInfo {
        name,
        constraint,
        default,
        is_const,
        origin: TypeParamOrigin::User,
    }
}

pub(crate) fn user_type_param(
    db: &dyn TypeDatabase,
    name: Atom,
    constraint: Option<TypeId>,
    default: Option<TypeId>,
    is_const: bool,
) -> TypeId {
    db.type_param(user_type_param_info(name, constraint, default, is_const))
}

pub(crate) const fn param_info(
    name: Option<Atom>,
    type_id: TypeId,
    optional: bool,
    rest: bool,
) -> ParamInfo {
    ParamInfo {
        name,
        type_id,
        optional,
        rest,
        arity_only_optional: false,
    }
}

pub(crate) fn global_function_fallback_type(db: &dyn TypeDatabase, args_atom: Atom) -> TypeId {
    let rest_param = param_info(Some(args_atom), TypeId::ANY, false, true);
    db.function(FunctionShape {
        params: vec![rest_param],
        this_type: None,
        return_type: TypeId::ANY,
        type_params: vec![],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    })
}

pub(crate) fn method_function_type(
    db: &dyn TypeDatabase,
    type_params: Vec<TypeParamInfo>,
    params: Vec<ParamInfo>,
    return_type: TypeId,
) -> TypeId {
    db.function(FunctionShape {
        type_params,
        params,
        this_type: None,
        return_type,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsz_solver::construction::TypeInterner;

    #[test]
    fn constructs_type_checking_surfaces() {
        let db = TypeInterner::new();
        let name = db.intern_string("T");
        let type_param = user_type_param(&db, name, Some(TypeId::STRING), None, true);

        assert_eq!(
            type_param,
            db.type_param(TypeParamInfo {
                name,
                constraint: Some(TypeId::STRING),
                default: None,
                is_const: true,
                origin: TypeParamOrigin::User,
            })
        );
        assert_eq!(
            type_checking_union(&db, vec![TypeId::STRING, TypeId::NUMBER]),
            db.union(vec![TypeId::STRING, TypeId::NUMBER])
        );
        assert_eq!(
            type_checking_index_access(&db, TypeId::STRING, TypeId::NUMBER),
            db.index_access(TypeId::STRING, TypeId::NUMBER)
        );
        assert_eq!(
            type_checking_literal_number(&db, 1.0),
            db.literal_number(1.0)
        );

        let param = param_info(
            Some(db.intern_string("value")),
            TypeId::BOOLEAN,
            true,
            false,
        );
        assert_eq!(param.type_id, TypeId::BOOLEAN);
        assert!(param.optional);
        assert!(!param.rest);

        let global_function = global_function_fallback_type(&db, db.intern_string("args"));
        assert!(has_function_shape(&db, global_function));

        let method = method_function_type(&db, vec![], vec![param], TypeId::NUMBER);
        let shape = tsz_solver::type_queries::get_function_shape(&db, method)
            .expect("method function should have shape");
        assert!(shape.is_method);
        assert_eq!(shape.return_type, TypeId::NUMBER);
    }
}
