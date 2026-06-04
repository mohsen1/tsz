#[test]
fn class_with_heritage_preserves_identity_through_merge() {
    let files = vec![
        (
            "base.ts".to_string(),
            "export class Base { x: number; }".to_string(),
        ),
        (
            "derived.ts".to_string(),
            "export class Derived extends Base { y: string; }".to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let store = &program.definition_store;

    // Both classes should have DefIds
    let base_entry = program.semantic_defs.iter().find(|(_, e)| e.name == "Base");
    let derived_entry = program
        .semantic_defs
        .iter()
        .find(|(_, e)| e.name == "Derived");

    assert!(base_entry.is_some(), "Base should be in semantic_defs");
    assert!(
        derived_entry.is_some(),
        "Derived should be in semantic_defs"
    );

    let base_def = store
        .find_def_by_symbol(base_entry.unwrap().0.0)
        .expect("Base DefId");
    let derived_def = store
        .find_def_by_symbol(derived_entry.unwrap().0.0)
        .expect("Derived DefId");

    // Both should have constructor companions
    assert!(
        store.get_constructor_def(base_def).is_some(),
        "Base should have ClassConstructor companion"
    );
    assert!(
        store.get_constructor_def(derived_def).is_some(),
        "Derived should have ClassConstructor companion"
    );

    // Heritage should be wired (Derived extends Base)
    let derived_extends = store.get_extends(derived_def);
    assert_eq!(
        derived_extends,
        Some(base_def),
        "Derived should extend Base via heritage resolution"
    );
}

#[test]
fn stable_identity_all_families_survive_merge_pipeline() {
    // All top-level declaration families (class, interface, type alias, enum,
    // namespace, function, variable) should produce stable DefIds in the
    // pre-populated DefinitionStore after merge.
    let files = vec![(
        "decls.ts".to_string(),
        concat!(
            "export class MyClass<T> { value: T; }\n",
            "export interface MyInterface { x: number; }\n",
            "export type MyAlias = string | number;\n",
            "export enum MyEnum { A, B, C }\n",
            "export namespace MyNS { export const inner = 1; }\n",
            "export function myFunc(): void {}\n",
            "export const myVar: number = 42;\n",
        )
        .to_string(),
    )];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);
    let store = &program.definition_store;
    let interner = &program.type_interner;

    // Each declaration family should have a DefId in the store
    let families = [
        ("MyClass", tsz_solver::def::DefKind::Class),
        ("MyInterface", tsz_solver::def::DefKind::Interface),
        ("MyAlias", tsz_solver::def::DefKind::TypeAlias),
        ("MyEnum", tsz_solver::def::DefKind::Enum),
        ("MyNS", tsz_solver::def::DefKind::Namespace),
        ("myFunc", tsz_solver::def::DefKind::Function),
        ("myVar", tsz_solver::def::DefKind::Variable),
    ];

    for (name, expected_kind) in &families {
        let atom = interner.intern_string(name);
        let defs = store
            .find_defs_by_name(atom)
            .unwrap_or_else(|| panic!("{name} should have DefId(s) in DefinitionStore"));
        assert!(
            !defs.is_empty(),
            "{name} should have at least one DefId, got 0"
        );
        let info = store
            .get(defs[0])
            .unwrap_or_else(|| panic!("{name} DefId should have DefinitionInfo"));
        assert_eq!(
            info.kind, *expected_kind,
            "{name} should have kind {expected_kind:?}, got {:?}",
            info.kind
        );
        assert!(info.symbol_id.is_some(), "{name} should have symbol_id set");
        assert!(info.file_id.is_some(), "{name} should have file_id set");
        assert!(info.is_exported, "{name} should be marked as exported");
    }

    // Class should also have a ClassConstructor companion
    let class_atom = interner.intern_string("MyClass");
    let class_defs = store.find_defs_by_name(class_atom).unwrap();
    let class_def = class_defs
        .iter()
        .find(|d| store.get(**d).unwrap().kind == tsz_solver::def::DefKind::Class)
        .expect("MyClass should have a Class DefId");
    assert!(
        store.get_constructor_def(*class_def).is_some(),
        "MyClass should have a ClassConstructor companion DefId"
    );

    // Generic should have type_param_count preserved
    let class_info = store.get(*class_def).unwrap();
    assert_eq!(
        class_info.type_params.len(),
        1,
        "MyClass<T> should have 1 type param"
    );

    // Enum should have member names
    let enum_atom = interner.intern_string("MyEnum");
    let enum_defs = store.find_defs_by_name(enum_atom).unwrap();
    let enum_def = enum_defs
        .iter()
        .find(|d| store.get(**d).unwrap().kind == tsz_solver::def::DefKind::Enum)
        .expect("MyEnum should have an Enum DefId");
    let enum_info = store.get(*enum_def).unwrap();
    assert_eq!(
        enum_info.enum_members.len(),
        3,
        "MyEnum should have 3 members"
    );

    // Namespace export linkage: MyNS should have 'inner' as an export
    let ns_atom = interner.intern_string("MyNS");
    let ns_defs = store.find_defs_by_name(ns_atom).unwrap();
    let ns_def = ns_defs
        .iter()
        .find(|d| store.get(**d).unwrap().kind == tsz_solver::def::DefKind::Namespace)
        .expect("MyNS should have a Namespace DefId");
    let ns_exports = store.get_exports(*ns_def);
    assert!(
        ns_exports.is_some() && !ns_exports.as_ref().unwrap().is_empty(),
        "MyNS should have at least one export (inner)"
    );
}

#[test]
fn stable_identity_survives_rebind_same_source() {
    // Parsing+binding the same source twice should produce identical DefId
    // structure in the DefinitionStore (same count, same kinds, same names).
    let source = concat!(
        "export class Foo<T> extends Array<T> {}\n",
        "export interface Bar { x: number; }\n",
        "export type Baz = string;\n",
        "export enum Color { Red, Green }\n",
    );

    let make_program = || {
        let files = vec![("test.ts".to_string(), source.to_string())];
        let results = parse_and_bind_parallel(files);
        merge_bind_results(results)
    };

    let p1 = make_program();
    let p2 = make_program();

    // Both runs should produce the same number of semantic_defs
    assert_eq!(
        p1.semantic_defs.len(),
        p2.semantic_defs.len(),
        "semantic_defs count should be identical across rebinds"
    );

    // Both runs should produce the same set of names with same kinds
    let names1: std::collections::BTreeMap<String, crate::binder::SemanticDefKind> = p1
        .semantic_defs
        .values()
        .map(|e| (e.name.clone(), e.kind))
        .collect();
    let names2: std::collections::BTreeMap<String, crate::binder::SemanticDefKind> = p2
        .semantic_defs
        .values()
        .map(|e| (e.name.clone(), e.kind))
        .collect();
    assert_eq!(
        names1, names2,
        "semantic_def names+kinds should be identical across rebinds"
    );

    // Both DefinitionStores should have the same number of entries
    let count1 = p1.definition_store.all_symbol_mappings().len();
    let count2 = p2.definition_store.all_symbol_mappings().len();
    assert_eq!(
        count1, count2,
        "DefinitionStore symbol mapping counts should be identical across rebinds"
    );
}

#[test]
fn stable_identity_cross_file_merge_preserves_all_defs() {
    // Cross-file declaration merging: interface + class across files
    // Both should get DefIds and the interface heritage should be resolved.
    let files = vec![
        (
            "types.ts".to_string(),
            "export interface Base { x: number; }".to_string(),
        ),
        (
            "impl.ts".to_string(),
            concat!(
                "export class Derived { y: string; }\n",
                "export type Alias = string;\n",
                "export enum Status { OK, ERR }\n",
            )
            .to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);
    let store = &program.definition_store;
    let interner = &program.type_interner;

    // All four declarations should have DefIds
    for name in &["Base", "Derived", "Alias", "Status"] {
        let atom = interner.intern_string(name);
        let defs = store.find_defs_by_name(atom);
        assert!(
            defs.is_some() && !defs.as_ref().unwrap().is_empty(),
            "{name} should have DefId(s) in cross-file merge"
        );
    }

    // Both files should have per-file semantic_defs
    let file0_names: std::collections::HashSet<_> = program.files[0]
        .semantic_defs
        .values()
        .map(|e| e.name.as_str())
        .collect();
    let file1_names: std::collections::HashSet<_> = program.files[1]
        .semantic_defs
        .values()
        .map(|e| e.name.as_str())
        .collect();
    assert!(file0_names.contains("Base"), "types.ts should own Base");
    assert!(
        file1_names.contains("Derived"),
        "impl.ts should own Derived"
    );
    assert!(file1_names.contains("Alias"), "impl.ts should own Alias");
    assert!(file1_names.contains("Status"), "impl.ts should own Status");
}
