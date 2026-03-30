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
                is_string_named: false,
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
                is_string_named: false,
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
                        heritage_names: Vec::new(),
                        is_abstract: false,
                        is_const: false,
                        is_exported: false,
                        is_global_augmentation: false,
                        is_declare: false,
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
        heritage_names: Vec::new(),
        is_abstract: false,
        is_const: false,
        is_exported: false,
        is_global_augmentation: false,
        is_declare: false,
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
            is_string_named: false,
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
            is_string_named: false,
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
        heritage_names: Vec::new(),
        is_abstract: false,
        is_const: false,
        is_exported: false,
        is_global_augmentation: false,
        is_declare: false,
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
            is_string_named: false,
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

// =============================================================================
// File-based index tests
// =============================================================================

#[test]
fn test_defs_by_file_empty() {
    let store = DefinitionStore::new();
    assert!(store.defs_by_file(42).is_empty());
    assert_eq!(store.file_count(), 0);
}

#[test]
fn test_defs_by_file_single_file() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    let mut info1 =
        DefinitionInfo::type_alias(interner.intern_string("Foo"), vec![], TypeId::NUMBER);
    info1.file_id = Some(10);
    let id1 = store.register(info1);

    let mut info2 = DefinitionInfo::interface(interner.intern_string("Bar"), vec![], vec![]);
    info2.file_id = Some(10);
    let id2 = store.register(info2);

    let defs = store.defs_by_file(10);
    assert_eq!(defs.len(), 2);
    assert!(defs.contains(&id1));
    assert!(defs.contains(&id2));
    assert_eq!(store.file_count(), 1);
}

#[test]
fn test_defs_by_file_multiple_files() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    let mut info_a =
        DefinitionInfo::type_alias(interner.intern_string("A"), vec![], TypeId::NUMBER);
    info_a.file_id = Some(1);
    let id_a = store.register(info_a);

    let mut info_b =
        DefinitionInfo::type_alias(interner.intern_string("B"), vec![], TypeId::STRING);
    info_b.file_id = Some(2);
    let id_b = store.register(info_b);

    assert_eq!(store.defs_by_file(1), vec![id_a]);
    assert_eq!(store.defs_by_file(2), vec![id_b]);
    assert_eq!(store.file_count(), 2);
}

#[test]
fn test_defs_by_file_no_file_id() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    // Definitions without file_id should not appear in file index.
    let info = DefinitionInfo::type_alias(interner.intern_string("Orphan"), vec![], TypeId::NUMBER);
    store.register(info);

    assert_eq!(store.file_count(), 0);
}

#[test]
fn test_invalidate_file_removes_definitions() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    let mut info =
        DefinitionInfo::type_alias(interner.intern_string("Foo"), vec![], TypeId::NUMBER);
    info.file_id = Some(5);
    info.symbol_id = Some(100);
    let def_id = store.register(info);

    // Register symbol mapping so we can verify cleanup.
    store.register_symbol_mapping(100, 5, def_id);

    assert!(store.contains(def_id));
    assert_eq!(store.find_def_by_symbol(100), Some(def_id));
    assert_eq!(store.lookup_by_symbol(100, 5), Some(def_id));

    let invalidated = store.invalidate_file(5);
    assert_eq!(invalidated, 1);

    // Definition should be gone.
    assert!(!store.contains(def_id));
    assert!(store.defs_by_file(5).is_empty());
    assert_eq!(store.file_count(), 0);

    // Symbol indices should be cleaned up.
    assert_eq!(store.find_def_by_symbol(100), None);
    assert_eq!(store.lookup_by_symbol(100, 5), None);
}

#[test]
fn test_invalidate_file_preserves_other_files() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    let mut info1 = DefinitionInfo::type_alias(interner.intern_string("A"), vec![], TypeId::NUMBER);
    info1.file_id = Some(1);
    let id1 = store.register(info1);

    let mut info2 = DefinitionInfo::type_alias(interner.intern_string("B"), vec![], TypeId::STRING);
    info2.file_id = Some(2);
    let id2 = store.register(info2);

    store.invalidate_file(1);

    assert!(!store.contains(id1));
    assert!(store.contains(id2));
    assert_eq!(store.defs_by_file(2), vec![id2]);
    assert_eq!(store.file_count(), 1);
}

