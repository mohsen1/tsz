#[test]
fn enum_identity_with_members_stable_across_merge_rebind() {
    // Enums with members should have member names and const flag preserved
    let files = vec![
        (
            "enums.ts".to_string(),
            r#"
                export enum Color { Red, Green, Blue }
                export const enum Direction { Up, Down, Left, Right }
            "#
            .to_string(),
        ),
        (
            "other.ts".to_string(),
            "export enum Status { Active, Inactive }".to_string(),
        ),
    ];

    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);
    let store = &program.definition_store;
    let interner = &program.type_interner;

    let find_def = |name: &str| -> Option<tsz_solver::def::DefId> {
        let atom = interner.intern_string(name);
        store.find_defs_by_name(atom)?.into_iter().next()
    };

    // Color: regular enum with 3 members
    let color_def = find_def("Color").expect("Color should have DefId");
    let color_info = store.get(color_def).expect("Color info");
    assert_eq!(
        color_info.enum_members.len(),
        3,
        "Color should have 3 enum members"
    );
    assert!(!color_info.is_const, "Color should not be const");

    // Direction: const enum with 4 members
    let dir_def = find_def("Direction").expect("Direction should have DefId");
    let dir_info = store.get(dir_def).expect("Direction info");
    assert_eq!(
        dir_info.enum_members.len(),
        4,
        "Direction should have 4 enum members"
    );
    assert!(dir_info.is_const, "Direction should be const");

    // Verify stable after rebind. Use program.semantic_defs for symbol lookup.
    let _binder_a = create_binder_from_bound_file(&program.files[0], &program, 0);
    let color_sym = program
        .semantic_defs
        .iter()
        .find(|(_, e)| e.name == "Color")
        .map(|(&id, _)| id)
        .expect("Color in program semantic_defs");
    assert_eq!(
        store.find_def_by_symbol(color_sym.0),
        Some(color_def),
        "Color's DefId should be stable after rebind"
    );

    // Cross-file enum identity should also be stable
    let _binder_b = create_binder_from_bound_file(&program.files[1], &program, 1);
    let status_sym = program
        .semantic_defs
        .iter()
        .find(|(_, e)| e.name == "Status")
        .map(|(&id, _)| id)
        .expect("Status in program semantic_defs");
    let status_def = find_def("Status").expect("Status should have DefId");
    assert_eq!(
        store.find_def_by_symbol(status_sym.0),
        Some(status_def),
        "Status's DefId should be stable after rebind"
    );
}

#[test]
fn namespace_with_nested_declarations_stable_across_merge() {
    // Namespace members should be wired as exports and survive merge/rebind
    let files = vec![(
        "ns.ts".to_string(),
        r#"
            export namespace Shapes {
                export class Circle { radius: number; }
                export interface Drawable { draw(): void; }
                export type Point = { x: number; y: number; };
                export enum ShapeKind { Circle, Square }
            }
        "#
        .to_string(),
    )];

    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);
    let store = &program.definition_store;
    let interner = &program.type_interner;

    let find_def = |name: &str| -> Option<tsz_solver::def::DefId> {
        let atom = interner.intern_string(name);
        store.find_defs_by_name(atom)?.into_iter().next()
    };

    let ns_def = find_def("Shapes").expect("Shapes namespace should have DefId");
    let ns_info = store.get(ns_def).expect("Shapes info");

    // All 4 namespace members should be wired as exports
    assert!(
        ns_info.exports.len() >= 4,
        "Shapes should have at least 4 exports, got {}",
        ns_info.exports.len()
    );

    // Verify individual members exist as DefIds
    let circle_def = find_def("Circle").expect("Circle should have DefId");
    let drawable_def = find_def("Drawable").expect("Drawable should have DefId");

    // Verify they're in the namespace's exports
    let export_defs: Vec<tsz_solver::def::DefId> =
        ns_info.exports.iter().map(|(_, id)| *id).collect();
    assert!(
        export_defs.contains(&circle_def),
        "Shapes.exports should contain Circle"
    );
    assert!(
        export_defs.contains(&drawable_def),
        "Shapes.exports should contain Drawable"
    );

    // Verify stable after rebind. Use program.semantic_defs for symbol lookup.
    let _binder = create_binder_from_bound_file(&program.files[0], &program, 0);
    let ns_sym = program
        .semantic_defs
        .iter()
        .find(|(_, e)| e.name == "Shapes")
        .map(|(&id, _)| id)
        .expect("Shapes in program semantic_defs");
    assert_eq!(
        store.find_def_by_symbol(ns_sym.0),
        Some(ns_def),
        "Shapes' DefId should be stable after rebind"
    );
}

