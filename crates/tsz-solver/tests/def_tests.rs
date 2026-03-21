use super::*;
use crate::types::Visibility;

#[allow(clippy::duplicate_mod)]
#[path = "common/mod.rs"]
mod common;
use common::create_test_interner;

#[test]
fn test_def_id_validity() {
    assert!(!DefId::INVALID.is_valid());
    assert!(DefId(1).is_valid());
    assert!(DefId(100).is_valid());
}

#[test]
fn test_definition_store_basic() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    let name = interner.intern_string("Foo");
    let info = DefinitionInfo::type_alias(name, vec![], TypeId::NUMBER);

    let def_id = store.register(info);
    assert!(def_id.is_valid());
    assert!(store.contains(def_id));

    let retrieved = store.get(def_id).expect("definition exists");
    assert_eq!(retrieved.kind, DefKind::TypeAlias);
    assert_eq!(retrieved.name, name);
    assert_eq!(retrieved.body, Some(TypeId::NUMBER));
}

#[test]
fn test_definition_store_interface() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    let name = interner.intern_string("Point");
    let x_name = interner.intern_string("x");
    let y_name = interner.intern_string("y");

    let info = DefinitionInfo::interface(
        name,
        vec![],
        vec![
            PropertyInfo {
                name: x_name,
                type_id: TypeId::NUMBER,
                write_type: TypeId::NUMBER,
                optional: false,
                readonly: false,
                is_method: false,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: 0,
            },
            PropertyInfo {
                name: y_name,
                type_id: TypeId::NUMBER,
                write_type: TypeId::NUMBER,
                optional: false,
                readonly: false,
                is_method: false,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: 0,
            },
        ],
    );

    let def_id = store.register(info);

    let retrieved = store.get(def_id).expect("definition exists");
    assert_eq!(retrieved.kind, DefKind::Interface);

    let shape = retrieved.instance_shape.expect("has instance shape");
    assert_eq!(shape.properties.len(), 2);
}

#[test]
fn test_definition_store_class_with_extends() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    // Base class
    let base_name = interner.intern_string("Base");
    let base_info = DefinitionInfo::class(base_name, vec![], vec![], vec![]);
    let base_id = store.register(base_info);

    // Derived class
    let derived_name = interner.intern_string("Derived");
    let derived_info =
        DefinitionInfo::class(derived_name, vec![], vec![], vec![]).with_extends(base_id);
    let derived_id = store.register(derived_info);

    assert_eq!(store.get_extends(derived_id), Some(base_id));
    assert_eq!(store.get_extends(base_id), None);
}

#[test]
fn test_definition_store_enum() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    let name = interner.intern_string("Direction");
    let up = interner.intern_string("Up");
    let down = interner.intern_string("Down");

    let info = DefinitionInfo::enumeration(
        name,
        vec![
            (up, EnumMemberValue::Number(0.0)),
            (down, EnumMemberValue::Number(1.0)),
        ],
    );

    let def_id = store.register(info);

    let retrieved = store.get(def_id).expect("definition exists");
    assert_eq!(retrieved.kind, DefKind::Enum);
    assert_eq!(retrieved.enum_members.len(), 2);
}

#[test]
fn test_definition_store_set_body() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    let name = interner.intern_string("Point");
    let mut info = DefinitionInfo::interface(name, vec![], vec![]);
    info.body = None; // Start with no body

    let def_id = store.register(info);
    assert_eq!(store.get_body(def_id), None);

    // Set body later
    store.set_body(def_id, TypeId::NUMBER);
    assert_eq!(store.get_body(def_id), Some(TypeId::NUMBER));
}

#[test]
fn test_content_addressed_def_ids() {
    let interner = create_test_interner();
    let generator = ContentAddressedDefIds::new();

    let name = interner.intern_string("Foo");

    // Same content -> same DefId
    let id1 = generator.get_or_create(name, 1, 100);
    let id2 = generator.get_or_create(name, 1, 100);
    assert_eq!(id1, id2);

    // Different content -> different DefId
    let id3 = generator.get_or_create(name, 1, 200);
    assert_ne!(id1, id3);

    let id4 = generator.get_or_create(name, 2, 100);
    assert_ne!(id1, id4);

    let name2 = interner.intern_string("Bar");
    let id5 = generator.get_or_create(name2, 1, 100);
    assert_ne!(id1, id5);
}

