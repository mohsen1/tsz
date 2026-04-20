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

    let interner = tsz_solver::TypeInterner::new();
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

    let interner = tsz_solver::TypeInterner::new();
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