#[test]
fn test_invalidate_file_cleans_body_to_alias() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    let mut info =
        DefinitionInfo::type_alias(interner.intern_string("Color"), vec![], TypeId::NUMBER);
    info.file_id = Some(3);
    store.register(info);

    // body_to_alias should map NUMBER -> DefId.
    assert!(store.find_type_alias_by_body(TypeId::NUMBER).is_some());

    store.invalidate_file(3);

    // After invalidation, the body_to_alias entry should be cleaned up.
    assert!(store.find_type_alias_by_body(TypeId::NUMBER).is_none());
}

#[test]
fn test_invalidate_file_idempotent() {
    let store = DefinitionStore::new();

    // Invalidating a non-existent file returns 0 and doesn't panic.
    assert_eq!(store.invalidate_file(999), 0);
    assert_eq!(store.invalidate_file(999), 0);
}

#[test]
fn test_invalidate_file_cleans_shape_index() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    let empty_shape = ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: None,
        number_index: None,
        symbol: None,
    };

    let mut info = DefinitionInfo::interface(interner.intern_string("Empty"), vec![], vec![]);
    info.file_id = Some(7);
    store.register(info);

    assert!(store.find_def_by_shape(&empty_shape).is_some());

    store.invalidate_file(7);

    assert!(store.find_def_by_shape(&empty_shape).is_none());
}

#[test]
fn test_clear_resets_file_index() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    let mut info = DefinitionInfo::type_alias(interner.intern_string("X"), vec![], TypeId::NUMBER);
    info.file_id = Some(1);
    store.register(info);

    assert_eq!(store.file_count(), 1);

    store.clear();

    assert_eq!(store.file_count(), 0);
    assert!(store.defs_by_file(1).is_empty());
}

// =============================================================================
// StoreStatistics tests
// =============================================================================

#[test]
fn test_statistics_empty_store() {
    let store = DefinitionStore::new();
    let stats = store.statistics();

    assert_eq!(stats.total_definitions, 0);
    assert_eq!(stats.type_aliases, 0);
    assert_eq!(stats.interfaces, 0);
    assert_eq!(stats.classes, 0);
    assert_eq!(stats.class_constructors, 0);
    assert_eq!(stats.enums, 0);
    assert_eq!(stats.namespaces, 0);
    assert_eq!(stats.functions, 0);
    assert_eq!(stats.variables, 0);
    assert_eq!(stats.type_to_def_entries, 0);
    assert_eq!(stats.symbol_def_index_entries, 0);
    assert_eq!(stats.symbol_only_index_entries, 0);
    assert_eq!(stats.body_to_alias_entries, 0);
    assert_eq!(stats.shape_to_def_entries, 0);
    assert_eq!(stats.file_count, 0);
    assert_eq!(stats.next_def_id, DefId::FIRST_VALID);
}

#[test]
fn test_statistics_counts_by_kind() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    // Register one of each kind.
    store.register(DefinitionInfo::type_alias(
        interner.intern_string("Alias1"),
        vec![],
        TypeId::NUMBER,
    ));
    store.register(DefinitionInfo::type_alias(
        interner.intern_string("Alias2"),
        vec![],
        TypeId::STRING,
    ));
    store.register(DefinitionInfo::interface(
        interner.intern_string("Iface"),
        vec![],
        vec![],
    ));
    store.register(DefinitionInfo::class(
        interner.intern_string("Cls"),
        vec![],
        vec![],
        vec![],
    ));
    store.register(DefinitionInfo::enumeration(
        interner.intern_string("Dir"),
        vec![],
    ));
    store.register(DefinitionInfo::namespace(
        interner.intern_string("NS"),
        vec![],
    ));

    let stats = store.statistics();

    assert_eq!(stats.total_definitions, 6);
    assert_eq!(stats.type_aliases, 2);
    assert_eq!(stats.interfaces, 1);
    assert_eq!(stats.classes, 1);
    assert_eq!(stats.enums, 1);
    assert_eq!(stats.namespaces, 1);
    assert_eq!(stats.functions, 0);
    assert_eq!(stats.variables, 0);
    // next_def_id should reflect 6 allocations starting from FIRST_VALID.
    assert_eq!(stats.next_def_id, DefId::FIRST_VALID + 6);
}

