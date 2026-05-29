#[test]
fn test_find_references_property_access_on_typed_var() {
    // When cursor is on the right side of a property access (d.prop1), references
    // should include the class member declaration and all access sites.
    let source = "class D {\n    prop1: string;\n}\nvar d: D;\nd.prop1;";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs =
        FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Cursor on "prop1" in "d.prop1" (line 4, col 2)
    let refs = find_refs.find_references(root, Position::new(4, 2));
    assert!(
        refs.is_some(),
        "Should find references when cursor is on the member name side of a property access"
    );
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find D.prop1 declaration + d.prop1 usage, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_property_access_different_member_name() {
    // Verify the fix is not hardcoded to a specific member name.
    let source = "class Widget {\n    render(): void {}\n}\nvar w: Widget;\nw.render();";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs =
        FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Cursor on "render" in "w.render()" (line 4, col 2)
    let refs = find_refs.find_references(root, Position::new(4, 2));
    assert!(
        refs.is_some(),
        "Should find references for render() when cursor is on member name in property access"
    );
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find Widget.render declaration + w.render usage, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_property_access_circular_class_inheritance() {
    // Mirrors documentHighlightAtInheritedProperties6: circular class extends.
    // Cursor on d.prop1 should resolve to D.prop1 (the declared type of d),
    // not C.prop1, because C and D are separate class symbols with separate members.
    let source = "class C extends D {\n    prop1: string;\n}\nclass D extends C {\n    prop1: string;\n}\nvar d: D;\nd.prop1;";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs =
        FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Cursor on "prop1" in "d.prop1" (line 7, col 2)
    let refs = find_refs.find_references(root, Position::new(7, 2));
    assert!(
        refs.is_some(),
        "Should find references for prop1 in d.prop1 even with circular class inheritance"
    );
    let refs = refs.unwrap();
    // D.prop1 declaration + d.prop1 usage (C.prop1 is a separate symbol and should not appear)
    assert!(
        refs.len() >= 2,
        "Should find D.prop1 + d.prop1 references, got {}",
        refs.len()
    );
}
