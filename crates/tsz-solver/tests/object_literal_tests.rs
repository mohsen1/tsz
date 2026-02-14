use super::*;
use crate::TypeInterner;
use crate::types::{ObjectFlags, Visibility};

#[test]
fn test_build_object_type() {
    let db = TypeInterner::new();
    let builder = ObjectLiteralBuilder::new(&db);

    let properties = vec![PropertyInfo {
        name: db.intern_string("x"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
    }];

    let obj_type = builder.build_object_type(properties);

    let key = db.lookup(obj_type).unwrap();
    assert!(matches!(key, TypeData::Object(_)));
}

#[test]
fn test_merge_spread() {
    let db = TypeInterner::new();
    let builder = ObjectLiteralBuilder::new(&db);

    // Create base object { x: number }
    let base_props = vec![PropertyInfo {
        name: db.intern_string("x"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
    }];

    // Create spread object { y: string, x: boolean }
    let spread_props = vec![
        PropertyInfo {
            name: db.intern_string("y"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            visibility: Visibility::Public,
            parent_id: None,
        },
        PropertyInfo {
            name: db.intern_string("x"),
            type_id: TypeId::BOOLEAN,
            write_type: TypeId::BOOLEAN,
            optional: false,
            readonly: false,
            is_method: false,
            visibility: Visibility::Public,
            parent_id: None,
        },
    ];
    let spread_type = db.object(spread_props);

    // Merge: { x: boolean, y: string } (x is overridden)
    let merged = builder.merge_spread(base_props, spread_type);

    assert_eq!(merged.len(), 2);

    let x_prop = merged
        .iter()
        .find(|p| db.resolve_atom_ref(p.name) == "x".into())
        .unwrap();
    assert_eq!(x_prop.type_id, TypeId::BOOLEAN);

    let y_prop = merged
        .iter()
        .find(|p| db.resolve_atom_ref(p.name) == "y".into())
        .unwrap();
    assert_eq!(y_prop.type_id, TypeId::STRING);
}

#[test]
fn test_apply_contextual_types() {
    let db = TypeInterner::new();
    let builder = ObjectLiteralBuilder::new(&db);

    // Create contextual type { x: number }
    let ctx_type = db.object(vec![PropertyInfo {
        name: db.intern_string("x"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
    }]);

    // Create properties { x: 1 } (where 1 is a literal number type)
    // For simplicity, we'll just use NUMBER type
    let properties = vec![PropertyInfo {
        name: db.intern_string("x"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
    }];

    let contextualized = builder.apply_contextual_types(properties, ctx_type);

    assert_eq!(contextualized.len(), 1);
    assert_eq!(contextualized[0].type_id, TypeId::NUMBER);
}

#[test]
fn test_extract_properties_from_intersection() {
    let db = TypeInterner::new();
    let builder = ObjectLiteralBuilder::new(&db);

    // Create intersection of { x: number } & { y: string }
    let type1 = db.object(vec![PropertyInfo {
        name: db.intern_string("x"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
    }]);

    let type2 = db.object(vec![PropertyInfo {
        name: db.intern_string("y"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
    }]);

    let intersection = db.intersection2(type1, type2);

    let props = builder.extract_properties(intersection);

    assert_eq!(props.len(), 2);
    assert!(
        props
            .iter()
            .any(|p| db.resolve_atom_ref(p.name) == "x".into())
    );
    assert!(
        props
            .iter()
            .any(|p| db.resolve_atom_ref(p.name) == "y".into())
    );
}

#[test]
fn test_collect_spread_properties() {
    let db = TypeInterner::new();
    let builder = ObjectLiteralBuilder::new(&db);

    let spread_type = db.object_with_flags(
        vec![PropertyInfo {
            name: db.intern_string("spread"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
            visibility: Visibility::Public,
            parent_id: None,
        }],
        ObjectFlags::FRESH_LITERAL,
    );

    let props = builder.collect_spread_properties(spread_type);
    assert_eq!(props.len(), 1);
    assert_eq!(db.resolve_atom_ref(props[0].name).as_ref(), "spread");
    assert_eq!(props[0].type_id, TypeId::NUMBER);
}
