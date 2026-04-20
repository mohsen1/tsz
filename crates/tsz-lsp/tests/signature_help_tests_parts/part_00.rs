#[test]
fn test_signature_help_simple() {
    // function add(x: number, y: number): number { return x + y; }
    // add(1, 2|);
    let source = "function add(x: number, y: number): number { return x + y; }\nadd(1, 2);";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let interner = TypeInterner::new();
    let line_map = LineMap::build(source);

    let provider = SignatureHelpProvider::new(
        parser.get_arena(),
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );

    // Position at the second argument '2' (line 1, column 7)
    let pos = Position::new(1, 7);
    let mut cache = None;
    let help = provider.get_signature_help(root, pos, &mut cache);

    assert!(help.is_some(), "Should find signature help");

    if let Some(h) = help {
        assert_eq!(h.active_parameter, 1, "Should be on second parameter");
        assert!(!h.signatures.is_empty(), "Should have signatures");
        // Note: The label format depends on how Checker resolves types
        // For a simple function it may not include the full signature
    }
}

#[test]
fn test_signature_help_no_call() {
    let source = "const x = 42;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let interner = TypeInterner::new();
    let line_map = LineMap::build(source);

    let provider = SignatureHelpProvider::new(
        parser.get_arena(),
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );

    // Position not in a call
    let pos = Position::new(0, 5);
    let mut cache = None;
    let help = provider.get_signature_help(root, pos, &mut cache);

    assert!(
        help.is_none(),
        "Should not find signature help outside call"
    );
}

#[test]
fn test_signature_help_first_arg() {
    // function foo(a: string): void {}
    // foo(|);
    let source = "function foo(a: string): void {}\nfoo();";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let interner = TypeInterner::new();
    let line_map = LineMap::build(source);

    let provider = SignatureHelpProvider::new(
        parser.get_arena(),
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );

    // Position inside the call (line 1, column 4)
    let pos = Position::new(1, 4);
    let mut cache = None;
    let help = provider.get_signature_help(root, pos, &mut cache);

    assert!(help.is_some(), "Should find signature help");

    if let Some(h) = help {
        assert_eq!(h.active_parameter, 0, "Should be on first parameter");
    }
}

#[test]
fn test_signature_help_incomplete_call_eof() {
    // function add(a: number, b: number): number { return a + b; }
    // add(
    let source = "function add(a: number, b: number): number { return a + b; }\nadd(";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let interner = TypeInterner::new();
    let line_map = LineMap::build(source);

    let provider = SignatureHelpProvider::new(
        parser.get_arena(),
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );

    // Position at EOF, just after '(' (line 1, column 4).
    let pos = Position::new(1, 4);
    let mut cache = None;
    let help = provider.get_signature_help(root, pos, &mut cache);

    assert!(
        help.is_some(),
        "Should find signature help in incomplete call"
    );
    if let Some(h) = help {
        assert_eq!(h.active_parameter, 0, "Should be on first parameter");
    }
}

#[test]
fn test_signature_help_incomplete_member_call() {
    let source = "interface Obj { method(a: number, b: string): void; }\ndeclare const obj: Obj;\nobj.method(";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let interner = TypeInterner::new();
    let line_map = LineMap::build(source);

    let provider = SignatureHelpProvider::new(
        parser.get_arena(),
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );

    let pos = Position::new(2, 11); // After the opening paren.
    let mut cache = None;
    let help = provider.get_signature_help(root, pos, &mut cache);

    assert!(help.is_some(), "Should find signature help for member call");
    if let Some(h) = help {
        assert_eq!(h.active_parameter, 0, "Should be on first parameter");
        assert!(!h.signatures.is_empty(), "Should have signatures");
    }
}

#[test]
fn test_signature_help_incomplete_callable_interface_call() {
    let source = "interface C { (): number; }\ndeclare const c: C;\nc(";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let interner = TypeInterner::new();
    let line_map = LineMap::build(source);

    let provider = SignatureHelpProvider::new(
        parser.get_arena(),
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );

    let pos = Position::new(2, 2); // After the opening paren.
    let mut cache = None;
    let help = provider.get_signature_help(root, pos, &mut cache);

    assert!(
        help.is_some(),
        "Should find signature help for incomplete callable interface call"
    );
    if let Some(h) = help {
        assert_eq!(h.active_parameter, 0, "Should be on first parameter");
        assert_eq!(h.signatures.len(), 1, "Should expose one call signature");
        assert_eq!(h.signatures[0].label, "c(): number");
    }
}