#[test]
fn test_definition_store_concurrent() {
    use std::thread;

    let store = std::sync::Arc::new(DefinitionStore::new());

    let handles: Vec<_> = (0..4)
        .map(|i| {
            let store = store.clone();
            thread::spawn(move || {
                for j in 0..100 {
                    let info = DefinitionInfo {
                        kind: DefKind::TypeAlias,
                        name: tsz_common::interner::Atom(i * 1000 + j),
                        type_params: vec![],
                        body: Some(TypeId::NUMBER),
                        instance_shape: None,
                        static_shape: None,
                        extends: None,
                        implements: Vec::new(),
                        enum_members: Vec::new(),
                        exports: Vec::new(),
                        file_id: None,
                        span: None,
                        symbol_id: None,
                    };
                    let id = store.register(info);
                    assert!(store.contains(id));
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().expect("thread completed");
    }

    assert_eq!(store.len(), 400);
}

// =============================================================================
// Symbol mapping index tests
// =============================================================================

#[test]
fn test_symbol_def_index_basic_lookup() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    let name = interner.intern_string("Foo");
    let info = DefinitionInfo::type_alias(name, vec![], TypeId::NUMBER);
    let def_id = store.register(info);

    // Register in the symbol index
    let symbol_id = 42u32;
    let file_idx = 0u32;
    store.register_symbol_mapping(symbol_id, file_idx, def_id);

    // Lookup should succeed
    assert_eq!(store.lookup_by_symbol(symbol_id, file_idx), Some(def_id));

    // Lookup with different file_idx should fail (different binder)
    assert_eq!(store.lookup_by_symbol(symbol_id, 1), None);

    // Lookup with different symbol_id should fail
    assert_eq!(store.lookup_by_symbol(43, file_idx), None);
}

#[test]
fn test_symbol_def_index_cross_binder_disambiguation() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    // Same SymbolId(5) in two different binders (file_idx 0 and 1)
    let name_foo = interner.intern_string("Foo");
    let info_foo = DefinitionInfo::type_alias(name_foo, vec![], TypeId::NUMBER);
    let def_foo = store.register(info_foo);

    let name_bar = interner.intern_string("Bar");
    let info_bar = DefinitionInfo::type_alias(name_bar, vec![], TypeId::STRING);
    let def_bar = store.register(info_bar);

    // Register same symbol_id=5 for different files
    store.register_symbol_mapping(5, 0, def_foo);
    store.register_symbol_mapping(5, 1, def_bar);

    // Each lookup returns the correct DefId for its binder
    assert_eq!(store.lookup_by_symbol(5, 0), Some(def_foo));
    assert_eq!(store.lookup_by_symbol(5, 1), Some(def_bar));
    assert_ne!(def_foo, def_bar);
}

#[test]
fn test_symbol_def_index_cleared_on_store_clear() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    let name = interner.intern_string("X");
    let info = DefinitionInfo::type_alias(name, vec![], TypeId::BOOLEAN);
    let def_id = store.register(info);
    store.register_symbol_mapping(10, 0, def_id);

    assert_eq!(store.lookup_by_symbol(10, 0), Some(def_id));

    store.clear();

    // After clear, lookup should return None
    assert_eq!(store.lookup_by_symbol(10, 0), None);
}

// =============================================================================
// Symbol-only index tests (find_def_by_symbol O(1))
// =============================================================================

#[test]
fn test_find_def_by_symbol_via_register() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    let name = interner.intern_string("MyClass");
    let mut info = DefinitionInfo::type_alias(name, vec![], TypeId::NUMBER);
    info.symbol_id = Some(42);
    let def_id = store.register(info);

    // find_def_by_symbol should return the registered DefId via O(1) index.
    assert_eq!(store.find_def_by_symbol(42), Some(def_id));

    // Non-existent symbol_id should return None.
    assert_eq!(store.find_def_by_symbol(99), None);
}

#[test]
fn test_find_def_by_symbol_via_register_symbol_mapping() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    let name = interner.intern_string("Iface");
    // Register without symbol_id in DefinitionInfo.
    let info = DefinitionInfo::type_alias(name, vec![], TypeId::NUMBER);
    let def_id = store.register(info);

    // register_symbol_mapping should populate symbol_only_index.
    store.register_symbol_mapping(77, 0, def_id);

    assert_eq!(store.find_def_by_symbol(77), Some(def_id));
}

#[test]
fn test_find_def_by_symbol_keeps_first_registered() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    // Register two defs with the same symbol_id (different file_idx).
    let name1 = interner.intern_string("A");
    let mut info1 = DefinitionInfo::type_alias(name1, vec![], TypeId::NUMBER);
    info1.symbol_id = Some(10);
    let def1 = store.register(info1);

    let name2 = interner.intern_string("A");
    let mut info2 = DefinitionInfo::type_alias(name2, vec![], TypeId::STRING);
    info2.symbol_id = Some(10);
    let def2 = store.register(info2);

    // The first registered DefId should be returned.
    assert_eq!(store.find_def_by_symbol(10), Some(def1));
    assert_ne!(def1, def2);
}

#[test]
fn test_find_def_by_symbol_cleared() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    let name = interner.intern_string("X");
    let mut info = DefinitionInfo::type_alias(name, vec![], TypeId::NUMBER);
    info.symbol_id = Some(55);
    store.register(info);

    assert!(store.find_def_by_symbol(55).is_some());

    store.clear();

    assert_eq!(store.find_def_by_symbol(55), None);
}

// =============================================================================
// Body-to-alias index tests (find_type_alias_by_body O(1))
// =============================================================================

#[test]
fn test_find_type_alias_by_body_via_register() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    let name = interner.intern_string("Color");
    let body_type = TypeId(200);
    let info = DefinitionInfo::type_alias(name, vec![], body_type);
    let def_id = store.register(info);

    // O(1) lookup should find it.
    assert_eq!(store.find_type_alias_by_body(body_type), Some(def_id));

    // Non-matching body should return None.
    assert_eq!(store.find_type_alias_by_body(TypeId(999)), None);
}

