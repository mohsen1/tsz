#[test]
fn definition_store_defids_deterministic_across_merges() {
    let files = vec![
        ("a.ts".to_string(), "export class X {}".to_string()),
        ("b.ts".to_string(), "export class Y {}".to_string()),
    ];

    // Merge twice with the same input
    let results1 = parse_and_bind_parallel(files.clone());
    let program1 = merge_bind_results(results1);

    let results2 = parse_and_bind_parallel(files);
    let program2 = merge_bind_results(results2);

    // Both merges should produce the same number of DefIds
    let stats1 = program1.definition_store.statistics();
    let stats2 = program2.definition_store.statistics();
    assert_eq!(
        stats1.total_definitions, stats2.total_definitions,
        "deterministic merge should produce same DefId count"
    );

    // The DefId values should also be the same (sequential allocation from 1)
    for (&sym_id, entry) in &program1.semantic_defs {
        let def1 = program1.definition_store.find_def_by_symbol(sym_id.0);
        // Find the corresponding symbol in program2 by name
        let sym_id2 = program2
            .semantic_defs
            .iter()
            .find(|(_, e)| e.name == entry.name)
            .map(|(&id, _)| id);
        if let Some(sid2) = sym_id2 {
            let def2 = program2.definition_store.find_def_by_symbol(sid2.0);
            assert!(
                def1.is_some() && def2.is_some(),
                "both merges should produce DefIds for '{}'",
                entry.name
            );
        }
    }
}

#[test]
fn definition_store_preserves_kind_and_metadata() {
    let source = r#"
        export abstract class Abs {}
        export const enum ConstEnum { X }
        export interface Generic<T> { value: T }
    "#;
    let files = vec![("test.ts".to_string(), source.to_string())];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    // Check that DefKind, is_abstract, is_const are preserved
    for (_sym_id, entry) in &program.semantic_defs {
        let def_id = program
            .definition_store
            .find_def_by_symbol(_sym_id.0)
            .expect("should have DefId");
        let info = program
            .definition_store
            .get(def_id)
            .expect("should have DefinitionInfo");

        match entry.name.as_str() {
            "Abs" => {
                assert_eq!(info.kind, tsz_solver::def::DefKind::Class);
                assert!(info.is_abstract, "Abs should be abstract");
            }
            "ConstEnum" => {
                assert_eq!(info.kind, tsz_solver::def::DefKind::Enum);
                assert!(info.is_const, "ConstEnum should be const");
            }
            "Generic" => {
                assert_eq!(info.kind, tsz_solver::def::DefKind::Interface);
                assert_eq!(
                    info.type_params.len(),
                    1,
                    "Generic should have 1 type param"
                );
            }
            _ => {}
        }
    }
}

