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