#[test]
fn test_statistics_index_counts() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    // Type alias with body -> populates body_to_alias.
    let info = DefinitionInfo::type_alias(interner.intern_string("A"), vec![], TypeId(200));
    let def_a = store.register(info);

    // Interface with shape -> populates shape_to_def.
    let mut info_b = DefinitionInfo::interface(interner.intern_string("B"), vec![], vec![]);
    info_b.file_id = Some(1);
    info_b.symbol_id = Some(10);
    let def_b = store.register(info_b);

    // Register type and symbol mappings.
    store.register_type_to_def(TypeId(300), def_a);
    store.register_symbol_mapping(10, 1, def_b);

    let stats = store.statistics();

    assert_eq!(stats.total_definitions, 2);
    assert_eq!(stats.type_to_def_entries, 1);
    // symbol_def_index has the explicit mapping.
    assert_eq!(stats.symbol_def_index_entries, 1);
    // symbol_only_index: sym 10 from register (symbol_id=Some(10)) + register_symbol_mapping.
    assert_eq!(stats.symbol_only_index_entries, 1);
    // body_to_alias: A's body TypeId(200).
    assert_eq!(stats.body_to_alias_entries, 1);
    // shape_to_def: B's empty interface shape.
    assert_eq!(stats.shape_to_def_entries, 1);
    assert_eq!(stats.file_count, 1);
}

#[test]
fn test_statistics_after_invalidation() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    let mut info = DefinitionInfo::type_alias(interner.intern_string("X"), vec![], TypeId::NUMBER);
    info.file_id = Some(5);
    info.symbol_id = Some(42);
    store.register(info);

    let stats_before = store.statistics();
    assert_eq!(stats_before.total_definitions, 1);
    assert_eq!(stats_before.type_aliases, 1);
    assert_eq!(stats_before.file_count, 1);

    store.invalidate_file(5);

    let stats_after = store.statistics();
    assert_eq!(stats_after.total_definitions, 0);
    assert_eq!(stats_after.type_aliases, 0);
    assert_eq!(stats_after.file_count, 0);
    assert_eq!(stats_after.symbol_only_index_entries, 0);
    // next_def_id is NOT reset by invalidation (monotonically increasing).
    assert!(stats_after.next_def_id >= stats_before.next_def_id);
}

#[test]
fn test_statistics_display_format() {
    let store = DefinitionStore::new();
    let stats = store.statistics();
    let display = format!("{stats}");

    // Verify key sections are present.
    assert!(display.contains("DefinitionStore statistics:"));
    assert!(display.contains("definitions:"));
    assert!(display.contains("type_aliases="));
    assert!(display.contains("indices:"));
    assert!(display.contains("files:"));
    assert!(display.contains("next_def_id:"));
}

#[test]
fn test_store_statistics_merge() {
    let interner = create_test_interner();

    // Create two stores with different definitions.
    let store_a = DefinitionStore::new();
    let store_b = DefinitionStore::new();

    let name_a = interner.intern_string("Foo");
    let name_b = interner.intern_string("Bar");
    let name_c = interner.intern_string("Baz");

    store_a.register(DefinitionInfo::type_alias(name_a, vec![], TypeId::NUMBER));
    store_a.register(DefinitionInfo::interface(name_b, vec![], vec![]));

    store_b.register(DefinitionInfo::type_alias(name_c, vec![], TypeId::STRING));

    let stats_a = store_a.statistics();
    let stats_b = store_b.statistics();

    let mut merged = StoreStatistics::default();
    merged.merge(&stats_a);
    merged.merge(&stats_b);

    assert_eq!(merged.total_definitions, 3);
    assert_eq!(merged.type_aliases, 2);
    assert_eq!(merged.interfaces, 1);
    // next_def_id should be max of both stores.
    assert_eq!(
        merged.next_def_id,
        stats_a.next_def_id.max(stats_b.next_def_id)
    );
}

#[test]
fn estimated_size_bytes_empty_store_is_nonzero() {
    let store = DefinitionStore::new();
    let size = store.estimated_size_bytes();
    // Even an empty store has its own struct size (DashMaps, atomics, etc.)
    assert!(
        size > 0,
        "Empty DefinitionStore should have nonzero estimated size"
    );
}