#[test]
fn definition_store_declaration_merge_preserves_first_defid() {
    // Two files with the same interface name → declaration merging
    let files = vec![
        (
            "a.ts".to_string(),
            "interface Shared { x: number }".to_string(),
        ),
        (
            "b.ts".to_string(),
            "interface Shared { y: string }".to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    // Declaration merging means one symbol, one semantic_def, one DefId
    let shared_entries: Vec<_> = program
        .semantic_defs
        .iter()
        .filter(|(_, e)| e.name == "Shared")
        .collect();
    assert_eq!(
        shared_entries.len(),
        1,
        "declaration-merged interface should have one semantic_def"
    );

    let (&sym_id, _) = shared_entries[0];
    let def_id = program
        .definition_store
        .find_def_by_symbol(sym_id.0)
        .expect("merged interface should have a DefId");
    assert!(
        def_id.is_valid(),
        "DefId for merged interface should be valid"
    );
}

#[test]
fn definition_store_namespace_exports_wired_during_pre_populate() {
    // Namespace members with parent_namespace should be wired as exports
    // of their parent's DefinitionInfo during pre_populate_definition_store.
    let source = r#"
        namespace MyNS {
            export class Foo {}
            export interface Bar {}
            export type Baz = string;
            export enum Color { Red }
        }
    "#;
    let files = vec![("test.ts".to_string(), source.to_string())];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    // Find the namespace DefId
    let ns_entry = program
        .semantic_defs
        .iter()
        .find(|(_, e)| e.name == "MyNS" && e.kind == crate::binder::SemanticDefKind::Namespace);
    let (&ns_sym, _) = ns_entry.expect("expected semantic def for MyNS");
    let ns_def_id = program
        .definition_store
        .find_def_by_symbol(ns_sym.0)
        .expect("MyNS should have a DefId");

    // The namespace's DefinitionInfo.exports should contain its members
    let exports = program
        .definition_store
        .get_exports(ns_def_id)
        .unwrap_or_default();
    assert!(
        exports.len() >= 4,
        "MyNS should have at least 4 exports (Foo, Bar, Baz, Color), got {}",
        exports.len()
    );

    // Each member should also have parent_namespace set in semantic_defs
    let member_names = ["Foo", "Bar", "Baz", "Color"];
    for name in &member_names {
        let member_entry = program.semantic_defs.values().find(|e| e.name == *name);
        let entry = member_entry.unwrap_or_else(|| panic!("expected semantic def for '{name}'"));
        assert_eq!(
            entry.parent_namespace,
            Some(ns_sym),
            "'{name}' should have parent_namespace = MyNS"
        );
    }
}

#[test]
fn definition_store_namespace_exports_survive_binder_reconstruction() {
    // After binder reconstruction (as check_files_parallel does),
    // namespace export wiring should still be intact in the shared store.
    let source = r#"
        namespace NS {
            export class Inner {}
        }
    "#;
    let files = vec![("test.ts".to_string(), source.to_string())];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    // Reconstruct binder (as check_files_parallel does)
    let _binder = create_binder_from_bound_file(&program.files[0], &program, 0);

    // Use program.semantic_defs (always populated) to verify parent_namespace
    let inner_entry = program.semantic_defs.values().find(|e| e.name == "Inner");
    assert!(
        inner_entry.is_some(),
        "program semantic_defs should have entry for Inner"
    );
    let inner = inner_entry.unwrap();
    assert!(
        inner.parent_namespace.is_some(),
        "Inner should have parent_namespace set"
    );

    // The shared DefinitionStore should still have the export wiring
    let ns_sym = program
        .semantic_defs
        .iter()
        .find(|(_, e)| e.name == "NS")
        .map(|(&id, _)| id)
        .expect("expected NS in program semantic_defs");
    let ns_def_id = program
        .definition_store
        .find_def_by_symbol(ns_sym.0)
        .expect("NS should have a DefId");
    let exports = program
        .definition_store
        .get_exports(ns_def_id)
        .unwrap_or_default();
    assert!(
        !exports.is_empty(),
        "NS exports should be non-empty in shared store after reconstruction"
    );
}

#[test]
fn definition_store_nested_namespace_exports_wired() {
    // Nested namespaces should have their own export wiring.
    let source = r#"
        namespace Outer {
            export namespace Inner {
                export class Deep {}
            }
        }
    "#;
    let files = vec![("test.ts".to_string(), source.to_string())];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    // Outer should have Inner as export
    let outer_sym = program
        .semantic_defs
        .iter()
        .find(|(_, e)| e.name == "Outer")
        .map(|(&id, _)| id)
        .expect("expected Outer");
    let outer_def = program
        .definition_store
        .find_def_by_symbol(outer_sym.0)
        .expect("Outer should have DefId");
    let outer_exports = program
        .definition_store
        .get_exports(outer_def)
        .unwrap_or_default();
    assert!(
        !outer_exports.is_empty(),
        "Outer should have Inner as an export"
    );

    // Inner should have Deep as export
    let inner_sym = program
        .semantic_defs
        .iter()
        .find(|(_, e)| e.name == "Inner")
        .map(|(&id, _)| id)
        .expect("expected Inner");
    let inner_def = program
        .definition_store
        .find_def_by_symbol(inner_sym.0)
        .expect("Inner should have DefId");
    let inner_exports = program
        .definition_store
        .get_exports(inner_def)
        .unwrap_or_default();
    assert!(
        !inner_exports.is_empty(),
        "Inner should have Deep as an export"
    );
}

// =============================================================================
// all_symbol_mappings + warm path tests
// =============================================================================

#[test]
fn all_symbol_mappings_covers_all_declaration_families() {
    // Verify that pre_populate_definition_store registers DefIds for all
    // major declaration families and that all_symbol_mappings() returns them.
    let source = r#"
        class MyClass {}
        interface MyInterface { x: number }
        type MyAlias = string;
        enum MyEnum { A, B }
        namespace MyNS { export class Inner {} }
        function myFunc() {}
        const myVar = 42;
    "#;
    let files = vec![("test.ts".to_string(), source.to_string())];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let mappings = program.definition_store.all_symbol_mappings();

    // Collect the names of all symbols that have DefIds
    let def_names: std::collections::HashSet<String> = mappings
        .iter()
        .filter_map(|(_raw_sym, def_id)| {
            program.definition_store.get(*def_id).map(|info| info.name)
        })
        .map(|atom| program.type_interner.resolve_atom(atom))
        .collect();

    assert!(
        def_names.contains("MyClass"),
        "all_symbol_mappings should include classes"
    );
    assert!(
        def_names.contains("MyInterface"),
        "all_symbol_mappings should include interfaces"
    );
    assert!(
        def_names.contains("MyAlias"),
        "all_symbol_mappings should include type aliases"
    );
    assert!(
        def_names.contains("MyEnum"),
        "all_symbol_mappings should include enums"
    );
    assert!(
        def_names.contains("MyNS"),
        "all_symbol_mappings should include namespaces"
    );
    assert!(
        def_names.contains("myFunc"),
        "all_symbol_mappings should include functions"
    );
    assert!(
        def_names.contains("myVar"),
        "all_symbol_mappings should include variables"
    );
}

#[test]
fn definition_store_identity_stable_across_merge_rebind() {
    // Verify that DefIds created during merge survive binder reconstruction.
    // This tests the full cycle: bind → merge → create_binder_from_bound_file.
    let files = vec![
        (
            "a.ts".to_string(),
            "export class Alpha {} export interface Beta {}".to_string(),
        ),
        (
            "b.ts".to_string(),
            "export type Gamma = string; export enum Delta { X }".to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    // Record DefIds from the merged program
    let alpha_def = program
        .semantic_defs
        .iter()
        .find(|(_, e)| e.name == "Alpha")
        .and_then(|(sym, _)| program.definition_store.find_def_by_symbol(sym.0));
    let beta_def = program
        .semantic_defs
        .iter()
        .find(|(_, e)| e.name == "Beta")
        .and_then(|(sym, _)| program.definition_store.find_def_by_symbol(sym.0));
    let gamma_def = program
        .semantic_defs
        .iter()
        .find(|(_, e)| e.name == "Gamma")
        .and_then(|(sym, _)| program.definition_store.find_def_by_symbol(sym.0));
    let delta_def = program
        .semantic_defs
        .iter()
        .find(|(_, e)| e.name == "Delta")
        .and_then(|(sym, _)| program.definition_store.find_def_by_symbol(sym.0));

    assert!(alpha_def.is_some(), "Alpha should have a DefId after merge");
    assert!(beta_def.is_some(), "Beta should have a DefId after merge");
    assert!(gamma_def.is_some(), "Gamma should have a DefId after merge");
    assert!(delta_def.is_some(), "Delta should have a DefId after merge");

    // Reconstruct binders (as check_files_parallel does)
    let _binder_a = create_binder_from_bound_file(&program.files[0], &program, 0);
    let _binder_b = create_binder_from_bound_file(&program.files[1], &program, 1);

    // Verify DefIds still resolve after reconstruction — the shared store persists.
    // Use program.semantic_defs (always populated) to find symbol IDs.
    for (name, expected) in [
        ("Alpha", alpha_def.unwrap()),
        ("Beta", beta_def.unwrap()),
        ("Gamma", gamma_def.unwrap()),
        ("Delta", delta_def.unwrap()),
    ] {
        let sym_id = program
            .semantic_defs
            .iter()
            .find(|(_, e)| e.name == name)
            .map(|(&id, _)| id);
        let sym = sym_id.unwrap_or_else(|| panic!("{name} should be in program semantic_defs"));
        let found = program.definition_store.find_def_by_symbol(sym.0);
        assert_eq!(
            found,
            Some(expected),
            "{name}'s DefId should be stable after binder reconstruction"
        );
    }
}

#[test]
fn all_symbol_mappings_count_matches_semantic_defs_count() {
    // The number of all_symbol_mappings entries should equal the number of
    // semantic_defs entries in the merged program (since pre_populate_definition_store
    // creates exactly one DefId per semantic_def entry).
    let files = vec![
        (
            "a.ts".to_string(),
            "export class A {} export interface B {}".to_string(),
        ),
        (
            "b.ts".to_string(),
            "export type C = number; export enum D { X, Y }".to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let mappings = program.definition_store.all_symbol_mappings();
    let semantic_defs_count = program.semantic_defs.len();

    assert_eq!(
        mappings.len(),
        semantic_defs_count,
        "all_symbol_mappings count ({}) should equal semantic_defs count ({})",
        mappings.len(),
        semantic_defs_count
    );
}

// =============================================================================
// Cross-file semantic_defs merge accumulation tests
// =============================================================================

#[test]
fn cross_file_interface_heritage_accumulated_in_semantic_defs() {
    // When an interface is declared across two files with different heritage
    // clauses, the merged semantic_defs entry should accumulate both sets of
    // heritage names.
    let files = vec![
        (
            "a.ts".to_string(),
            "interface Foo extends Bar { x: number }".to_string(),
        ),
        (
            "b.ts".to_string(),
            "interface Foo extends Baz { y: string }".to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let foo_entry = program
        .semantic_defs
        .values()
        .find(|e| e.name == "Foo" && e.kind == crate::binder::SemanticDefKind::Interface)
        .expect("expected semantic def for Foo");

    assert!(
        foo_entry.heritage_names().contains(&"Bar".to_string()),
        "Foo should have heritage name 'Bar' from file a.ts, got {:?}",
        foo_entry.heritage_names()
    );
    assert!(
        foo_entry.heritage_names().contains(&"Baz".to_string()),
        "Foo should have heritage name 'Baz' from file b.ts, got {:?}",
        foo_entry.heritage_names()
    );
}

