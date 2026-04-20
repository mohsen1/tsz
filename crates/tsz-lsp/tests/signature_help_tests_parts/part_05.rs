#[test]
fn test_signature_help_rest_parameter_function() {
    // Function with only rest parameter
    let source =
        "function collect(...items: string[]): string[] { return items; }\ncollect(\"a\", \"b\");";
    let (parser, binder, interner, line_map, root) = setup_provider(source);
    let provider = SignatureHelpProvider::new(
        parser.get_arena(),
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );
    let mut cache = None;
    let help = provider.get_signature_help(root, Position::new(1, 8), &mut cache);
    assert!(help.is_some(), "Should find signature help for rest param");
    let h = help.unwrap();
    let sig = &h.signatures[h.active_signature as usize];
    assert!(sig.is_variadic, "Signature should be variadic");
    assert_eq!(sig.parameters.len(), 1);
    assert!(sig.parameters[0].is_rest);
    assert!(
        sig.parameters[0].label.starts_with("..."),
        "Rest param label should start with '...', got: {}",
        sig.parameters[0].label
    );
}

#[test]
fn test_signature_help_callback_parameter_position() {
    // Function that receives a callback, cursor at callback position
    let source =
        "function run(callback: (result: number) => void): void {}\nrun((r) => console.log(r));";
    let (parser, binder, interner, line_map, root) = setup_provider(source);
    let provider = SignatureHelpProvider::new(
        parser.get_arena(),
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );
    let mut cache = None;
    let help = provider.get_signature_help(root, Position::new(1, 4), &mut cache);
    assert!(
        help.is_some(),
        "Should find signature help at callback argument"
    );
    let h = help.unwrap();
    assert_eq!(
        h.active_parameter, 0,
        "Callback is the first parameter of run()"
    );
}

#[test]
fn test_signature_help_void_function() {
    // Function with void return type
    let source = "function doNothing(): void {}\ndoNothing();";
    let (parser, binder, interner, line_map, root) = setup_provider(source);
    let provider = SignatureHelpProvider::new(
        parser.get_arena(),
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );
    let mut cache = None;
    // Cursor inside the parens (line 1, col 10 = between '(' and ')')
    let help = provider
        .get_signature_help(root, Position::new(1, 10), &mut cache)
        .expect("Should find signature help for void function");
    let sig = &help.signatures[help.active_signature as usize];
    assert!(
        sig.label.contains("void"),
        "Label should contain 'void' return type, got: {}",
        sig.label
    );
    assert_eq!(
        sig.parameters.len(),
        0,
        "Void function should have 0 params"
    );
}

#[test]
fn test_signature_help_default_parameter_values_in_signature() {
    // Function with default values -- params with defaults are optional
    let source = "function create(name: string, count: number = 1, flag: boolean = true): void {}\ncreate(\"test\");";
    let (parser, binder, interner, line_map, root) = setup_provider(source);
    let provider = SignatureHelpProvider::new(
        parser.get_arena(),
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );
    let mut cache = None;
    let help = provider
        .get_signature_help(root, Position::new(1, 7), &mut cache)
        .expect("Should find signature help with default params");
    let sig = &help.signatures[help.active_signature as usize];
    assert_eq!(sig.parameters.len(), 3, "Should have 3 parameters");
    assert!(
        !sig.parameters[0].is_optional,
        "First param without default should not be optional"
    );
}

#[test]
fn test_signature_help_after_trailing_comma_position() {
    // foo(1, 2, |) -- after trailing comma with three params
    let source = "function triple(a: number, b: number, c: number): void {}\ntriple(1, 2, );";
    let (parser, binder, interner, line_map, root) = setup_provider(source);
    let provider = SignatureHelpProvider::new(
        parser.get_arena(),
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );
    let mut cache = None;
    // Cursor after trailing comma (line 1, col 13)
    let help = provider.get_signature_help(root, Position::new(1, 13), &mut cache);
    assert!(
        help.is_some(),
        "Should find signature help after trailing comma"
    );
    let h = help.unwrap();
    assert_eq!(
        h.active_parameter, 2,
        "After trailing comma with 2 args, should be on third parameter"
    );
}