#[test]
fn estimated_size_bytes_grows_with_definitions() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    let size_before = store.estimated_size_bytes();

    // Register several definitions.
    for i in 0..10 {
        let name = interner.intern_string(&format!("Type{i}"));
        let info = DefinitionInfo::type_alias(name, vec![], TypeId::NUMBER);
        store.register(info);
    }

    let size_after = store.estimated_size_bytes();
    assert!(
        size_after > size_before,
        "Adding definitions should increase estimated size: before={size_before}, after={size_after}"
    );
}

#[test]
fn estimated_size_bytes_accounts_for_type_params() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    // Register without type params, measure.
    let name_a = interner.intern_string("Foo");
    let info = DefinitionInfo::type_alias(name_a, vec![], TypeId::NUMBER);
    store.register(info);
    let size_one = store.estimated_size_bytes();

    // Register a second definition with type params in the same store.
    let name_b = interner.intern_string("Bar");
    let params = vec![
        crate::TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: None,
            default: None,
            is_const: false,
        },
        crate::TypeParamInfo {
            name: interner.intern_string("U"),
            constraint: Some(TypeId::STRING),
            default: None,
            is_const: false,
        },
    ];
    let info = DefinitionInfo::type_alias(name_b, params, TypeId::NUMBER);
    store.register(info);
    let size_two = store.estimated_size_bytes();

    // The second definition (with type params) should add more than
    // what a zero-param definition adds, because TypeParamInfo entries
    // consume additional heap space.
    let delta = size_two - size_one;
    // A zero-param DefinitionInfo in a DashMap uses ~DefinitionInfo + DefId + overhead.
    // With 2 TypeParamInfo entries, it should be at least 2 * size_of::<TypeParamInfo>() larger.
    let min_extra = 2 * std::mem::size_of::<crate::TypeParamInfo>();
    assert!(
        delta >= min_extra,
        "Adding a definition with 2 type params should add at least {min_extra} bytes \
         for the TypeParamInfo entries, but delta was only {delta}"
    );
}

#[test]
fn test_store_statistics_includes_estimated_size() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    // Empty store should still have non-zero estimated size (struct overhead).
    let empty_stats = store.statistics();
    assert!(
        empty_stats.estimated_size_bytes > 0,
        "Even an empty DefinitionStore has non-zero estimated_size_bytes for struct overhead"
    );

    // Add a definition and verify the estimate grows.
    let name = interner.intern_string("TestType");
    let info = DefinitionInfo::type_alias(name, vec![], TypeId::NUMBER);
    store.register(info);

    let stats_after = store.statistics();
    assert!(
        stats_after.estimated_size_bytes > empty_stats.estimated_size_bytes,
        "estimated_size_bytes should grow after adding a definition: {} vs {}",
        stats_after.estimated_size_bytes,
        empty_stats.estimated_size_bytes,
    );
    assert_eq!(stats_after.total_definitions, 1);

    // The estimated_size_bytes in stats should match the live estimate.
    assert_eq!(
        stats_after.estimated_size_bytes,
        store.estimated_size_bytes(),
        "StoreStatistics::estimated_size_bytes must equal DefinitionStore::estimated_size_bytes()"
    );
}

#[test]
fn test_store_statistics_merge_includes_estimated_size() {
    let mut stats_a = StoreStatistics {
        total_definitions: 10,
        estimated_size_bytes: 5000,
        ..Default::default()
    };
    let stats_b = StoreStatistics {
        total_definitions: 5,
        estimated_size_bytes: 3000,
        ..Default::default()
    };
    stats_a.merge(&stats_b);
    assert_eq!(stats_a.total_definitions, 15);
    assert_eq!(stats_a.estimated_size_bytes, 8000);
}

// =============================================================================
// Name-based index tests (find_defs_by_name O(1))
// =============================================================================

#[test]
fn test_find_defs_by_name_basic() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    let name = interner.intern_string("Foo");
    let info = DefinitionInfo::type_alias(name, vec![], TypeId::NUMBER);
    let def_id = store.register(info);

    // Should find the registered definition by name.
    let found = store.find_defs_by_name(name).expect("should find by name");
    assert_eq!(found, vec![def_id]);

    // Non-existent name should return None.
    let other = interner.intern_string("Bar");
    assert!(store.find_defs_by_name(other).is_none());
}