// =============================================================================
// ClassConstructor Companion Pre-population Tests
// =============================================================================

#[test]
fn class_constructor_companion_created_during_merge() {
    let files = vec![("a.ts".to_string(), "export class Foo {}".to_string())];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let stats = program.definition_store.statistics();
    assert!(
        stats.classes >= 1,
        "expected at least 1 class def, got {}",
        stats.classes
    );
    assert!(
        stats.class_constructors >= 1,
        "expected at least 1 ClassConstructor companion, got {}",
        stats.class_constructors
    );
    assert!(
        stats.class_to_constructor_entries >= 1,
        "expected class_to_constructor index entry, got {}",
        stats.class_to_constructor_entries
    );
}

#[test]
fn class_constructor_companion_has_correct_name_and_kind() {
    let files = vec![(
        "a.ts".to_string(),
        "export class Widget<T> { value: T; }".to_string(),
    )];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    // Find the class DefId
    let class_sym = program
        .semantic_defs
        .iter()
        .find(|(_, e)| e.name == "Widget")
        .map(|(&id, _)| id)
        .expect("Widget should be in semantic_defs");

    let class_def = program
        .definition_store
        .find_def_by_symbol(class_sym.0)
        .expect("Widget should have a DefId");

    let class_info = program
        .definition_store
        .get(class_def)
        .expect("Widget DefId should have info");
    assert_eq!(
        class_info.kind,
        tsz_solver::def::DefKind::Class,
        "Widget should be DefKind::Class"
    );

    // Check the constructor companion
    let ctor_def = program
        .definition_store
        .get_constructor_def(class_def)
        .expect("Widget should have a ClassConstructor companion");

    let ctor_info = program
        .definition_store
        .get(ctor_def)
        .expect("Constructor DefId should have info");
    assert_eq!(
        ctor_info.kind,
        tsz_solver::def::DefKind::ClassConstructor,
        "Companion should be DefKind::ClassConstructor"
    );
    // Constructor companion should share the same symbol_id
    assert_eq!(
        ctor_info.symbol_id, class_info.symbol_id,
        "Constructor companion should share the class's symbol_id"
    );
    // Body should be None (filled lazily by checker)
    assert!(
        ctor_info.body.is_none(),
        "Pre-populated constructor body should be None (lazy)"
    );
}

