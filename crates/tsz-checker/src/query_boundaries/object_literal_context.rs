use tsz_solver::TypeId;
use tsz_solver::construction::TypeDatabase;

pub(crate) fn contextual_union_preserve_members(
    db: &dyn TypeDatabase,
    members: Vec<TypeId>,
) -> TypeId {
    db.union_preserve_members(members)
}

pub(crate) fn contextual_intersection(db: &dyn TypeDatabase, members: Vec<TypeId>) -> TypeId {
    db.intersection(members)
}

pub(crate) fn mapped_contextual_property_number_key_type(
    db: &dyn TypeDatabase,
    value: f64,
) -> TypeId {
    db.literal_number(value)
}

pub(crate) fn mapped_contextual_property_string_key_type(
    db: &dyn TypeDatabase,
    value: &str,
) -> TypeId {
    db.literal_string(value)
}