#[test]
fn test_find_defs_by_name_multiple_same_name() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    let name = interner.intern_string("Point");
    let info1 = DefinitionInfo::interface(name, vec![], vec![]);
    let id1 = store.register(info1);

    // Second definition with the same name (interface merging scenario).
    let info2 = DefinitionInfo::interface(name, vec![], vec![]);
    let id2 = store.register(info2);

    let found = store.find_defs_by_name(name).expect("should find by name");
    assert_eq!(found.len(), 2);
    assert!(found.contains(&id1));
    assert!(found.contains(&id2));
}

#[test]
fn test_find_defs_by_name_cleared() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    let name = interner.intern_string("X");
    let info = DefinitionInfo::type_alias(name, vec![], TypeId::NUMBER);
    store.register(info);

    assert!(store.find_defs_by_name(name).is_some());

    store.clear();

    assert!(store.find_defs_by_name(name).is_none());
}

#[test]
fn test_find_defs_by_name_after_invalidation() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    let name = interner.intern_string("Widget");
    let mut info = DefinitionInfo::type_alias(name, vec![], TypeId::NUMBER);
    info.file_id = Some(10);
    store.register(info);

    assert!(store.find_defs_by_name(name).is_some());

    store.invalidate_file(10);

    // After invalidation, the name index entry should be cleaned up.
    assert!(store.find_defs_by_name(name).is_none());
}

#[test]
fn test_find_defs_by_name_partial_invalidation() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    let name = interner.intern_string("Shared");

    // Register two defs with the same name in different files.
    let mut info1 = DefinitionInfo::interface(name, vec![], vec![]);
    info1.file_id = Some(1);
    let id1 = store.register(info1);

    let mut info2 = DefinitionInfo::interface(name, vec![], vec![]);
    info2.file_id = Some(2);
    let id2 = store.register(info2);

    assert_eq!(store.find_defs_by_name(name).unwrap().len(), 2);

    // Invalidate only file 1.
    store.invalidate_file(1);

    let remaining = store
        .find_defs_by_name(name)
        .expect("should still have entries");
    assert_eq!(remaining, vec![id2]);
    assert!(!remaining.contains(&id1));
}

// =============================================================================
// Heritage resolution tests
// =============================================================================

#[test]
fn test_resolve_heritage_basic() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    // Register a base class "Animal".
    let animal_name = interner.intern_string("Animal");
    let animal_info = DefinitionInfo::class(animal_name, vec![], vec![], vec![]);
    let animal_id = store.register(animal_info);

    // Register a derived class "Dog" with heritage_names pointing to "Animal".
    let dog_name = interner.intern_string("Dog");
    let mut dog_info = DefinitionInfo::class(dog_name, vec![], vec![], vec![]);
    dog_info.heritage_names = vec!["Animal".to_string()];
    let dog_id = store.register(dog_info);

    // Resolve heritage for Dog — should find Animal.
    let resolved = store.resolve_heritage(dog_id, &|s| interner.intern_string(s));
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0], ("Animal".to_string(), animal_id));
}

#[test]
fn test_resolve_heritage_unresolved() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    let name = interner.intern_string("Orphan");
    let mut info = DefinitionInfo::class(name, vec![], vec![], vec![]);
    info.heritage_names = vec!["NonExistent".to_string()];
    let id = store.register(info);

    // Try to resolve a heritage name that doesn't exist — should return empty.
    let resolved = store.resolve_heritage(id, &|s| interner.intern_string(s));
    assert!(resolved.is_empty());
}

#[test]
fn test_resolve_heritage_skips_self() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    let name = interner.intern_string("SelfRef");
    let mut info = DefinitionInfo::class(name, vec![], vec![], vec![]);
    info.heritage_names = vec!["SelfRef".to_string()];
    let id = store.register(info);

    // Self-references should be skipped.
    let resolved = store.resolve_heritage(id, &|s| interner.intern_string(s));
    assert!(resolved.is_empty());
}

#[test]
fn test_resolve_heritage_skips_non_class_interface() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    // Register a type alias named "Target".
    let target_name = interner.intern_string("Target");
    let target_info = DefinitionInfo::type_alias(target_name, vec![], TypeId::NUMBER);
    store.register(target_info);

    // Register a derived class with heritage_names pointing to "Target".
    let derived_name = interner.intern_string("Derived");
    let mut derived_info = DefinitionInfo::class(derived_name, vec![], vec![], vec![]);
    derived_info.heritage_names = vec!["Target".to_string()];
    let derived_id = store.register(derived_info);

    // Type aliases should not match heritage resolution.
    let resolved = store.resolve_heritage(derived_id, &|s| interner.intern_string(s));
    assert!(resolved.is_empty());
}

