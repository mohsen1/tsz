#[test]
fn test_signature_help_generic_function() {
    // Generic function called WITHOUT explicit type arguments:
    // infer type parameters from call arguments in the signature label.
    let source = "function identity<T>(value: T): T { return value; }\nidentity(42);";
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
        .get_signature_help(root, Position::new(1, 9), &mut cache)
        .expect("Should find signature help for generic function");
    let sig = &help.signatures[help.active_signature as usize];
    // No explicit type args -> type params hidden, T instantiated from argument type
    assert!(
        !sig.label.contains("<T>"),
        "Label should NOT contain type parameter <T> when no explicit type args, got: {}",
        sig.label
    );
    assert_eq!(
        sig.label, "identity(value: number): number",
        "Type params should be instantiated from inferred argument types"
    );
    assert_eq!(sig.parameters.len(), 1);
    assert_eq!(sig.parameters[0].name, "value");
}

#[test]
fn test_signature_help_generic_function_with_explicit_type_args() {
    // Generic function called WITH explicit type arguments:
    // Type parameters are instantiated with the explicit type args and
    // the <T> prefix is hidden (matching TypeScript behavior).
    let source = "function identity<T>(value: T): T { return value; }\nidentity<number>(42);";
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
        .get_signature_help(root, Position::new(1, 17), &mut cache)
        .expect("Should find signature help for generic function with explicit type args");
    let sig = &help.signatures[help.active_signature as usize];
    assert!(
        !sig.label.contains("<T>"),
        "Label should NOT contain <T> when explicit type args instantiate it, got: {}",
        sig.label
    );
    assert!(
        sig.label.contains("number"),
        "Label should show instantiated type 'number', got: {}",
        sig.label
    );
    assert_eq!(sig.parameters.len(), 1);
    assert_eq!(sig.parameters[0].name, "value");
    assert!(
        sig.parameters[0].label.contains("number"),
        "Parameter label should show 'number' instead of 'T', got: {}",
        sig.parameters[0].label
    );
}

#[test]
fn test_signature_help_generic_with_constraint() {
    // Generic function with extends constraint, called WITHOUT explicit type args.
    // Type parameter is instantiated from argument type.
    let source = "function first<T extends any[]>(arr: T): T { return arr; }\nfirst([1, 2]);";
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
        .get_signature_help(root, Position::new(1, 6), &mut cache)
        .expect("Should find signature help for generic function with constraint");
    let sig = &help.signatures[help.active_signature as usize];
    // No explicit type args -> type params hidden, T instantiated from argument type.
    assert!(
        !sig.label.contains("extends"),
        "Label should NOT contain 'extends' constraint without explicit type args, got: {}",
        sig.label
    );
    assert_eq!(
        sig.label, "first(arr: number[]): number[]",
        "Type params should be instantiated from inferred argument types"
    );
}

#[test]
fn test_signature_help_constructor_class_direct() {
    // Direct class constructor call via `new`
    let source = "class Point {\n  constructor(x: number, y: number) {}\n}\nnew Point(1, 2);";
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
    let help = provider.get_signature_help(root, Position::new(3, 10), &mut cache);
    if let Some(h) = help {
        assert!(
            !h.signatures.is_empty(),
            "Should have constructor signatures"
        );
        let sig = &h.signatures[h.active_signature as usize];
        assert_eq!(h.active_parameter, 0, "Should be on first parameter");
        assert_eq!(sig.parameters.len(), 2, "Constructor should have 2 params");
    }
}

#[test]
fn test_signature_help_constructor_second_arg() {
    // Test active parameter in constructor call at second argument
    let source = "class Pair {\n  constructor(a: string, b: number) {}\n}\nnew Pair(\"x\", 42);";
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
    let help = provider.get_signature_help(root, Position::new(3, 14), &mut cache);
    if let Some(h) = help {
        assert_eq!(
            h.active_parameter, 1,
            "Should be on second parameter in constructor call"
        );
    }
}

#[test]
fn test_signature_help_method_call_on_class_instance() {
    // Method call on a class instance (not just interface)
    let source = "class Calculator {\n  add(a: number, b: number): number { return a + b; }\n}\nconst calc = new Calculator();\ncalc.add(1, 2);";
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
    let help = provider.get_signature_help(root, Position::new(4, 9), &mut cache);
    if let Some(h) = help {
        assert!(
            !h.signatures.is_empty(),
            "Should have signatures for method call"
        );
        assert_eq!(h.active_parameter, 0, "Should be on first parameter");
    }
}

#[test]
fn test_signature_help_deeply_nested_calls() {
    // Three levels of nesting: a(b(c(|)))
    let source = "function a(x: string): void {}\nfunction b(x: number): string { return ''; }\nfunction c(x: boolean): number { return 0; }\na(b(c(";
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
    // Cursor after c( -- should show signature for c
    let help = provider.get_signature_help(root, Position::new(3, 6), &mut cache);
    if let Some(h) = help {
        assert!(!h.signatures.is_empty());
        let sig = &h.signatures[h.active_signature as usize];
        // The innermost call is c, so we expect its parameter
        assert!(
            sig.label.contains("boolean") || sig.label.starts_with("c("),
            "Should show signature for innermost call 'c', got: {}",
            sig.label
        );
    }
}

#[test]
fn test_signature_help_callback_as_argument() {
    // Function that takes a callback as an argument
    let source = "function forEach(arr: any[], callback: (item: any) => void): void {}\nforEach([1, 2], (x) => {});";
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
    // Cursor at the callback argument position
    let help = provider.get_signature_help(root, Position::new(1, 16), &mut cache);
    if let Some(h) = help {
        assert_eq!(
            h.active_parameter, 1,
            "Should be on callback parameter (second arg)"
        );
        let sig = &h.signatures[h.active_signature as usize];
        assert_eq!(sig.parameters.len(), 2, "Should have 2 parameters");
    }
}

#[test]
fn test_signature_help_default_parameter_value() {
    // Function with default parameter value -- default params should be treated as optional
    let source =
        "function greet(name: string, greeting: string = \"hello\"): void {}\ngreet(\"world\");";
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
        .get_signature_help(root, Position::new(1, 6), &mut cache)
        .expect("Should find signature help");
    let sig = &help.signatures[help.active_signature as usize];
    assert_eq!(sig.parameters.len(), 2, "Should have 2 parameters");
    // The function has default value, so the second param should be treated as optional
    // by the overload selection logic (not requiring 2 args to match)
    assert_eq!(help.active_parameter, 0);
}

#[test]
fn test_signature_help_rest_param_active_parameter_clamp() {
    // With rest parameter, active_parameter should advance past the last named param
    let source = "function log(prefix: string, ...msgs: string[]): void {}\nlog(\"info\", \"a\", \"b\", \"c\");";
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

    // At "a" -- second arg, maps to rest param (index 1)
    let h1 = provider
        .get_signature_help(root, Position::new(1, 12), &mut cache)
        .expect("help at rest arg 1");
    assert_eq!(
        h1.active_parameter, 1,
        "First rest arg should be param index 1"
    );

    // At "b" -- third arg, still rest param (index 2)
    let h2 = provider
        .get_signature_help(root, Position::new(1, 17), &mut cache)
        .expect("help at rest arg 2");
    assert_eq!(
        h2.active_parameter, 2,
        "Second rest arg should be param index 2"
    );

    // At "c" -- fourth arg, still rest param (index 3)
    let h3 = provider
        .get_signature_help(root, Position::new(1, 22), &mut cache)
        .expect("help at rest arg 3");
    assert_eq!(
        h3.active_parameter, 3,
        "Third rest arg should be param index 3"
    );
}