#[test]
fn class_constructor_companion_multifile() {
    let files = vec![
        ("a.ts".to_string(), "export class Alpha {}".to_string()),
        (
            "b.ts".to_string(),
            "export class Beta<T> extends Object {}".to_string(),
        ),
        (
            "c.ts".to_string(),
            "export abstract class Gamma {}".to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let stats = program.definition_store.statistics();
    assert!(
        stats.classes >= 3,
        "expected at least 3 class defs, got {}",
        stats.classes
    );
    assert!(
        stats.class_constructors >= 3,
        "expected at least 3 ClassConstructor companions, got {}",
        stats.class_constructors
    );
    assert!(
        stats.class_to_constructor_entries >= 3,
        "expected at least 3 class_to_constructor entries, got {}",
        stats.class_to_constructor_entries
    );
}

#[test]
fn class_constructor_companion_survives_binder_reconstruction() {
    let files = vec![
        (
            "a.ts".to_string(),
            "export class Foo { x: number; }".to_string(),
        ),
        (
            "b.ts".to_string(),
            "export class Bar<T> { value: T; }".to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    // Capture DefIds before reconstruction
    let foo_sym = program
        .semantic_defs
        .iter()
        .find(|(_, e)| e.name == "Foo")
        .map(|(&id, _)| id)
        .expect("Foo in semantic_defs");
    let foo_def = program
        .definition_store
        .find_def_by_symbol(foo_sym.0)
        .expect("Foo DefId");
    let foo_ctor = program
        .definition_store
        .get_constructor_def(foo_def)
        .expect("Foo constructor companion");

    // Reconstruct binders (simulates what check_files_parallel does)
    let _binder_a = create_binder_from_bound_file(&program.files[0], &program, 0);
    let _binder_b = create_binder_from_bound_file(&program.files[1], &program, 1);

    // After reconstruction, the class_to_constructor mapping should still work
    assert_eq!(
        program.definition_store.get_constructor_def(foo_def),
        Some(foo_ctor),
        "ClassConstructor companion should survive binder reconstruction"
    );

    // The DefId should still resolve
    let ctor_info = program.definition_store.get(foo_ctor);
    assert!(
        ctor_info.is_some(),
        "Constructor DefId should still have info after reconstruction"
    );
}

// =============================================================================
// Multi-file Identity Stability Tests (all declaration families)
// =============================================================================

#[test]
fn multifile_identity_all_families_survive_merge_rebind() {
    let files = vec![
        (
            "a.ts".to_string(),
            r#"
export class MyClass<T> { value: T; }
export interface MyInterface { x: number; }
export type MyAlias = string | number;
"#
            .to_string(),
        ),
        (
            "b.ts".to_string(),
            r#"
export enum MyEnum { A, B, C }
export namespace MyNS { export type T = number; }
export function myFunc(): void {}
export const myVar = 42;
"#
            .to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let store = &program.definition_store;

    // Collect all semantic def names and their DefIds before reconstruction
    let mut pre_reconstruct: Vec<(String, tsz_solver::def::DefId)> = Vec::new();
    for (&sym_id, entry) in &program.semantic_defs {
        if let Some(def_id) = store.find_def_by_symbol(sym_id.0) {
            pre_reconstruct.push((entry.name.clone(), def_id));
        }
    }

    // Verify all 7 families are represented
    let expected_names = [
        "MyClass",
        "MyInterface",
        "MyAlias",
        "MyEnum",
        "MyNS",
        "myFunc",
        "myVar",
    ];
    for name in &expected_names {
        assert!(
            pre_reconstruct.iter().any(|(n, _)| n == name),
            "{name} should have a DefId in the store"
        );
    }

    // Reconstruct binders for both files
    let binder_a = create_binder_from_bound_file(&program.files[0], &program, 0);
    let binder_b = create_binder_from_bound_file(&program.files[1], &program, 1);

    // Verify all semantic_defs in reconstructed binders still have DefIds
    for binder in [&binder_a, &binder_b] {
        for &sym_id in binder.semantic_defs.keys() {
            let def_id = store.find_def_by_symbol(sym_id.0);
            assert!(
                def_id.is_some(),
                "Reconstructed SymbolId({}) should still have DefId in shared store",
                sym_id.0
            );
        }
    }

    // Verify DefIds didn't change
    for (name, original_def) in &pre_reconstruct {
        let current_sym = program
            .semantic_defs
            .iter()
            .find(|(_, e)| e.name == *name)
            .map(|(&id, _)| id);
        if let Some(sym_id) = current_sym {
            let current_def = store.find_def_by_symbol(sym_id.0);
            assert_eq!(
                current_def,
                Some(*original_def),
                "{name}: DefId should be stable across reconstruction"
            );
        }
    }
}

#[test]
fn interface_merge_across_files_preserves_identity() {
    // Interface declaration merging: same interface in two files
    let files = vec![
        (
            "a.ts".to_string(),
            "export interface Merged { x: number; }".to_string(),
        ),
        (
            "b.ts".to_string(),
            "export interface Merged { y: string; }".to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let store = &program.definition_store;

    // After merge, 'Merged' should exist as a single logical entity
    // in semantic_defs (merged under one SymbolId)
    let merged_entries: Vec<_> = program
        .semantic_defs
        .iter()
        .filter(|(_, e)| e.name == "Merged")
        .collect();

    assert!(
        !merged_entries.is_empty(),
        "Merged interface should be in semantic_defs"
    );

    // Each semantic_def entry should have a DefId
    for (sym_id, _) in &merged_entries {
        let def_id = store.find_def_by_symbol(sym_id.0);
        assert!(
            def_id.is_some(),
            "Merged interface SymbolId({}) should have DefId",
            sym_id.0
        );

        // Verify it's DefKind::Interface
        if let Some(def_id) = def_id {
            let kind = store.get_kind(def_id);
            assert_eq!(
                kind,
                Some(tsz_solver::def::DefKind::Interface),
                "Merged interface should be DefKind::Interface"
            );
        }
    }
}

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

// =============================================================================
// Stable identity through merge/rebind for all declaration families
// =============================================================================

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