#[test]
fn test_find_type_alias_by_body_via_set_body() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    let name = interner.intern_string("Alias");
    // Register with no body initially.
    let info = DefinitionInfo {
        kind: DefKind::TypeAlias,
        name,
        type_params: vec![],
        body: None,
        instance_shape: None,
        static_shape: None,
        extends: None,
        implements: Vec::new(),
        enum_members: Vec::new(),
        exports: Vec::new(),
        file_id: None,
        span: None,
        symbol_id: None,
    };
    let def_id = store.register(info);

    // No body yet.
    assert_eq!(store.find_type_alias_by_body(TypeId(300)), None);

    // Set body lazily.
    store.set_body(def_id, TypeId(300));

    // Now O(1) lookup should find it.
    assert_eq!(store.find_type_alias_by_body(TypeId(300)), Some(def_id));
}

#[test]
fn test_find_type_alias_by_body_ignores_generic_aliases() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    let name = interner.intern_string("GenericAlias");
    let body_type = TypeId(400);
    let tp = crate::types::TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let info = DefinitionInfo::type_alias(name, vec![tp], body_type);
    store.register(info);

    // Generic type aliases should NOT be indexed.
    assert_eq!(store.find_type_alias_by_body(body_type), None);
}

#[test]
fn test_find_type_alias_by_body_ignores_non_alias_kinds() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    let name = interner.intern_string("MyInterface");
    let body_type = TypeId(600);
    let mut info = DefinitionInfo::interface(name, vec![], vec![]);
    info.body = Some(body_type);
    store.register(info);

    // Interface bodies should NOT be indexed in body_to_alias.
    assert_eq!(store.find_type_alias_by_body(body_type), None);
}

#[test]
fn test_find_type_alias_by_body_cleared() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    let name = interner.intern_string("X");
    let body = TypeId(700);
    let info = DefinitionInfo::type_alias(name, vec![], body);
    store.register(info);

    assert!(store.find_type_alias_by_body(body).is_some());

    store.clear();

    assert_eq!(store.find_type_alias_by_body(body), None);
}

// =============================================================================
// Shape-to-def index tests (find_def_by_shape O(1))
// =============================================================================

#[test]
fn test_find_def_by_shape_via_register() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    let name = interner.intern_string("Point");
    let x_name = interner.intern_string("x");
    let y_name = interner.intern_string("y");

    let props = vec![
        PropertyInfo {
            name: x_name,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
        },
        PropertyInfo {
            name: y_name,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
        },
    ];

    let info = DefinitionInfo::interface(name, vec![], props.clone());
    let def_id = store.register(info);

    // Build the same shape for lookup.
    let lookup_shape = ObjectShape {
        flags: ObjectFlags::empty(),
        properties: props,
        string_index: None,
        number_index: None,
        symbol: None,
    };

    // O(1) lookup should find it.
    assert_eq!(store.find_def_by_shape(&lookup_shape), Some(def_id));
}

#[test]
fn test_find_def_by_shape_no_match() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    let name = interner.intern_string("Foo");
    let info = DefinitionInfo::type_alias(name, vec![], TypeId::NUMBER);
    store.register(info);

    // Type aliases have no instance_shape, so lookup should return None.
    let empty_shape = ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: None,
        number_index: None,
        symbol: None,
    };
    assert_eq!(store.find_def_by_shape(&empty_shape), None);
}

#[test]
fn test_find_def_by_shape_via_set_instance_shape() {
    use std::sync::Arc;

    let interner = create_test_interner();
    let store = DefinitionStore::new();

    let name = interner.intern_string("Iface");
    // Register with no instance shape.
    let info = DefinitionInfo {
        kind: DefKind::Interface,
        name,
        type_params: vec![],
        body: None,
        instance_shape: None,
        static_shape: None,
        extends: None,
        implements: Vec::new(),
        enum_members: Vec::new(),
        exports: Vec::new(),
        file_id: None,
        span: None,
        symbol_id: None,
    };
    let def_id = store.register(info);

    let z_name = interner.intern_string("z");
    let shape = ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo {
            name: z_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
        }],
        string_index: None,
        number_index: None,
        symbol: None,
    };

    // No shape yet.
    assert_eq!(store.find_def_by_shape(&shape), None);

    // Set instance shape.
    store.set_instance_shape(def_id, Arc::new(shape.clone()));

    // Now O(1) lookup should find it.
    assert_eq!(store.find_def_by_shape(&shape), Some(def_id));
}

#[test]
fn test_find_def_by_shape_cleared() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    let name = interner.intern_string("A");
    let info = DefinitionInfo::interface(name, vec![], vec![]);
    store.register(info);

    let empty_shape = ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: None,
        number_index: None,
        symbol: None,
    };

    assert!(store.find_def_by_shape(&empty_shape).is_some());

    store.clear();

    assert_eq!(store.find_def_by_shape(&empty_shape), None);
}
