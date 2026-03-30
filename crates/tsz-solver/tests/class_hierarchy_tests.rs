use super::merge::merge_properties;
use crate::types::{PropertyInfo, TypeId, Visibility};
use tsz_binder::SymbolId;

#[test]
fn test_merge_properties() {
    let interner = crate::TypeInterner::new();

    let name_atom = interner.intern_string("name");
    let age_atom = interner.intern_string("age");

    let base_props = vec![PropertyInfo {
        name: name_atom,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }];

    let own_props = vec![
        PropertyInfo {
            name: name_atom, // Override
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: true,
            readonly: true,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
        PropertyInfo {
            name: age_atom, // New
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
    ];

    let dummy_symbol = SymbolId(999);
    let merged = merge_properties(base_props, own_props, dummy_symbol);

    assert_eq!(merged.len(), 2);
    // name should be overridden
    let name_prop = merged.iter().find(|p| p.name == name_atom).unwrap();
    assert_eq!(name_prop.type_id, TypeId::NUMBER); // Overridden
    assert!(name_prop.optional); // Overridden
    assert!(name_prop.readonly); // Overridden
}
