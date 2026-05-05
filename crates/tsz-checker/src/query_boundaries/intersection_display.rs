use tsz_solver::{ObjectShape, TypeDatabase, TypeId, TypeResolver};

pub(crate) fn collected_properties_object_type<R: TypeResolver>(
    db: &dyn TypeDatabase,
    resolver: &R,
    type_id: TypeId,
) -> Option<TypeId> {
    match tsz_solver::collect_properties(type_id, db, resolver) {
        tsz_solver::PropertyCollectionResult::Properties {
            properties,
            string_index,
            number_index,
        } if !properties.is_empty() || string_index.is_some() || number_index.is_some() => {
            if string_index.is_some() || number_index.is_some() {
                Some(db.object_with_index(ObjectShape {
                    properties,
                    string_index,
                    number_index,
                    ..ObjectShape::default()
                }))
            } else {
                Some(db.object(properties))
            }
        }
        _ => None,
    }
}
