use tsz_solver::{TypeDatabase, TypeId};

pub(crate) use tsz_solver::type_queries::LiteralTypeKind;

pub(crate) fn classify_literal_type(db: &dyn TypeDatabase, type_id: TypeId) -> LiteralTypeKind {
    tsz_solver::type_queries::classify_literal_type(db, type_id)
}

pub(crate) fn is_callable_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_callable_type(db, type_id)
}

pub(crate) fn is_function_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_function_type(db, type_id)
}
