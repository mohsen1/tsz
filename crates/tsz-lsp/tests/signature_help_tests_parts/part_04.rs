#[test]
fn test_signature_help_tagged_template_literal() {
    // Tagged template expression: tag`text ${expr} text`
    let source = "function tag(strings: TemplateStringsArray, ...values: any[]): string { return ''; }\ntag`hello ${42} world`;";
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
    // Cursor in the template text before the expression
    let help = provider.get_signature_help(root, Position::new(1, 5), &mut cache);
    if let Some(h) = help {
        assert!(
            !h.signatures.is_empty(),
            "Should provide signature help for tagged template"
        );
        // In template text, active param should be 0 (templateStrings)
        assert_eq!(
            h.active_parameter, 0,
            "Cursor in template text should map to param 0"
        );
    }
}

#[test]
fn test_signature_help_applicable_span() {
    // Verify that applicable_span_start and applicable_span_length are reasonable
    let source = "function add(x: number, y: number): number { return x + y; }\nadd(10, 20);";
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
        .get_signature_help(root, Position::new(1, 4), &mut cache)
        .expect("Should find signature help");
    // The applicable span should cover the arguments region
    let span_start = help.applicable_span_start as usize;
    let span_length = help.applicable_span_length as usize;
    assert!(
        span_start > 0,
        "Applicable span start should be after opening paren"
    );
    assert!(
        span_length > 0,
        "Applicable span length should be non-zero for non-empty args"
    );
    // The span should cover "10, 20"
    let span_text = &source[span_start..span_start + span_length];
    assert!(
        span_text.contains("10") && span_text.contains("20"),
        "Span should cover arguments, got: '{span_text}'"
    );
}

#[test]
fn test_signature_help_argument_count() {
    // Verify argument_count field
    let source = "function f(a: number, b: number, c: number): void {}\nf(1, 2, 3);";
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
        .get_signature_help(root, Position::new(1, 2), &mut cache)
        .expect("Should find signature help");
    assert_eq!(
        help.argument_count, 3,
        "argument_count should reflect actual arguments at call site"
    );
}

#[test]
fn test_signature_help_line_comment_between_args() {
    // Comma detection should skip line comments
    let source =
        "function foo(a: number, b: number): void {}\nfoo(1 // comment with , comma\n, 2);";
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
    // Cursor at "2" on line 2
    let help = provider.get_signature_help(root, Position::new(2, 2), &mut cache);
    if let Some(h) = help {
        assert_eq!(
            h.active_parameter, 1,
            "Should be on second parameter after comma (not confused by comment comma)"
        );
    }
}

#[test]
fn test_signature_help_zero_param_function() {
    // Function with no parameters
    let source = "function noop(): void {}\nnoop();";
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
        .get_signature_help(root, Position::new(1, 5), &mut cache)
        .expect("Should find signature help for zero-param function");
    let sig = &help.signatures[help.active_signature as usize];
    assert_eq!(sig.parameters.len(), 0, "Should have zero parameters");
    assert_eq!(help.active_parameter, 0);
    assert!(
        sig.label.contains("noop("),
        "Label should contain function name, got: {}",
        sig.label
    );
}

#[test]
fn test_signature_help_callable_interface_with_multiple_call_signatures() {
    // Interface with multiple call signatures (overloads via callable interface)
    let source = "interface Handler {\n  (event: string): void;\n  (event: string, data: any): void;\n  (event: string, data: any, callback: () => void): void;\n}\ndeclare const handler: Handler;\nhandler(\"click\", {}, () => {});";
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
    // Cursor at the third argument
    let help = provider
        .get_signature_help(root, Position::new(6, 21), &mut cache)
        .expect("Should find signature help for callable interface");
    assert_eq!(
        help.signatures.len(),
        3,
        "Should have 3 call signatures from interface"
    );
    // Active signature should be the 3-param one
    let active = &help.signatures[help.active_signature as usize];
    assert_eq!(
        active.parameters.len(),
        3,
        "Active signature should be the 3-param overload"
    );
}

#[test]
fn test_signature_help_nested_function_calls_inner() {
    // f(g(|)) -- cursor inside g() should show g's signature
    let source =
        "function f(x: string): void {}\nfunction g(y: number): string { return ''; }\nf(g(42));";
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
    // Cursor at "42" inside g() (line 2, col 4)
    let help = provider.get_signature_help(root, Position::new(2, 4), &mut cache);
    if let Some(h) = help {
        let sig = &h.signatures[h.active_signature as usize];
        assert!(
            sig.label.starts_with("g("),
            "Should show signature for inner call 'g', got: {}",
            sig.label
        );
    }
}

#[test]
fn test_signature_help_method_on_property_access() {
    // obj.method(|) should show method's signature
    let source = "interface MyObj { doStuff(a: number, b: string): boolean; }\ndeclare const obj: MyObj;\nobj.doStuff(1, \"x\");";
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
    let help = provider.get_signature_help(root, Position::new(2, 12), &mut cache);
    assert!(
        help.is_some(),
        "Should find signature help for property access method call"
    );
    let h = help.unwrap();
    let sig = &h.signatures[h.active_signature as usize];
    assert!(
        sig.label.starts_with("doStuff("),
        "Label should start with method name 'doStuff', got: {}",
        sig.label
    );
    assert_eq!(sig.parameters.len(), 2);
}

#[test]
fn test_signature_help_constructor_with_new() {
    // new Foo(|) should show constructor signature
    let source =
        "class Foo {\n  constructor(name: string, age: number) {}\n}\nnew Foo(\"bar\", 42);";
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
    let help = provider.get_signature_help(root, Position::new(3, 8), &mut cache);
    if let Some(h) = help {
        assert!(
            !h.signatures.is_empty(),
            "Should have constructor signatures for new Foo()"
        );
        let sig = &h.signatures[h.active_signature as usize];
        assert_eq!(sig.parameters.len(), 2, "Constructor should have 2 params");
        assert_eq!(h.active_parameter, 0);
    }
}

#[test]
fn test_signature_help_generic_function_with_explicit_type_arg() {
    // identity<string>(|) should show instantiated signature
    let source =
        "function identity<T>(value: T): T { return value; }\nidentity<string>(\"hello\");";
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
    let help = provider.get_signature_help(root, Position::new(1, 17), &mut cache);
    assert!(
        help.is_some(),
        "Should find signature help for generic function with explicit type arg"
    );
    let h = help.unwrap();
    let sig = &h.signatures[h.active_signature as usize];
    assert!(
        !sig.label.contains("<T>"),
        "Label should NOT contain <T> when explicit type arg instantiates it, got: {}",
        sig.label
    );
    assert!(
        sig.label.contains("string") || sig.label.contains("\"hello\""),
        "Label should show an instantiated explicit/string argument type, got: {}",
        sig.label
    );
}