#[test]
fn test_set_heritage() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    let base_name = interner.intern_string("Base");
    let base_info = DefinitionInfo::class(base_name, vec![], vec![], vec![]);
    let base_id = store.register(base_info);

    let iface_name = interner.intern_string("Serializable");
    let iface_info = DefinitionInfo::interface(iface_name, vec![], vec![]);
    let iface_id = store.register(iface_info);

    let child_name = interner.intern_string("Child");
    let child_info = DefinitionInfo::class(child_name, vec![], vec![], vec![]);
    let child_id = store.register(child_info);

    // Wire extends and implements via set_heritage.
    store.set_heritage(child_id, Some(base_id), vec![iface_id]);

    let child = store.get(child_id).unwrap();
    assert_eq!(child.extends, Some(base_id));
    assert_eq!(child.implements, vec![iface_id]);

    // Calling set_heritage again overwrites previous values.
    store.set_heritage(child_id, Some(base_id), vec![iface_id, iface_id]);
    let child = store.get(child_id).unwrap();
    assert_eq!(child.extends, Some(base_id));
    assert_eq!(child.implements, vec![iface_id, iface_id]);
}

#[test]
fn test_statistics_includes_name_index() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    let name_a = interner.intern_string("A");
    let name_b = interner.intern_string("B");
    store.register(DefinitionInfo::type_alias(name_a, vec![], TypeId::NUMBER));
    store.register(DefinitionInfo::type_alias(name_b, vec![], TypeId::STRING));
    // Second def with name "A" (same name, different def).
    store.register(DefinitionInfo::interface(name_a, vec![], vec![]));

    let stats = store.statistics();
    // Two unique names: "A" and "B".
    assert_eq!(stats.name_to_defs_entries, 2);
}

#[test]
fn all_symbol_mappings_returns_registered_pairs() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    // Register a few definitions with symbol_id set
    let name_a = interner.intern_string("ClassA");
    let name_b = interner.intern_string("InterfaceB");

    let mut info_a = DefinitionInfo::type_alias(name_a, vec![], TypeId::NUMBER);
    info_a.kind = DefKind::Class;
    info_a.symbol_id = Some(10);
    info_a.file_id = Some(0);
    let def_a = store.register(info_a);
    store.register_symbol_mapping(10, 0, def_a);

    let mut info_b = DefinitionInfo::interface(name_b, vec![], vec![]);
    info_b.symbol_id = Some(20);
    info_b.file_id = Some(0);
    let def_b = store.register(info_b);
    store.register_symbol_mapping(20, 0, def_b);

    let mappings = store.all_symbol_mappings();
    assert_eq!(mappings.len(), 2, "should have 2 symbol mappings");

    let mapping_set: std::collections::HashSet<(u32, DefId)> = mappings.into_iter().collect();
    assert!(
        mapping_set.contains(&(10, def_a)),
        "should contain symbol 10 → def_a"
    );
    assert!(
        mapping_set.contains(&(20, def_b)),
        "should contain symbol 20 → def_b"
    );
}

#[test]
fn all_symbol_mappings_empty_store() {
    let store = DefinitionStore::new();
    let mappings = store.all_symbol_mappings();
    assert!(mappings.is_empty(), "empty store should return no mappings");
}

// =============================================================================
// from_semantic_defs factory tests
// =============================================================================

#[test]
fn from_semantic_defs_empty_map() {
    let defs = rustc_hash::FxHashMap::default();
    let store =
        DefinitionStore::from_semantic_defs(&defs, |s| tsz_common::interner::Atom(s.len() as u32));
    assert_eq!(store.statistics().total_definitions, 0);
}

