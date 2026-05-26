use crate::construction::TypeInterner;
use crate::types::{MappedType, PropertyInfo, TypeData, TypeId, TypeParamInfo};

#[test]
fn inferred_declaration_mapped_constraint_surface_uses_primitive_constraint() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("Source");
    let key_name = interner.intern_string("Key");
    let source_param = interner.type_param(TypeParamInfo {
        name: t_name,
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    });

    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: key_name,
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: interner.keyof(source_param),
        name_type: None,
        template: TypeId::VOID,
        readonly_modifier: None,
        optional_modifier: None,
    });

    assert_eq!(
        crate::type_queries::inferred_declaration_mapped_constraint_surface(&interner, mapped),
        Some(TypeId::STRING)
    );
}

#[test]
fn inferred_declaration_mapped_constraint_surface_expands_object_constraint() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("Item");
    let key_name = interner.intern_string("Property");
    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");
    let constraint = interner.object(vec![
        PropertyInfo::new(a_name, TypeId::STRING),
        PropertyInfo::new(b_name, TypeId::NUMBER),
    ]);
    let source_param = interner.type_param(TypeParamInfo {
        name: t_name,
        constraint: Some(constraint),
        default: None,
        is_const: false,
    });

    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: key_name,
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: interner.keyof(source_param),
        name_type: None,
        template: TypeId::VOID,
        readonly_modifier: None,
        optional_modifier: None,
    });

    let surface =
        crate::type_queries::inferred_declaration_mapped_constraint_surface(&interner, mapped)
            .expect("object constraint should produce a public surface");
    let Some(TypeData::Object(shape_id)) = interner.lookup(surface) else {
        panic!(
            "expected object surface, got {:?}",
            interner.lookup(surface)
        );
    };
    let shape = interner.object_shape(shape_id);
    assert_eq!(shape.properties.len(), 2);
    let a_prop = shape
        .properties
        .iter()
        .find(|prop| prop.name == a_name)
        .expect("expected a property");
    let b_prop = shape
        .properties
        .iter()
        .find(|prop| prop.name == b_name)
        .expect("expected b property");
    assert_eq!(a_prop.type_id, TypeId::VOID);
    assert_eq!(b_prop.type_id, TypeId::VOID);
}

#[test]
fn number_wrapper_properties_sort_in_declaration_display_order() {
    let interner = TypeInterner::new();
    let mut props: Vec<_> = [
        "toLocaleString",
        "toString",
        "valueOf",
        "toExponential",
        "toFixed",
        "toPrecision",
    ]
    .into_iter()
    .map(|name| PropertyInfo::new(interner.intern_string(name), TypeId::VOID))
    .collect();

    assert!(
        crate::type_queries::sort_number_wrapper_properties_for_display(&interner, &mut props,)
    );

    let names: Vec<_> = props
        .iter()
        .map(|prop| interner.resolve_atom_ref(prop.name).to_string())
        .collect();
    assert_eq!(
        names,
        vec![
            "toString",
            "toFixed",
            "toExponential",
            "toPrecision",
            "valueOf",
            "toLocaleString",
        ]
    );
}