#[test]
fn test_signature_help_multiple_overloads_selection_by_arg_count() {
    // With 3 overloads of different arities, the right one should be selected
    let source = "function multi(a: number): void;\nfunction multi(a: number, b: string): void;\nfunction multi(a: number, b: string, c: boolean): void;\nfunction multi(a: number, b?: string, c?: boolean): void {}\nmulti(1, \"x\", true);";
    let (parser, binder, interner, line_map, root) = setup_provider(source);
    let provider = SignatureHelpProvider::new(
        parser.get_arena(),
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );
    let mut cache = None;
    // Cursor at third argument "true" (line 4, col 14)
    let help = provider.get_signature_help(root, Position::new(4, 14), &mut cache);
    assert!(
        help.is_some(),
        "Should find signature help for overloaded function"
    );
    let h = help.unwrap();
    assert!(
        h.signatures.len() >= 3,
        "Should have at least 3 overloads, got: {}",
        h.signatures.len()
    );
    let active = &h.signatures[h.active_signature as usize];
    assert_eq!(
        active.parameters.len(),
        3,
        "Active overload should be the 3-param one"
    );
}

#[test]
fn test_signature_help_destructured_parameter() {
    // Function with destructured parameter
    let source =
        "function process({ x, y }: { x: number; y: number }): void {}\nprocess({ x: 1, y: 2 });";
    let (parser, binder, interner, line_map, root) = setup_provider(source);
    let provider = SignatureHelpProvider::new(
        parser.get_arena(),
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );
    let mut cache = None;
    let help = provider.get_signature_help(root, Position::new(1, 8), &mut cache);
    assert!(
        help.is_some(),
        "Should find signature help for destructured parameter function"
    );
    let h = help.unwrap();
    assert_eq!(h.active_parameter, 0);
    let sig = &h.signatures[h.active_signature as usize];
    assert_eq!(sig.parameters.len(), 1, "Destructured counts as one param");
}

#[test]
fn test_signature_help_method_on_class_with_multiple_methods() {
    // Ensure correct method is resolved when class has multiple methods
    let source = "class Svc {\n  start(port: number): void {}\n  stop(): void {}\n}\ndeclare const svc: Svc;\nsvc.start(8080);";
    let (parser, binder, interner, line_map, root) = setup_provider(source);
    let provider = SignatureHelpProvider::new(
        parser.get_arena(),
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );
    let mut cache = None;
    let help = provider.get_signature_help(root, Position::new(5, 10), &mut cache);
    if let Some(h) = help {
        let sig = &h.signatures[h.active_signature as usize];
        assert!(
            sig.label.starts_with("start("),
            "Should show 'start' method signature, got: {}",
            sig.label
        );
        assert_eq!(sig.parameters.len(), 1);
    }
}

#[test]
fn test_signature_help_function_returning_function() {
    // Function that returns another function
    let source = "function outer(): (x: number) => void { return (x) => {}; }\nouter();";
    let (parser, binder, interner, line_map, root) = setup_provider(source);
    let provider = SignatureHelpProvider::new(
        parser.get_arena(),
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );
    let mut cache = None;
    let help = provider.get_signature_help(root, Position::new(1, 6), &mut cache);
    if let Some(h) = help {
        let sig = &h.signatures[h.active_signature as usize];
        assert!(
            sig.label.starts_with("outer("),
            "Should show outer function signature, got: {}",
            sig.label
        );
        assert_eq!(sig.parameters.len(), 0, "outer() takes no params");
    }
}

#[test]
fn test_signature_help_multiline_call() {
    // Function call spanning multiple lines
    let source = "function build(a: number, b: string, c: boolean): void {}\nbuild(\n  1,\n  \"hello\",\n  true\n);";
    let (parser, binder, interner, line_map, root) = setup_provider(source);
    let provider = SignatureHelpProvider::new(
        parser.get_arena(),
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );
    let mut cache = None;

    // Cursor at "1" on line 2
    let h0 = provider.get_signature_help(root, Position::new(2, 2), &mut cache);
    if let Some(h) = h0 {
        assert_eq!(h.active_parameter, 0, "First line arg should be param 0");
    }

    // Cursor at "hello" on line 3
    let h1 = provider.get_signature_help(root, Position::new(3, 3), &mut cache);
    if let Some(h) = h1 {
        assert_eq!(h.active_parameter, 1, "Second line arg should be param 1");
    }

    // Cursor at "true" on line 4
    let h2 = provider.get_signature_help(root, Position::new(4, 2), &mut cache);
    if let Some(h) = h2 {
        assert_eq!(h.active_parameter, 2, "Third line arg should be param 2");
    }
}

