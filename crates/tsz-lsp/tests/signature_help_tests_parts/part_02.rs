#[test]
fn test_signature_with_optional_parameter() {
    let source = "function bar(required: string, optional?: number): void {}\nbar(\"a\");";
    let (parser, binder, interner, line_map, root) = setup_provider(source);
    let provider = SignatureHelpProvider::new(
        parser.get_arena(),
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );
    let pos = Position::new(1, 5);
    let mut cache = None;
    let help = provider.get_signature_help(root, pos, &mut cache);
    assert!(help.is_some());
    let h = help.unwrap();
    let sig = &h.signatures[h.active_signature as usize];
    assert_eq!(sig.parameters.len(), 2);
    assert!(
        !sig.parameters[0].is_optional,
        "First param should not be optional"
    );
    assert!(
        sig.parameters[1].is_optional,
        "Second param should be optional"
    );
    assert!(
        sig.parameters[1].label.contains("?"),
        "Optional param label should contain '?'"
    );
}

#[test]
fn test_signature_with_rest_parameter() {
    let source =
        "function variadic(first: string, ...rest: number[]): void {}\nvariadic(\"a\", 1, 2, 3);";
    let (parser, binder, interner, line_map, root) = setup_provider(source);
    let provider = SignatureHelpProvider::new(
        parser.get_arena(),
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );
    let pos = Position::new(1, 10);
    let mut cache = None;
    let help = provider.get_signature_help(root, pos, &mut cache);
    assert!(help.is_some());
    let h = help.unwrap();
    let sig = &h.signatures[h.active_signature as usize];
    assert!(sig.is_variadic, "Signature should be variadic");
    assert!(
        sig.parameters.last().unwrap().is_rest,
        "Last param should be rest"
    );
    assert!(
        sig.parameters.last().unwrap().label.starts_with("..."),
        "Rest param label should start with '...'"
    );
}

#[test]
fn test_signature_help_prefers_source_type_alias_and_inferred_return_type() {
    let source = "type Box = { value: number };\nfunction id(value: Box) { return value; }\nid({ value: 1 });";
    let (parser, binder, interner, line_map, root) = setup_provider(source);
    let provider = SignatureHelpProvider::new(
        parser.get_arena(),
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );
    let pos = Position::new(2, 4);
    let mut cache = None;
    let help = provider.get_signature_help(root, pos, &mut cache);
    assert!(help.is_some(), "Should find signature help");
    let h = help.unwrap();
    let sig = &h.signatures[h.active_signature as usize];
    assert_eq!(
        sig.label, "id(value: Box): Box",
        "Signature help should prefer source alias names and inferred return type"
    );
}

#[test]
fn test_signature_label_for_interface_method() {
    let source = "interface Obj { method(a: number, b: string): void; }\ndeclare const obj: Obj;\nobj.method(1, \"x\");";
    let (parser, binder, interner, line_map, root) = setup_provider(source);
    let provider = SignatureHelpProvider::new(
        parser.get_arena(),
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );
    let pos = Position::new(2, 11); // After "obj.method("
    let mut cache = None;
    let help = provider.get_signature_help(root, pos, &mut cache);
    assert!(help.is_some(), "Should find signature help for member call");
    let h = help.unwrap();
    let sig = &h.signatures[h.active_signature as usize];
    // The callee name should be "method" (the property name)
    assert!(
        sig.label.starts_with("method("),
        "Label should start with method name, got: {}",
        sig.label
    );
    assert_eq!(
        sig.prefix, "method(",
        "Prefix should be method name + open paren, got: {}",
        sig.prefix
    );
}

#[test]
fn test_signature_active_parameter_at_different_positions() {
    let source = "function triple(a: number, b: number, c: number): void {}\ntriple(1, 2, 3);";
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

    // At first arg
    let h0 = provider
        .get_signature_help(root, Position::new(1, 7), &mut cache)
        .expect("help at 1st arg");
    assert_eq!(h0.active_parameter, 0);

    // At second arg
    let h1 = provider
        .get_signature_help(root, Position::new(1, 10), &mut cache)
        .expect("help at 2nd arg");
    assert_eq!(h1.active_parameter, 1);

    // At third arg
    let h2 = provider
        .get_signature_help(root, Position::new(1, 13), &mut cache)
        .expect("help at 3rd arg");
    assert_eq!(h2.active_parameter, 2);
}

#[test]
fn test_signature_overload_count() {
    let source = "interface Fn {\n  (a: number): void;\n  (a: number, b: string): void;\n  (a: number, b: string, c: boolean): void;\n}\ndeclare const fn: Fn;\nfn(1);";
    let (parser, binder, interner, line_map, root) = setup_provider(source);
    let provider = SignatureHelpProvider::new(
        parser.get_arena(),
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );
    let pos = Position::new(6, 3);
    let mut cache = None;
    let help = provider.get_signature_help(root, pos, &mut cache);
    assert!(help.is_some());
    let h = help.unwrap();
    assert_eq!(h.signatures.len(), 3, "Should have 3 overloaded signatures");
    // The active signature should be the one with 1 param
    let active = &h.signatures[h.active_signature as usize];
    assert_eq!(
        active.parameters.len(),
        1,
        "Active signature should match arg count"
    );
}

// =========================================================================
// Additional edge case tests
// =========================================================================

#[test]
fn test_signature_help_no_function_call() {
    let source = "const x = 1;";
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
    let help = provider.get_signature_help(root, Position::new(0, 5), &mut cache);
    assert!(
        help.is_none(),
        "Should not provide signature help outside function call"
    );
}

#[test]
fn test_signature_help_empty_file() {
    let source = "";
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
    let help = provider.get_signature_help(root, Position::new(0, 0), &mut cache);
    assert!(
        help.is_none(),
        "Should not provide signature help in empty file"
    );
}

#[test]
fn test_signature_help_arrow_function_call() {
    let source = "const add = (a: number, b: number): number => a + b;\nadd(";
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
    if let Some(h) = help {
        assert!(
            !h.signatures.is_empty(),
            "Should have signatures for arrow function"
        );
    }
}

#[test]
fn test_signature_help_nested_call() {
    let source = "function outer(x: number): string { return ''; }\nfunction inner(s: string): void {}\ninner(outer(";
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
    // Position inside inner call (outer's open paren)
    let help = provider.get_signature_help(root, Position::new(2, 12), &mut cache);
    if let Some(h) = help {
        // Should show signature for the innermost call (outer)
        assert!(!h.signatures.is_empty());
    }
}

// =========================================================================
// Extended coverage tests
// =========================================================================

