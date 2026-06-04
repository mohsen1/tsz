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
    assert_eq!(
        store.statistics().symbol_def_index_entries,
        7,
        "one composite symbol/file mapping per semantic definition"
    );
    assert_eq!(
        store.statistics().symbol_only_index_entries,
        7,
        "constructor companion shares the class symbol and must not add a second symbol-only entry"
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
    for symbol_id in 1..=7 {
        assert_eq!(
            store.lookup_by_symbol(symbol_id, 1),
            store.find_def_by_symbol(symbol_id),
            "semantic defs should keep composite and file-agnostic symbol lookups in sync"
        );
    }

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

#[test]
fn test_all_definition_names_qualifies_namespace_exports() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    let namespace_name = interner.intern_string("Intl");
    let export_name = interner.intern_string("NumberFormatOptions");
    let namespace = store.register(DefinitionInfo::namespace(namespace_name, Vec::new()));
    let exported = store.register(DefinitionInfo::interface(
        export_name,
        Vec::new(),
        Vec::new(),
    ));
    store.add_export(namespace, export_name, exported);

    let names: rustc_hash::FxHashMap<_, _> = store.all_definition_names().into_iter().collect();
    assert_eq!(
        names.get(&exported),
        Some(&vec![namespace_name, export_name])
    );
}

include!("def_tests_parts/type_to_def.rs");
