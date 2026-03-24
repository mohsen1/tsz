//! Freshness helpers for object literal excess property checking.

use crate::types::ObjectFlags;
use crate::visitor::{ObjectTypeKind, classify_object_type};
use crate::{TypeDatabase, TypeId};

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
    widen_freshness_deep(db, type_id, 0)
}

/// Deeply widen freshness on an object type and all its property types.
/// This matches TSC's `getRegularTypeOfObjectLiteral` which recursively removes
/// freshness from nested object literal types.
///
/// Unlike the previous implementation which only recursed into fresh objects,
/// this version walks into ALL object types to find and widen nested fresh
/// property types. This is necessary because generic inference can produce
/// non-fresh outer objects whose property types are still fresh (e.g., when
/// inferring through `Readonly<T>`).
fn widen_freshness_deep(db: &dyn TypeDatabase, type_id: TypeId, depth: u32) -> TypeId {
    // Guard against infinite recursion in cyclic types.
    if depth > 10 {
        return type_id;
    }

    let (shape_id, has_index) = match classify_object_type(db, type_id) {
        ObjectTypeKind::Object(shape_id) => (shape_id, false),
        ObjectTypeKind::ObjectWithIndex(shape_id) => (shape_id, true),
        ObjectTypeKind::NotObject => return type_id,
    };

    let shape = db.object_shape(shape_id);
    let is_fresh = shape.flags.contains(ObjectFlags::FRESH_LITERAL);

    // Check if any property types need freshness widening.
    let any_fresh_props = shape.properties.iter().any(|prop| {
        is_fresh_object_type(db, prop.type_id)
            || (prop.write_type != TypeId::UNDEFINED && is_fresh_object_type(db, prop.write_type))
    });

    // If this object is not fresh and no properties are fresh, nothing to do.
    if !is_fresh && !any_fresh_props {
        return type_id;
    }

    let mut new_shape = (*shape).clone();

    if is_fresh {
        new_shape.flags.remove(ObjectFlags::FRESH_LITERAL);
    }

    // Recursively widen freshness on property types.
    for prop in &mut new_shape.properties {
        prop.type_id = widen_freshness_deep(db, prop.type_id, depth + 1);
        if prop.write_type != TypeId::UNDEFINED && prop.write_type != prop.type_id {
            prop.write_type = widen_freshness_deep(db, prop.write_type, depth + 1);
        }
    }

    let new_type =
        if has_index || new_shape.string_index.is_some() || new_shape.number_index.is_some() {
            db.object_with_index(new_shape)
        } else {
            db.object_with_flags(new_shape.properties, new_shape.flags)
        };

    // Carry forward display properties from the fresh TypeId to the widened TypeId.
    if let Some(display_props) = db.get_display_properties(type_id) {
        db.store_display_properties(new_type, display_props.as_ref().clone());
    }

    new_type
}
