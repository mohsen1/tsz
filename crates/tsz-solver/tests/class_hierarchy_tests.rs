use super::*;
use crate::TypeInterner;
use crate::types::Visibility;

#[test]
fn test_merge_properties() {
    let interner = TypeInterner::new();
    let builder = ClassTypeBuilder::new(&interner);

    let name_atom = interner.intern_string("name");
    let age_atom = interner.intern_string("age");

    let base_props = vec![PropertyInfo {
        name: name_atom,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
    }];

    let own_props = vec![
        PropertyInfo {
            name: name_atom, // Override
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: true,
            readonly: true,
            is_method: false,
            visibility: Visibility::Public,
            parent_id: None,
        },
        PropertyInfo {
            name: age_atom, // New
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
            visibility: Visibility::Public,
            parent_id: None,
        },
    ];

    let dummy_symbol = SymbolId(999);
    let merged = builder.merge_properties(base_props, own_props, dummy_symbol);

    assert_eq!(merged.len(), 2);
    // name should be overridden
    let name_prop = merged.iter().find(|p| p.name == name_atom).unwrap();
    assert_eq!(name_prop.type_id, TypeId::NUMBER); // Overridden
    assert!(name_prop.optional); // Overridden
    assert!(name_prop.readonly); // Overridden
}
