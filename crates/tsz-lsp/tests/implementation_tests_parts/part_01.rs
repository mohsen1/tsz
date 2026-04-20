#[test]
fn test_deep_inheritance_chain_interface() {
    // A -> B -> C: searching for A should find B (direct implementor), not C
    let source = "interface A {}\ninterface B extends A {}\ninterface C extends B {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Searching for A should find B (extends A directly)
    let pos = Position::new(0, 10);
    let result = provider.get_implementations(root, pos);

    assert!(result.is_some(), "Should find interfaces extending A");
    let locs = result.unwrap();
    assert_eq!(locs.len(), 1, "Should find only direct extender B");
    assert_eq!(locs[0].range.start.line, 1);
}

#[test]
fn test_class_implements_multiple_interfaces() {
    let source = "interface Readable {}\ninterface Writable {}\nclass Stream implements Readable, Writable {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Searching for Readable should find Stream
    let pos_readable = Position::new(0, 10);
    let result_readable = provider.get_implementations(root, pos_readable);
    assert!(
        result_readable.is_some(),
        "Should find implementors of Readable"
    );
    assert_eq!(result_readable.unwrap().len(), 1);

    // Searching for Writable should also find Stream
    let pos_writable = Position::new(1, 10);
    let result_writable = provider.get_implementations(root, pos_writable);
    assert!(
        result_writable.is_some(),
        "Should find implementors of Writable"
    );
    assert_eq!(result_writable.unwrap().len(), 1);
}

#[test]
fn test_interface_with_no_implementations_empty_body() {
    let source = "interface Empty {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 10);
    let result = provider.get_implementations(root, pos);

    assert!(
        result.is_none(),
        "Interface with no implementors should return None"
    );
}

#[test]
fn test_position_at_interface_keyword() {
    // Cursor at the "interface" keyword itself, before the name
    let source = "interface Foo {}\nclass Bar implements Foo {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "Foo" (col 10) should work
    let pos = Position::new(0, 10);
    let result = provider.get_implementations(root, pos);
    assert!(
        result.is_some(),
        "Should find implementations when cursor is on interface name"
    );
    assert_eq!(result.unwrap().len(), 1);
}

#[test]
fn test_empty_file_implementations() {
    let source = "";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 0);
    let result = provider.get_implementations(root, pos);

    assert!(
        result.is_none(),
        "Empty file should return None for implementations"
    );
}

#[test]
fn test_abstract_class_with_concrete_and_abstract_methods() {
    let source = "abstract class Base {\n  abstract go(): void;\n  stop() {}\n}\nclass Impl extends Base {\n  go() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "Base" in "abstract class Base"
    let pos = Position::new(0, 15);
    let result = provider.get_implementations(root, pos);

    assert!(
        result.is_some(),
        "Should find implementations of abstract class Base"
    );
    let locs = result.unwrap();
    assert_eq!(locs.len(), 1, "Should find one concrete implementor");
    assert_eq!(locs[0].range.start.line, 4);
}

#[test]
fn test_multiple_abstract_class_implementors() {
    let source = "abstract class Shape {\n  abstract area(): number;\n}\nclass Circle extends Shape {\n  area() { return 0; }\n}\nclass Rect extends Shape {\n  area() { return 0; }\n}\nclass Triangle extends Shape {\n  area() { return 0; }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 15);
    let result = provider.get_implementations(root, pos);

    assert!(result.is_some(), "Should find implementations of Shape");
    let locs = result.unwrap();
    assert_eq!(locs.len(), 3, "Should find three implementors");
}

#[test]
fn test_find_implementations_for_name_interface() {
    let source = "interface Runnable {}\nclass Worker implements Runnable {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let results = provider.find_implementations_for_name("Runnable", TargetKind::Interface);
    assert_eq!(results.len(), 1, "Should find Worker implementing Runnable");
    assert_eq!(results[0].name, "Worker");
}

#[test]
fn test_find_implementations_for_name_no_match() {
    let source = "interface Foo {}\nclass Bar {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let results = provider.find_implementations_for_name("Foo", TargetKind::Interface);
    assert!(
        results.is_empty(),
        "Bar does not implement Foo, should find nothing"
    );
}

#[test]
fn test_resolve_target_kind_for_interface() {
    let source = "interface MyInterface {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let kind = provider.resolve_target_kind_for_name("MyInterface");
    assert_eq!(kind, Some(TargetKind::Interface));
}

