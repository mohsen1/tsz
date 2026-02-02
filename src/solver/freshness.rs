//! Freshness helpers for object literal excess property checking.

use crate::solver::types::ObjectFlags;
use crate::solver::visitor::{ObjectTypeKind, classify_object_type};
use crate::solver::{TypeDatabase, TypeId};

pub fn is_fresh_object_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    match classify_object_type(db, type_id) {
        ObjectTypeKind::Object(shape_id) | ObjectTypeKind::ObjectWithIndex(shape_id) => {
            let shape = db.object_shape(shape_id);
            shape.flags.contains(ObjectFlags::FRESH_LITERAL)
        }
        ObjectTypeKind::NotObject => false,
    }
}

pub fn widen_freshness(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    let (shape_id, has_index) = match classify_object_type(db, type_id) {
        ObjectTypeKind::Object(shape_id) => (shape_id, false),
        ObjectTypeKind::ObjectWithIndex(shape_id) => (shape_id, true),
        ObjectTypeKind::NotObject => return type_id,
    };

    let shape = db.object_shape(shape_id);
    if !shape.flags.contains(ObjectFlags::FRESH_LITERAL) {
        return type_id;
    }

    let mut new_shape = (*shape).clone();
    new_shape.flags.remove(ObjectFlags::FRESH_LITERAL);

    if has_index || new_shape.string_index.is_some() || new_shape.number_index.is_some() {
        db.object_with_index(new_shape)
    } else {
        db.object_with_flags(new_shape.properties, new_shape.flags)
    }
}