#[test]
fn test_signature_help_between_arguments() {
    // Test edge case: cursor between arguments (after comma, before next arg)
    // function process(a: any, b: number, c: string): void {}
    // process(1, |2, 3);
    //          ^ cursor here should be on parameter 1
    let source = "function process(a: any, b: number, c: string): void {}\nprocess(1, 2, 3);";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let interner = TypeInterner::new();
    let line_map = LineMap::build(source);

    let provider = SignatureHelpProvider::new(
        parser.get_arena(),
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );

    // Test cursor at first argument
    let pos1 = Position::new(1, 8); // At "1"
    let mut cache = None;
    let help1 = provider.get_signature_help(root, pos1, &mut cache);
    if let Some(h) = help1 {
        assert_eq!(h.active_parameter, 0, "Should be on first parameter");
    }

    // Test cursor at second argument
    let pos2 = Position::new(1, 11); // At "2"
    let help2 = provider.get_signature_help(root, pos2, &mut cache);
    if let Some(h) = help2 {
        assert_eq!(h.active_parameter, 1, "Should be on second parameter");
    }

    // Test cursor between comma and second argument
    let pos_between = Position::new(1, 10); // Between "," and "2"
    let help_between = provider.get_signature_help(root, pos_between, &mut cache);
    if let Some(h) = help_between {
        assert_eq!(h.active_parameter, 1, "Should be on second parameter");
    }

    // Test cursor at third argument
    let pos3 = Position::new(1, 14); // At "3"
    let help3 = provider.get_signature_help(root, pos3, &mut cache);
    if let Some(h) = help3 {
        assert_eq!(h.active_parameter, 2, "Should be on third parameter");
    }
}

#[test]
fn test_signature_help_trailing_comma() {
    // function foo(a: number, b: string): void {}
    // foo(1, |);
    let source = "function foo(a: number, b: string): void {}\nfoo(1, );";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let interner = TypeInterner::new();
    let line_map = LineMap::build(source);

    let provider = SignatureHelpProvider::new(
        parser.get_arena(),
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );

    let pos = Position::new(1, 6); // After the comma.
    let mut cache = None;
    let help = provider.get_signature_help(root, pos, &mut cache);
    assert!(help.is_some(), "Should find signature help");

    if let Some(h) = help {
        assert_eq!(
            h.active_parameter, 1,
            "Should be on second parameter after trailing comma"
        );
    }
}

#[test]
fn test_signature_help_comment_comma_ignored() {
    // function foo(a: number, b: string): void {}
    // foo(1 /*,*/ |);
    let source = "function foo(a: number, b: string): void {}\nfoo(1 /*,*/ );";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let interner = TypeInterner::new();
    let line_map = LineMap::build(source);

    let provider = SignatureHelpProvider::new(
        parser.get_arena(),
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );

    let pos = Position::new(1, 11); // After the comment, before the close paren.
    let mut cache = None;
    let help = provider.get_signature_help(root, pos, &mut cache);
    assert!(help.is_some(), "Should find signature help");

    if let Some(h) = help {
        assert_eq!(
            h.active_parameter, 0,
            "Should stay on first parameter when comma is only in comment"
        );
    }
}

#[test]
fn test_signature_help_overload_selection() {
    let source = "interface Fn {\n  (a: number): void;\n  (a: number, b: string): void;\n}\ndeclare const fn: Fn;\nfn(1);\nfn(1, \"x\");";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let interner = TypeInterner::new();
    let line_map = LineMap::build(source);

    let provider = SignatureHelpProvider::new(
        parser.get_arena(),
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );

    let mut cache = None;
    let pos_first = Position::new(5, 3); // At "1"
    let help_first = provider.get_signature_help(root, pos_first, &mut cache);
    assert!(
        help_first.is_some(),
        "Should find signature help for first call"
    );
    let first = help_first.unwrap();
    assert!(first.signatures.len() >= 2, "Expected overload signatures");
    let first_active = &first.signatures[first.active_signature as usize];
    assert!(
        !first_active.label.contains("b: string"),
        "First call should select single-arg overload"
    );

    let pos_second = Position::new(6, 6); // At "\"x\""
    let help_second = provider.get_signature_help(root, pos_second, &mut cache);
    assert!(
        help_second.is_some(),
        "Should find signature help for second call"
    );
    let second = help_second.unwrap();
    assert!(second.signatures.len() >= 2, "Expected overload signatures");
    let second_active = &second.signatures[second.active_signature as usize];
    assert!(
        second_active.label.contains("b: string"),
        "Second call should select two-arg overload"
    );
}

