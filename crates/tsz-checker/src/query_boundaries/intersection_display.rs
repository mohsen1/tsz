use std::sync::Arc;
use tsz_solver::construction::TypeDatabase;
use tsz_solver::relations::subtype::TypeResolver;
use tsz_solver::{ObjectShape, TypeId};

pub(crate) use tsz_solver::objects::PropertyCollectionResult;

/// The property name whose required occurrences across a *written*
/// intersection's members are literal values from mutually exclusive
/// value-sets, forcing that intersection to reduce to `never` (`tsc`'s
/// TS18031). `members` is the pre-reduction member list recovered from
/// source syntax — see `declared_intersection_annotation_display_for_expression`
/// — since the interned intersection has already collapsed to the single
/// canonical `TypeId::NEVER` by the time a property-access failure is
/// reported against it.
pub(crate) fn find_disjoint_literal_property_across_intersection(
    db: &dyn TypeDatabase,
    members: &[TypeId],
) -> Option<tsz_common::interner::Atom> {
    tsz_solver::type_queries::find_disjoint_literal_property_across_intersection(db, members)
}

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
            symbol_index,
        } if !properties.is_empty()
            || string_index.is_some()
            || number_index.is_some()
            || symbol_index.is_some() =>
        {
            if string_index.is_some() || number_index.is_some() || symbol_index.is_some() {
                Some(db.object_with_index(ObjectShape {
                    properties,
                    string_index,
                    number_index,
                    symbol_index,
                    ..ObjectShape::default()
                }))
            } else {
                Some(db.object(properties))
            }
        }
        _ => None,
    }
}

pub(crate) fn collected_properties_object_shape<R: TypeResolver>(
    db: &dyn TypeDatabase,
    resolver: &R,
    type_id: TypeId,
) -> Option<Arc<ObjectShape>> {
    match collect_properties(type_id, db, resolver) {
        PropertyCollectionResult::Properties {
            properties,
            string_index,
            number_index,
            symbol_index,
        } => Some(Arc::new(ObjectShape {
            properties,
            string_index,
            number_index,
            symbol_index,
            ..ObjectShape::default()
        })),
        _ => None,
    }
}
