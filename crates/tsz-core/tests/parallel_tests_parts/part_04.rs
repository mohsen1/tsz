#[test]
fn cross_file_interface_heritage_survives_definition_store() {
    // The DefinitionInfo in the shared DefinitionStore should also have the
    // accumulated heritage names from both files.
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

    let (&foo_sym, _) = program
        .semantic_defs
        .iter()
        .find(|(_, e)| e.name == "Foo" && e.kind == crate::binder::SemanticDefKind::Interface)
        .expect("expected semantic def for Foo");

    let def_id = program
        .definition_store
        .find_def_by_symbol(foo_sym.0)
        .expect("Foo should have a DefId");

    let info = program
        .definition_store
        .get(def_id)
        .expect("Foo's DefinitionInfo should exist");

    assert!(
        info.heritage_names.contains(&"Bar".to_string()),
        "DefinitionInfo should have heritage 'Bar', got {:?}",
        info.heritage_names
    );
    assert!(
        info.heritage_names.contains(&"Baz".to_string()),
        "DefinitionInfo should have heritage 'Baz', got {:?}",
        info.heritage_names
    );
}

#[test]
fn cross_file_enum_members_accumulated_in_semantic_defs() {
    // When an enum is declared across two files (declaration merging), both
    // files' member names should be accumulated in the merged semantic_defs.
    let files = vec![
        ("a.ts".to_string(), "enum Color { Red, Green }".to_string()),
        (
            "b.ts".to_string(),
            "enum Color { Blue, Yellow }".to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let color_entry = program
        .semantic_defs
        .values()
        .find(|e| e.name == "Color" && e.kind == crate::binder::SemanticDefKind::Enum)
        .expect("expected semantic def for Color");

    let members = &color_entry.enum_member_names;
    assert!(
        members.contains(&"Red".to_string()),
        "Color should have member 'Red', got {members:?}"
    );
    assert!(
        members.contains(&"Green".to_string()),
        "Color should have member 'Green', got {members:?}"
    );
    assert!(
        members.contains(&"Blue".to_string()),
        "Color should have member 'Blue', got {members:?}"
    );
    assert!(
        members.contains(&"Yellow".to_string()),
        "Color should have member 'Yellow', got {members:?}"
    );
}

#[test]
fn cross_file_script_interfaces_merge_into_single_semantic_def() {
    // Both files are script files (no import/export statements) so their
    // top-level interface declarations share the global scope and merge.
    let files = vec![
        (
            "a.ts".to_string(),
            "interface Foo { x: number }".to_string(),
        ),
        (
            "b.ts".to_string(),
            "interface Foo { y: string }".to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    // Both declarations should merge into one semantic_def.
    let foo_entries: Vec<_> = program
        .semantic_defs
        .values()
        .filter(|e| e.name == "Foo" && e.kind == crate::binder::SemanticDefKind::Interface)
        .collect();
    assert_eq!(
        foo_entries.len(),
        1,
        "Should have exactly one merged semantic_def for Foo"
    );
}

#[test]
fn cross_file_type_param_arity_update_in_semantic_defs() {
    // If file A declares `interface Foo {}` (no type params) and file B
    // declares `interface Foo<T> {}`, the merged entry should have arity 1.
    let files = vec![
        (
            "a.ts".to_string(),
            "interface Foo { x: number }".to_string(),
        ),
        ("b.ts".to_string(), "interface Foo<T> { y: T }".to_string()),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let foo_entry = program
        .semantic_defs
        .values()
        .find(|e| e.name == "Foo" && e.kind == crate::binder::SemanticDefKind::Interface)
        .expect("expected semantic def for Foo");

    assert_eq!(
        foo_entry.type_param_count, 1,
        "Foo should have type_param_count=1 after cross-file merge with generic declaration"
    );
}

#[test]
fn cross_file_class_heritage_accumulated_in_semantic_defs() {
    // Classes can merge with interfaces across files. Heritage names from
    // both the class and interface declarations should be accumulated.
    let files = vec![
        (
            "a.ts".to_string(),
            "class Foo extends Base { x = 1 }".to_string(),
        ),
        (
            "b.ts".to_string(),
            "interface Foo extends Extra { y: string }".to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    // The merged symbol should have the Class kind (class takes precedence)
    let foo_entry = program
        .semantic_defs
        .values()
        .find(|e| e.name == "Foo")
        .expect("expected semantic def for Foo");

    assert!(
        foo_entry.heritage_names().contains(&"Base".to_string()),
        "Foo should have heritage 'Base' from class declaration, got {:?}",
        foo_entry.heritage_names()
    );
    assert!(
        foo_entry.heritage_names().contains(&"Extra".to_string()),
        "Foo should have heritage 'Extra' from interface merge, got {:?}",
        foo_entry.heritage_names()
    );
}

#[test]
fn cross_file_semantic_def_identity_stable_in_definition_store() {
    // Both files are script files (no import/export) so their top-level
    // interface declarations merge in the global scope. The merged identity
    // in DefinitionStore should reflect accumulated heritage from both files.
    let files = vec![
        (
            "a.ts".to_string(),
            "interface Widget extends Renderable { render(): void }".to_string(),
        ),
        (
            "b.ts".to_string(),
            "interface Widget extends Serializable { serialize(): string }".to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    // Should be exactly one semantic_def for Widget
    let widget_entries: Vec<_> = program
        .semantic_defs
        .iter()
        .filter(|(_, e)| e.name == "Widget")
        .collect();
    assert_eq!(
        widget_entries.len(),
        1,
        "Should have exactly one merged semantic_def for Widget, got {}",
        widget_entries.len()
    );

    let (&widget_sym, widget_entry) = widget_entries[0];
    assert!(
        widget_entry
            .heritage_names()
            .contains(&"Renderable".to_string()),
        "Widget heritage should include Renderable"
    );
    assert!(
        widget_entry
            .heritage_names()
            .contains(&"Serializable".to_string()),
        "Widget heritage should include Serializable"
    );

    // The DefinitionStore should have exactly one DefId for this symbol
    let def_id = program
        .definition_store
        .find_def_by_symbol(widget_sym.0)
        .expect("Widget should have a DefId in DefinitionStore");

    let info = program
        .definition_store
        .get(def_id)
        .expect("Widget's DefinitionInfo should exist");

    assert_eq!(info.kind, tsz_solver::def::DefKind::Interface);
    assert!(info.heritage_names.contains(&"Renderable".to_string()));
    assert!(info.heritage_names.contains(&"Serializable".to_string()));
}

// =============================================================================
// Heritage resolution at pre-populate time (Pass 3)
// =============================================================================

#[test]
fn heritage_resolution_wires_class_extends_in_definition_store() {
    // When a class extends another class and both are in semantic_defs,
    // pre_populate_definition_store should wire DefinitionInfo.extends.
    let files = vec![(
        "classes.ts".to_string(),
        "class Base {} class Derived extends Base {}".to_string(),
    )];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let base_sym = program
        .semantic_defs
        .iter()
        .find(|(_, e)| e.name == "Base")
        .map(|(&sym, _)| sym)
        .expect("Base should be in semantic_defs");
    let derived_sym = program
        .semantic_defs
        .iter()
        .find(|(_, e)| e.name == "Derived")
        .map(|(&sym, _)| sym)
        .expect("Derived should be in semantic_defs");

    let base_def = program
        .definition_store
        .find_def_by_symbol(base_sym.0)
        .expect("Base should have a DefId");
    let derived_def = program
        .definition_store
        .find_def_by_symbol(derived_sym.0)
        .expect("Derived should have a DefId");

    let derived_info = program
        .definition_store
        .get(derived_def)
        .expect("Derived DefinitionInfo should exist");
    assert_eq!(
        derived_info.extends,
        Some(base_def),
        "Derived.extends should point to Base's DefId"
    );
}

#[test]
fn heritage_resolution_wires_class_implements_in_definition_store() {
    // When a class implements interfaces, pre_populate should wire implements.
    let files = vec![(
        "impl.ts".to_string(),
        "interface IFoo {} interface IBar {} class Baz implements IFoo, IBar {}".to_string(),
    )];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let ifoo_sym = program
        .semantic_defs
        .iter()
        .find(|(_, e)| e.name == "IFoo")
        .map(|(&sym, _)| sym)
        .expect("IFoo should be in semantic_defs");
    let ibar_sym = program
        .semantic_defs
        .iter()
        .find(|(_, e)| e.name == "IBar")
        .map(|(&sym, _)| sym)
        .expect("IBar should be in semantic_defs");
    let baz_sym = program
        .semantic_defs
        .iter()
        .find(|(_, e)| e.name == "Baz")
        .map(|(&sym, _)| sym)
        .expect("Baz should be in semantic_defs");

    let ifoo_def = program
        .definition_store
        .find_def_by_symbol(ifoo_sym.0)
        .expect("IFoo DefId");
    let ibar_def = program
        .definition_store
        .find_def_by_symbol(ibar_sym.0)
        .expect("IBar DefId");
    let baz_def = program
        .definition_store
        .find_def_by_symbol(baz_sym.0)
        .expect("Baz DefId");

    let baz_info = program
        .definition_store
        .get(baz_def)
        .expect("Baz DefinitionInfo");
    assert!(
        baz_info.implements.contains(&ifoo_def),
        "Baz.implements should contain IFoo, got {:?}",
        baz_info.implements
    );
    assert!(
        baz_info.implements.contains(&ibar_def),
        "Baz.implements should contain IBar, got {:?}",
        baz_info.implements
    );
}

#[test]
fn heritage_resolution_skips_property_access_names() {
    // Heritage names like "ns.Base" contain dots and cannot be resolved by
    // simple name lookup. Pre-populate should leave extends as None.
    let files = vec![(
        "dotted.ts".to_string(),
        "namespace ns { export class Base {} } class Derived extends ns.Base {}".to_string(),
    )];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let derived_sym = program
        .semantic_defs
        .iter()
        .find(|(_, e)| e.name == "Derived")
        .map(|(&sym, _)| sym)
        .expect("Derived should be in semantic_defs");
    let derived_def = program
        .definition_store
        .find_def_by_symbol(derived_sym.0)
        .expect("Derived DefId");
    let derived_info = program
        .definition_store
        .get(derived_def)
        .expect("Derived DefinitionInfo");

    assert_eq!(
        derived_info.extends, None,
        "Dotted heritage names should not be resolved at pre-populate time"
    );
}

#[test]
fn heritage_resolution_survives_cross_file_merge() {
    // Heritage should be resolved even when class and its parent are in
    // different files (both are script files so they share the global scope).
    let files = vec![
        ("a.ts".to_string(), "class Parent {}".to_string()),
        (
            "b.ts".to_string(),
            "class Child extends Parent {}".to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let parent_sym = program
        .semantic_defs
        .iter()
        .find(|(_, e)| e.name == "Parent")
        .map(|(&sym, _)| sym)
        .expect("Parent should be in semantic_defs");
    let child_sym = program
        .semantic_defs
        .iter()
        .find(|(_, e)| e.name == "Child")
        .map(|(&sym, _)| sym)
        .expect("Child should be in semantic_defs");

    let parent_def = program
        .definition_store
        .find_def_by_symbol(parent_sym.0)
        .expect("Parent DefId");
    let child_def = program
        .definition_store
        .find_def_by_symbol(child_sym.0)
        .expect("Child DefId");

    let child_info = program
        .definition_store
        .get(child_def)
        .expect("Child DefinitionInfo");
    assert_eq!(
        child_info.extends,
        Some(parent_def),
        "Child.extends should point to Parent's DefId across files"
    );
}

#[test]
fn split_heritage_names_in_semantic_defs() {
    // Verify that extends_names and implements_names are split correctly
    // in the merged semantic_defs.
    let files = vec![(
        "split.ts".to_string(),
        "interface I {} class Base {} class Derived extends Base implements I {}".to_string(),
    )];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let derived = program
        .semantic_defs
        .values()
        .find(|e| e.name == "Derived")
        .expect("Derived should be in semantic_defs");

    assert_eq!(derived.extends_names, vec!["Base"]);
    assert_eq!(derived.implements_names, vec!["I"]);
    // Combined accessor should include both
    assert_eq!(derived.heritage_names(), vec!["Base", "I"]);
}

// =============================================================================
// Type parameter name identity through merge/rebind
// =============================================================================

#[test]
fn type_param_names_captured_for_all_generic_families() {
    // Verify that binder captures type parameter names for classes, interfaces,
    // type aliases, and functions — and that they survive merge into DefinitionStore.
    let files = vec![(
        "generics.ts".to_string(),
        r#"
            export class Container<T, U> {}
            export interface Mapper<In, Out> {}
            export type Pair<A, B> = [A, B];
            export function identity<X>(x: X): X { return x; }
            export enum Color { Red, Green, Blue }
            export namespace Utils {}
        "#
        .to_string(),
    )];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    // Helper: find semantic def by name and verify type param names
    let check = |name: &str, expected_names: &[&str]| {
        let entry = program
            .semantic_defs
            .values()
            .find(|e| e.name == name)
            .unwrap_or_else(|| panic!("{name} should be in semantic_defs"));
        assert_eq!(
            entry.type_param_count as usize,
            expected_names.len(),
            "{name}: type_param_count mismatch"
        );
        assert_eq!(
            entry.type_param_names,
            expected_names
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
            "{name}: type_param_names mismatch"
        );

        // Verify DefinitionStore also has real names (non-zero Atoms)
        let sym = program
            .semantic_defs
            .iter()
            .find(|(_, e)| e.name == name)
            .map(|(s, _)| s)
            .unwrap();
        let def_id = program
            .definition_store
            .find_def_by_symbol(sym.0)
            .unwrap_or_else(|| panic!("{name} should have DefId"));
        let info = program
            .definition_store
            .get(def_id)
            .unwrap_or_else(|| panic!("{name} should have DefinitionInfo"));
        assert_eq!(
            info.type_params.len(),
            expected_names.len(),
            "{name}: DefinitionInfo type_params count mismatch"
        );
        // Generic entries should have real interned names (Atom != 0).
        for (i, tp) in info.type_params.iter().enumerate() {
            assert_ne!(
                tp.name,
                tsz_common::interner::Atom(0),
                "{name}: type param {i} should have a real name, not Atom(0)"
            );
        }
    };

    check("Container", &["T", "U"]);
    check("Mapper", &["In", "Out"]);
    check("Pair", &["A", "B"]);
    check("identity", &["X"]);
    check("Color", &[]);
    check("Utils", &[]);
}

#[test]
fn type_param_names_survive_cross_file_merge() {
    // When a non-generic interface is first declared in file A and then
    // augmented with generics in file B, the merged entry should have the
    // names from file B.
    let files = vec![
        (
            "a.ts".to_string(),
            "interface Foo { x: number }".to_string(),
        ),
        (
            "b.ts".to_string(),
            "interface Foo<T, U> { y: T }".to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    let foo_entry = program
        .semantic_defs
        .values()
        .find(|e| e.name == "Foo")
        .expect("Foo should be in semantic_defs");

    assert_eq!(foo_entry.type_param_count, 2);
    assert_eq!(
        foo_entry.type_param_names,
        vec!["T".to_string(), "U".to_string()]
    );

    // Verify DefinitionStore entry has proper names
    let foo_sym = program
        .semantic_defs
        .iter()
        .find(|(_, e)| e.name == "Foo")
        .map(|(s, _)| s)
        .unwrap();
    let def_id = program
        .definition_store
        .find_def_by_symbol(foo_sym.0)
        .expect("Foo should have DefId");
    let info = program
        .definition_store
        .get(def_id)
        .expect("Foo DefinitionInfo");
    assert_eq!(info.type_params.len(), 2);
}

#[test]
fn type_param_names_stable_across_rebind() {
    // Verify that type param names survive the full cycle:
    // bind → merge → create_binder_from_bound_file → DefinitionStore lookup
    let files = vec![
        (
            "a.ts".to_string(),
            "export class Box<T> { value: T; }".to_string(),
        ),
        (
            "b.ts".to_string(),
            "export type Result<Ok, Err> = Ok | Err;".to_string(),
        ),
    ];
    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);

    // Record original DefIds
    let box_sym = program
        .semantic_defs
        .iter()
        .find(|(_, e)| e.name == "Box")
        .map(|(s, _)| *s)
        .expect("Box should exist");
    let result_sym = program
        .semantic_defs
        .iter()
        .find(|(_, e)| e.name == "Result")
        .map(|(s, _)| *s)
        .expect("Result should exist");

    let box_def = program
        .definition_store
        .find_def_by_symbol(box_sym.0)
        .expect("Box DefId");
    let result_def = program
        .definition_store
        .find_def_by_symbol(result_sym.0)
        .expect("Result DefId");

    // Reconstruct binders (as check_files_parallel does)
    let _binder_a = create_binder_from_bound_file(&program.files[0], &program, 0);
    let _binder_b = create_binder_from_bound_file(&program.files[1], &program, 1);

    // DefIds must be stable after rebind
    assert_eq!(
        program.definition_store.find_def_by_symbol(box_sym.0),
        Some(box_def),
        "Box DefId should be stable after rebind"
    );
    assert_eq!(
        program.definition_store.find_def_by_symbol(result_sym.0),
        Some(result_def),
        "Result DefId should be stable after rebind"
    );

    // Type param names should still be in the DefinitionStore
    let box_info = program.definition_store.get(box_def).unwrap();
    assert_eq!(
        box_info.type_params.len(),
        1,
        "Box should have 1 type param"
    );

    let result_info = program.definition_store.get(result_def).unwrap();
    assert_eq!(
        result_info.type_params.len(),
        2,
        "Result should have 2 type params"
    );
}

#[test]
fn single_file_definition_store_from_binder() {
    // Verify create_definition_store_from_binder produces a valid store
    // from a single binder's semantic_defs.
    use crate::parallel::create_definition_store_from_binder;

    let source = r#"
        export class MyClass<T> {}
        export interface MyInterface<A, B> {}
        export type MyAlias = string;
        export enum MyEnum { X, Y }
        export namespace MyNS {}
        export function myFunc<R>(x: R): R { return x; }
    "#;

    let parsed = crate::parallel::parse_file_single("test.ts".to_string(), source.to_string());
    let mut binder = crate::binder::BinderState::new();
    binder.bind_source_file(&parsed.arena, parsed.source_file);

    let interner = tsz_solver::construction::TypeInterner::new();
    let store = create_definition_store_from_binder(&binder, &interner);

    // All 6 top-level declarations should have DefIds
    let stats = store.statistics();
    assert!(
        stats.total_definitions >= 6,
        "Expected at least 6 definitions, got {}",
        stats.total_definitions
    );

    // Verify class has type param name
    let class_def = store
        .find_defs_by_name(interner.intern_string("MyClass"))
        .and_then(|defs| defs.first().copied());
    assert!(class_def.is_some(), "MyClass should have a DefId");
    let class_info = store.get(class_def.unwrap()).unwrap();
    assert_eq!(class_info.type_params.len(), 1);
    assert_eq!(class_info.kind, tsz_solver::def::DefKind::Class);

    // Verify interface has 2 type params
    let iface_def = store
        .find_defs_by_name(interner.intern_string("MyInterface"))
        .and_then(|defs| defs.first().copied());
    assert!(iface_def.is_some(), "MyInterface should have a DefId");
    let iface_info = store.get(iface_def.unwrap()).unwrap();
    assert_eq!(iface_info.type_params.len(), 2);
    assert_eq!(iface_info.kind, tsz_solver::def::DefKind::Interface);

    // Verify enum with members
    let enum_def = store
        .find_defs_by_name(interner.intern_string("MyEnum"))
        .and_then(|defs| defs.first().copied());
    assert!(enum_def.is_some(), "MyEnum should have a DefId");
    let enum_info = store.get(enum_def.unwrap()).unwrap();
    assert_eq!(enum_info.kind, tsz_solver::def::DefKind::Enum);
    assert_eq!(enum_info.enum_members.len(), 2);

    // Verify namespace
    let ns_def = store
        .find_defs_by_name(interner.intern_string("MyNS"))
        .and_then(|defs| defs.first().copied());
    assert!(ns_def.is_some(), "MyNS should have a DefId");
    let ns_info = store.get(ns_def.unwrap()).unwrap();
    assert_eq!(ns_info.kind, tsz_solver::def::DefKind::Namespace);
}

#[test]
fn definition_store_preserves_is_global_augmentation_flag() {
    // Verify that the is_global_augmentation flag from binder semantic_defs
    // flows through pre_populate_definition_store into DefinitionInfo.
    use crate::parallel::create_definition_store_from_binder;

    let source = r#"
export {};
declare global {
    interface AugmentedGlobal {
        foo: string;
    }
}
type LocalType = number;
"#;

    let parsed = crate::parallel::parse_file_single("test.ts".to_string(), source.to_string());
    let mut binder = crate::binder::BinderState::new();
    binder.bind_source_file(&parsed.arena, parsed.source_file);

    let interner = tsz_solver::construction::TypeInterner::new();
    let store = create_definition_store_from_binder(&binder, &interner);

    // AugmentedGlobal should have is_global_augmentation = true
    let aug_def = store
        .find_defs_by_name(interner.intern_string("AugmentedGlobal"))
        .and_then(|defs| defs.first().copied());
    assert!(aug_def.is_some(), "AugmentedGlobal should have a DefId");
    let aug_info = store.get(aug_def.unwrap()).unwrap();
    assert!(
        aug_info.is_global_augmentation,
        "declare global interface should have is_global_augmentation=true in DefinitionInfo"
    );

    // LocalType should have is_global_augmentation = false
    let local_def = store
        .find_defs_by_name(interner.intern_string("LocalType"))
        .and_then(|defs| defs.first().copied());
    assert!(local_def.is_some(), "LocalType should have a DefId");
    let local_info = store.get(local_def.unwrap()).unwrap();
    assert!(
        !local_info.is_global_augmentation,
        "regular type alias should have is_global_augmentation=false"
    );
}

#[test]
fn multi_file_merge_preserves_semantic_def_identity_across_files() {
    // Verify that semantic_defs from multiple files survive merge and produce
    // stable DefIds in the shared DefinitionStore.
    let files = vec![
        (
            "file_a.ts".to_string(),
            r"
export class Foo<T> { }
export interface IBar { x: number }
export type Baz = string;
"
            .to_string(),
        ),
        (
            "file_b.ts".to_string(),
            r"
export enum Color { Red, Green }
export namespace NS { export type Inner = number }
export function myFunc(): void { }
export const myVar: string = 'hello';
"
            .to_string(),
        ),
    ];

    let bind_results = parse_and_bind_parallel(files);
    let program = merge_bind_results(bind_results);

    // The merged program's definition_store should contain DefIds for all
    // top-level declarations from both files.
    let store = &program.definition_store;
    let interner = &program.type_interner;
    let stats = store.statistics();

    // At minimum: Foo, IBar, Baz, Color, NS, myFunc, myVar = 7 top-level defs
    // Plus NS.Inner as a namespace member
    assert!(
        stats.total_definitions >= 7,
        "Expected at least 7 definitions from 2-file merge, got {}",
        stats.total_definitions
    );

    // Verify each family has a DefId
    let has_def = |name: &str| -> bool {
        store
            .find_defs_by_name(interner.intern_string(name))
            .is_some()
    };
    assert!(has_def("Foo"), "class Foo should have DefId after merge");
    assert!(
        has_def("IBar"),
        "interface IBar should have DefId after merge"
    );
    assert!(
        has_def("Baz"),
        "type alias Baz should have DefId after merge"
    );
    assert!(has_def("Color"), "enum Color should have DefId after merge");
    assert!(has_def("NS"), "namespace NS should have DefId after merge");
    assert!(
        has_def("myFunc"),
        "function myFunc should have DefId after merge"
    );
    assert!(
        has_def("myVar"),
        "variable myVar should have DefId after merge"
    );

    // Verify DefKind correctness
    let get_kind = |name: &str| -> Option<tsz_solver::def::DefKind> {
        let defs: Vec<tsz_solver::def::DefId> =
            store.find_defs_by_name(interner.intern_string(name))?;
        let id = *defs.first()?;
        let info = store.get(id)?;
        Some(info.kind)
    };
    assert_eq!(get_kind("Foo"), Some(tsz_solver::def::DefKind::Class));
    assert_eq!(get_kind("IBar"), Some(tsz_solver::def::DefKind::Interface));
    assert_eq!(get_kind("Baz"), Some(tsz_solver::def::DefKind::TypeAlias));
    assert_eq!(get_kind("Color"), Some(tsz_solver::def::DefKind::Enum));
    assert_eq!(get_kind("NS"), Some(tsz_solver::def::DefKind::Namespace));
    assert_eq!(get_kind("myFunc"), Some(tsz_solver::def::DefKind::Function));
    assert_eq!(get_kind("myVar"), Some(tsz_solver::def::DefKind::Variable));
}

// =============================================================================
// Stable identity: heritage resolution survives merge/rebind
// =============================================================================

#[test]
fn heritage_extends_stable_after_merge_rebind() {
    // Class extends and interface extends should survive the full
    // bind → merge → rebind cycle with heritage wired in the store.
    let files = vec![
        (
            "a.ts".to_string(),
            r#"
                export class Base<T> { value: T; }
                export class Derived extends Base<string> { extra: number; }
                export interface IBase { x: number; }
                export interface IExtended extends IBase { y: string; }
            "#
            .to_string(),
        ),
        ("b.ts".to_string(), "export class Other {}".to_string()),
    ];

    let results = parse_and_bind_parallel(files);
    let program = merge_bind_results(results);
    let store = &program.definition_store;
    let interner = &program.type_interner;

    // Helper to find DefId by name
    let find_def = |name: &str| -> Option<tsz_solver::def::DefId> {
        let atom = interner.intern_string(name);
        store.find_defs_by_name(atom)?.into_iter().next()
    };

    let base_def = find_def("Base").expect("Base should have DefId");
    let derived_def = find_def("Derived").expect("Derived should have DefId");
    let ibase_def = find_def("IBase").expect("IBase should have DefId");
    let iextended_def = find_def("IExtended").expect("IExtended should have DefId");

    // Verify heritage was wired during pre-population
    let derived_info = store.get(derived_def).expect("Derived info");
    assert_eq!(
        derived_info.extends,
        Some(base_def),
        "Derived.extends should point to Base"
    );

    let iextended_info = store.get(iextended_def).expect("IExtended info");
    assert_eq!(
        iextended_info.extends,
        Some(ibase_def),
        "IExtended.extends should point to IBase"
    );

    // Reconstruct binders and verify DefIds are still resolvable.
    // Use program.semantic_defs (always populated) for symbol lookup.
    let _binder_a = create_binder_from_bound_file(&program.files[0], &program, 0);
    for (name, expected_def) in [
        ("Base", base_def),
        ("Derived", derived_def),
        ("IBase", ibase_def),
        ("IExtended", iextended_def),
    ] {
        let sym_id = program
            .semantic_defs
            .iter()
            .find(|(_, e)| e.name == name)
            .map(|(&id, _)| id)
            .unwrap_or_else(|| panic!("{name} should be in program semantic_defs"));
        let found = store.find_def_by_symbol(sym_id.0);
        assert_eq!(
            found,
            Some(expected_def),
            "{name}'s DefId should be stable after binder reconstruction"
        );
    }
}

#[test]
fn class_implements_stable_after_merge() {
    // Class implements should be wired during pre-population
    let files = vec![(
        "main.ts".to_string(),
        r#"
            export interface Serializable { serialize(): string; }
            export interface Cloneable { clone(): Cloneable; }
            export class Widget implements Serializable, Cloneable {
                serialize() { return ""; }
                clone() { return new Widget(); }
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

    let serializable_def = find_def("Serializable").expect("Serializable should have DefId");
    let cloneable_def = find_def("Cloneable").expect("Cloneable should have DefId");
    let widget_def = find_def("Widget").expect("Widget should have DefId");

    let widget_info = store.get(widget_def).expect("Widget info");
    assert!(
        widget_info.implements.contains(&serializable_def),
        "Widget.implements should contain Serializable"
    );
    assert!(
        widget_info.implements.contains(&cloneable_def),
        "Widget.implements should contain Cloneable"
    );
}

#[test]
fn generic_type_alias_identity_stable_across_merge_rebind() {
    // Generic type aliases should preserve type param count and names
    // through the merge/rebind cycle.
    let files = vec![
        (
            "types.ts".to_string(),
            r#"
                export type Pair<A, B> = { first: A; second: B; };
                export type Optional<T> = T | undefined;
            "#
            .to_string(),
        ),
        (
            "usage.ts".to_string(),
            "export type Id = string;".to_string(),
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

    // Verify type param arity and names
    let pair_def = find_def("Pair").expect("Pair should have DefId");
    let pair_info = store.get(pair_def).expect("Pair info");
    assert_eq!(
        pair_info.type_params.len(),
        2,
        "Pair should have 2 type params"
    );
    assert_eq!(
        pair_info.type_params[0].name,
        interner.intern_string("A"),
        "Pair's first type param should be 'A'"
    );
    assert_eq!(
        pair_info.type_params[1].name,
        interner.intern_string("B"),
        "Pair's second type param should be 'B'"
    );

    let optional_def = find_def("Optional").expect("Optional should have DefId");
    let optional_info = store.get(optional_def).expect("Optional info");
    assert_eq!(
        optional_info.type_params.len(),
        1,
        "Optional should have 1 type param"
    );
    assert_eq!(
        optional_info.type_params[0].name,
        interner.intern_string("T"),
        "Optional's type param should be 'T'"
    );

    // Non-generic type alias should have no type params
    let id_def = find_def("Id").expect("Id should have DefId");
    let id_info = store.get(id_def).expect("Id info");
    assert!(
        id_info.type_params.is_empty(),
        "Id should have no type params"
    );

    // Verify stable after rebind
    let _binder = create_binder_from_bound_file(&program.files[0], &program, 0);
    let pair_sym = program
        .semantic_defs
        .iter()
        .find(|(_, e)| e.name == "Pair")
        .map(|(&id, _)| id)
        .expect("Pair should be in program semantic_defs");
    assert_eq!(
        store.find_def_by_symbol(pair_sym.0),
        Some(pair_def),
        "Pair's DefId should be stable after rebind"
    );
}

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
    for (&sym_id, entry) in program.semantic_defs.iter() {
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

// =============================================================================
// is_declare flag through merge pipeline
// =============================================================================