#[test]
fn from_semantic_defs_creates_all_declaration_families() {
    use tsz_binder::{SemanticDefEntry, SemanticDefKind, SymbolId};

    let mut defs = rustc_hash::FxHashMap::default();

    let make_entry = |kind: SemanticDefKind, name: &str| SemanticDefEntry {
        kind,
        name: name.to_string(),
        file_id: 1,
        span_start: 0,
        type_param_count: 0,
        type_param_names: Vec::new(),
        is_exported: true,
        enum_member_names: Vec::new(),
        is_const: false,
        is_abstract: false,
        is_declare: false,
        extends_names: Vec::new(),
        implements_names: Vec::new(),
        parent_namespace: None,
        is_global_augmentation: false,
    };

    defs.insert(SymbolId(1), make_entry(SemanticDefKind::Class, "MyClass"));
    defs.insert(
        SymbolId(2),
        make_entry(SemanticDefKind::Interface, "MyIface"),
    );
    defs.insert(
        SymbolId(3),
        make_entry(SemanticDefKind::TypeAlias, "MyAlias"),
    );
    defs.insert(SymbolId(4), {
        let mut e = make_entry(SemanticDefKind::Enum, "MyEnum");
        e.enum_member_names = vec!["A".to_string(), "B".to_string()];
        e
    });
    defs.insert(SymbolId(5), make_entry(SemanticDefKind::Namespace, "MyNS"));
    defs.insert(SymbolId(6), make_entry(SemanticDefKind::Function, "myFunc"));
    defs.insert(SymbolId(7), make_entry(SemanticDefKind::Variable, "myVar"));

    // Use a simple hash-based interning for the test
    let names: std::cell::RefCell<Vec<String>> = std::cell::RefCell::new(Vec::new());
    let store = DefinitionStore::from_semantic_defs(&defs, |s| {
        let mut names = names.borrow_mut();
        let idx = names.len();
        names.push(s.to_string());
        tsz_common::interner::Atom(idx as u32)
    });

    // 7 declarations + 1 ClassConstructor companion = 8
    assert_eq!(
        store.statistics().total_definitions,
        8,
        "7 decls + 1 class constructor companion = 8"
    );

    // Verify each kind via symbol lookup
    assert!(store.find_def_by_symbol(1).is_some(), "Class should exist");
    assert!(
        store.find_def_by_symbol(2).is_some(),
        "Interface should exist"
    );
    assert!(
        store.find_def_by_symbol(3).is_some(),
        "TypeAlias should exist"
    );
    assert!(store.find_def_by_symbol(4).is_some(), "Enum should exist");
    assert!(
        store.find_def_by_symbol(5).is_some(),
        "Namespace should exist"
    );
    assert!(
        store.find_def_by_symbol(6).is_some(),
        "Function should exist"
    );
    assert!(
        store.find_def_by_symbol(7).is_some(),
        "Variable should exist"
    );

    // Verify class has constructor companion
    let class_def = store.find_def_by_symbol(1).unwrap();
    let ctor = store.get_constructor_def(class_def);
    assert!(
        ctor.is_some(),
        "Class should have ClassConstructor companion"
    );
    let ctor_info = store.get(ctor.unwrap()).unwrap();
    assert_eq!(ctor_info.kind, super::DefKind::ClassConstructor);

    // Verify enum has members
    let enum_def = store.find_def_by_symbol(4).unwrap();
    let enum_info = store.get(enum_def).unwrap();
    assert_eq!(enum_info.enum_members.len(), 2);
}

#[test]
fn from_semantic_defs_wires_namespace_exports() {
    use tsz_binder::{SemanticDefEntry, SemanticDefKind, SymbolId};

    let mut defs = rustc_hash::FxHashMap::default();

    // Namespace parent
    defs.insert(
        SymbolId(10),
        SemanticDefEntry {
            kind: SemanticDefKind::Namespace,
            name: "NS".to_string(),
            file_id: 1,
            span_start: 0,
            type_param_count: 0,
            type_param_names: Vec::new(),
            is_exported: true,
            enum_member_names: Vec::new(),
            is_const: false,
            is_abstract: false,
            is_declare: false,
            extends_names: Vec::new(),
            implements_names: Vec::new(),
            parent_namespace: None,
            is_global_augmentation: false,
        },
    );

    // Child inside namespace
    defs.insert(
        SymbolId(11),
        SemanticDefEntry {
            kind: SemanticDefKind::Interface,
            name: "Inner".to_string(),
            file_id: 1,
            span_start: 10,
            type_param_count: 0,
            type_param_names: Vec::new(),
            is_exported: true,
            enum_member_names: Vec::new(),
            is_const: false,
            is_abstract: false,
            is_declare: false,
            extends_names: Vec::new(),
            implements_names: Vec::new(),
            parent_namespace: Some(SymbolId(10)),
            is_global_augmentation: false,
        },
    );

    let store = DefinitionStore::from_semantic_defs(&defs, |s| {
        tsz_common::interner::Atom(
            s.bytes()
                .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32)),
        )
    });

    let ns_def = store.find_def_by_symbol(10).unwrap();
    let ns_info = store.get(ns_def).unwrap();
    assert_eq!(
        ns_info.exports.len(),
        1,
        "Namespace should have 1 export (Inner)"
    );
}

