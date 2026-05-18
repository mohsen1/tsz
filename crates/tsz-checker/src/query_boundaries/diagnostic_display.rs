//! Query helpers used by diagnostic type-display policy.

use tsz_solver::{ObjectShape, TypeDatabase, TypeId};

use super::common;

/// Widen object property literal types for use in a diagnostic display context.
///
/// Fresh object literals can carry literal-valued display properties while their
/// structural shape is already widened. This boundary helper widens whichever
/// object layer the formatter would use, and recursively widens nested anonymous
/// object shapes up to a fixed depth.
pub(crate) fn widen_object_properties_for_diagnostic_display(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> TypeId {
    widen_object_properties_for_diagnostic_display_depth(db, type_id, 0)
}

fn widen_object_properties_for_diagnostic_display_depth(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    depth: usize,
) -> TypeId {
    if depth > 8 {
        return type_id;
    }

    let display_props = db.get_display_properties(type_id);
    let Some(shape) = object_shape_for_type(db, type_id) else {
        return type_id;
    };

    if let Some(display_props) = display_props {
        let mut new_shape = shape.as_ref().clone();
        new_shape.properties = display_props.as_ref().clone();
        let mut changed = true;
        changed |= widen_props_for_diagnostic_display(db, &mut new_shape.properties, depth);
        changed |= widen_indexes_for_diagnostic_display(db, &mut new_shape, depth);
        return if changed {
            db.object_with_index(new_shape)
        } else {
            type_id
        };
    }

    let should_widen = shape.is_fresh_literal()
        || (shape.symbol.is_none() && db.get_display_alias(type_id).is_none());
    if !should_widen {
        return type_id;
    }

    let mut widened_shape = shape.as_ref().clone();
    let changed = widen_props_for_diagnostic_display(db, &mut widened_shape.properties, depth)
        | widen_indexes_for_diagnostic_display(db, &mut widened_shape, depth);
    if changed {
        db.object_with_index(widened_shape)
    } else {
        type_id
    }
}

fn object_shape_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<ObjectShape>> {
    tsz_solver::type_queries::get_object_shape(db, type_id)
}

fn widen_props_for_diagnostic_display(
    db: &dyn TypeDatabase,
    props: &mut [tsz_solver::PropertyInfo],
    depth: usize,
) -> bool {
    let mut changed = false;
    for prop in props.iter_mut() {
        let read = widen_prop_for_diagnostic_display(db, prop.type_id, depth + 1);
        let write = widen_prop_for_diagnostic_display(db, prop.write_type, depth + 1);
        changed |= read != prop.type_id || write != prop.write_type;
        prop.type_id = read;
        prop.write_type = write;
    }
    changed
}

fn widen_indexes_for_diagnostic_display(
    db: &dyn TypeDatabase,
    shape: &mut ObjectShape,
    depth: usize,
) -> bool {
    let mut changed = false;
    if let Some(index) = shape.string_index.as_mut() {
        let value = widen_prop_for_diagnostic_display(db, index.value_type, depth + 1);
        changed |= value != index.value_type;
        index.value_type = value;
    }
    if let Some(index) = shape.number_index.as_mut() {
        let value = widen_prop_for_diagnostic_display(db, index.value_type, depth + 1);
        changed |= value != index.value_type;
        index.value_type = value;
    }
    changed
}

fn widen_prop_for_diagnostic_display(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    depth: usize,
) -> TypeId {
    let widened = common::widen_literal_type(db, type_id);
    if widened != type_id {
        return widened;
    }
    widen_object_properties_for_diagnostic_display_depth(db, type_id, depth)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsz_solver::{
        IndexSignature, ObjectShape, PropertyInfo, TypeFormatter, TypeInterner, TypeParamInfo,
    };

    fn object_with_props(db: &TypeInterner, props: Vec<PropertyInfo>) -> TypeId {
        db.object_with_index(ObjectShape {
            properties: props,
            ..Default::default()
        })
    }

    fn formatted(db: &TypeInterner, type_id: TypeId) -> String {
        TypeFormatter::new(db)
            .with_display_properties()
            .format(type_id)
            .into_owned()
    }

    #[test]
    fn diagnostic_display_widens_fresh_display_properties() {
        let db = TypeInterner::new();
        let x = db.intern_string("x");
        let fresh = db.object_fresh_with_display(
            vec![PropertyInfo::new(x, TypeId::NUMBER)],
            vec![PropertyInfo::new(x, db.literal_number(3.0))],
        );

        let widened = widen_object_properties_for_diagnostic_display(&db, fresh);

        assert_ne!(widened, fresh);
        let rendered = formatted(&db, widened);
        assert!(
            rendered.contains("{ x: number; }"),
            "expected widened display property, got {rendered}"
        );
        assert!(
            !rendered.contains("{ x: 3; }"),
            "fresh literal display should not leak into related info: {rendered}"
        );
    }

    #[test]
    fn diagnostic_display_widens_direct_structural_fresh_literals() {
        let db = TypeInterner::new();
        let x = db.intern_string("x");
        let object = db.object_fresh(vec![PropertyInfo::new(x, db.literal_number(3.0))]);

        let widened = widen_object_properties_for_diagnostic_display(&db, object);
        let shape = object_shape_for_type(&db, widened).expect("expected object shape");

        assert_eq!(shape.properties[0].type_id, TypeId::NUMBER);
        assert_eq!(shape.properties[0].write_type, TypeId::NUMBER);
    }

    #[test]
    fn diagnostic_display_widens_anonymous_structural_literals() {
        let db = TypeInterner::new();
        let name = db.intern_string("name");
        let object = object_with_props(
            &db,
            vec![PropertyInfo::new(name, db.literal_string("alice"))],
        );

        let widened = widen_object_properties_for_diagnostic_display(&db, object);
        let shape = object_shape_for_type(&db, widened).expect("expected object shape");

        assert_eq!(shape.properties[0].type_id, TypeId::STRING);
        assert_eq!(shape.properties[0].write_type, TypeId::STRING);
    }

    #[test]
    fn diagnostic_display_keeps_named_alias_objects_unwidened() {
        let db = TypeInterner::new();
        let x = db.intern_string("x");
        let object = object_with_props(&db, vec![PropertyInfo::new(x, db.literal_number(3.0))]);
        let alias = db.type_param(TypeParamInfo {
            name: db.intern_string("Named"),
            constraint: None,
            default: None,
            is_const: false,
        });
        db.store_display_alias(object, alias);

        let widened = widen_object_properties_for_diagnostic_display(&db, object);

        assert_eq!(widened, object);
    }

    #[test]
    fn diagnostic_display_widens_anonymous_index_signatures() {
        let db = TypeInterner::new();
        let object = db.object_with_index(ObjectShape {
            string_index: Some(IndexSignature {
                key_type: TypeId::STRING,
                value_type: db.literal_number(3.0),
                readonly: false,
                param_name: None,
            }),
            ..Default::default()
        });

        let widened = widen_object_properties_for_diagnostic_display(&db, object);
        let shape = object_shape_for_type(&db, widened).expect("expected object shape");

        assert_eq!(
            shape
                .string_index
                .expect("expected string index")
                .value_type,
            TypeId::NUMBER
        );
    }

    #[test]
    fn diagnostic_display_depth_limit_leaves_deep_anonymous_objects_unexpanded() {
        let db = TypeInterner::new();
        let leaf_name = db.intern_string("leaf");
        let next_name = db.intern_string("next");
        let mut by_depth_from_leaf = Vec::new();
        let mut current = object_with_props(
            &db,
            vec![PropertyInfo::new(leaf_name, db.literal_number(1.0))],
        );
        by_depth_from_leaf.push(current);
        for _ in 0..10 {
            current = object_with_props(&db, vec![PropertyInfo::new(next_name, current)]);
            by_depth_from_leaf.push(current);
        }
        let original_by_depth: Vec<_> = by_depth_from_leaf.iter().rev().copied().collect();

        let widened = widen_object_properties_for_diagnostic_display(&db, current);
        let mut cursor = widened;
        for _ in 0..8 {
            let shape = object_shape_for_type(&db, cursor).expect("expected nested object");
            cursor = shape.properties[0].type_id;
        }
        let depth_eight = object_shape_for_type(&db, cursor).expect("expected depth-8 object");

        assert_eq!(
            depth_eight.properties[0].type_id, original_by_depth[9],
            "object beyond the recursion limit should remain untouched"
        );
    }
}
