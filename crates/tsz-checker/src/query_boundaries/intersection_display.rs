use tsz_solver::construction::TypeDatabase;
use tsz_solver::relations::subtype::TypeResolver;
use tsz_solver::{ObjectShape, TypeId};

pub(crate) use tsz_solver::objects::PropertyCollectionResult;

pub(crate) fn collect_properties<R: TypeResolver>(
    type_id: TypeId,
    db: &dyn TypeDatabase,
    resolver: &R,
) -> PropertyCollectionResult {
    tsz_solver::objects::collect_properties(type_id, db, resolver)
}

pub(crate) fn collected_properties_object_type<R: TypeResolver>(
    db: &dyn TypeDatabase,
    resolver: &R,
    type_id: TypeId,
) -> Option<TypeId> {
    match collect_properties(type_id, db, resolver) {
        PropertyCollectionResult::Properties {
            properties,
            string_index,
            number_index,
            symbol_index: _,
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