#[test]
fn from_semantic_defs_resolves_heritage() {
    use tsz_binder::{SemanticDefEntry, SemanticDefKind, SymbolId};

    let mut defs = rustc_hash::FxHashMap::default();

    // Parent class
    defs.insert(
        SymbolId(20),
        SemanticDefEntry {
            kind: SemanticDefKind::Class,
            name: "Base".to_string(),
            file_id: 1,
            span_start: 0,
            type_param_count: 0,
            type_param_names: Vec::new(),
            is_exported: true,
            enum_member_names: Vec::new(),
            is_const: false,
            is_abstract: false,
            is_declare: false,
            extends_names: Vec::new(),
            implements_names: Vec::new(),
            parent_namespace: None,
            is_global_augmentation: false,
        },
    );

    // Interface
    defs.insert(
        SymbolId(21),
        SemanticDefEntry {
            kind: SemanticDefKind::Interface,
            name: "Printable".to_string(),
            file_id: 1,
            span_start: 20,
            type_param_count: 0,
            type_param_names: Vec::new(),
            is_exported: true,
            enum_member_names: Vec::new(),
            is_const: false,
            is_abstract: false,
            is_declare: false,
            extends_names: Vec::new(),
            implements_names: Vec::new(),
            parent_namespace: None,
            is_global_augmentation: false,
        },
    );

    // Child class extending Base, implementing Printable
    defs.insert(
        SymbolId(22),
        SemanticDefEntry {
            kind: SemanticDefKind::Class,
            name: "Child".to_string(),
            file_id: 1,
            span_start: 40,
            type_param_count: 0,
            type_param_names: Vec::new(),
            is_exported: true,
            enum_member_names: Vec::new(),
            is_const: false,
            is_abstract: false,
            is_declare: false,
            extends_names: vec!["Base".to_string()],
            implements_names: vec!["Printable".to_string()],
            parent_namespace: None,
            is_global_augmentation: false,
        },
    );

    // Use a deterministic interning scheme for tests
    let store = DefinitionStore::from_semantic_defs(&defs, |s| {
        tsz_common::interner::Atom(
            s.bytes()
                .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32)),
        )
    });

    let base_def = store.find_def_by_symbol(20).unwrap();
    let printable_def = store.find_def_by_symbol(21).unwrap();
    let child_def = store.find_def_by_symbol(22).unwrap();

    let child_info = store.get(child_def).unwrap();

    assert_eq!(
        child_info.extends,
        Some(base_def),
        "Child.extends should point to Base"
    );
    assert!(
        child_info.implements.contains(&printable_def),
        "Child.implements should contain Printable"
    );
}

/// `TypeEnvironment::get_lazy_type_params` should fall back to the `DefinitionStore`
/// when type params are not in the local cache. This mirrors how `get_def` falls
/// back to the store for type bodies.
#[test]
fn test_type_environment_get_lazy_type_params_definition_store_fallback() {
    use crate::def::resolver::TypeEnvironment;
    use crate::def::resolver::TypeResolver;

    let interner = create_test_interner();
    let store = std::sync::Arc::new(DefinitionStore::new());

    // Register a type alias with type params in the store
    let name = interner.intern_string("Readonly");
    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let info = DefinitionInfo::type_alias(name, vec![t_param], TypeId::NUMBER);
    let def_id = store.register(info);

    // Create a TypeEnvironment with the store but DON'T insert params locally
    let mut env = TypeEnvironment::new();
    env.set_definition_store(store);

    // get_lazy_type_params should find the params via the DefinitionStore fallback
    let params = env.get_lazy_type_params(def_id);
    assert!(
        params.is_some(),
        "get_lazy_type_params should find params from DefinitionStore"
    );
    let params = params.unwrap();
    assert_eq!(params.len(), 1);
    assert_eq!(params[0].name, t_param.name);
}
