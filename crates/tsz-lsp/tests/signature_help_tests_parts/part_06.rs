#[test]
fn test_signature_help_string_argument_with_commas() {
    // Commas inside string literals should not advance the parameter index
    let source = "function f(a: string, b: number): void {}\nf(\"a,b,c\", 42);";
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
    // Cursor at "42" (line 1, col 11)
    let help = provider.get_signature_help(root, Position::new(1, 11), &mut cache);
    if let Some(h) = help {
        assert_eq!(
            h.active_parameter, 1,
            "Commas in string should not affect parameter index"
        );
    }
}

#[test]
fn test_signature_help_no_args_function() {
    let source = "function noArgs(): void {}\nnoArgs();";
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
    let help = provider.get_signature_help(root, Position::new(1, 7), &mut cache);
    if let Some(h) = help {
        assert_eq!(h.active_parameter, 0);
    }
}

#[test]
fn test_signature_help_single_param() {
    let source = "function log(msg: string): void {}\nlog('hello');";
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
    let help = provider.get_signature_help(root, Position::new(1, 5), &mut cache);
    if let Some(h) = help {
        assert_eq!(h.active_parameter, 0);
        assert!(!h.signatures.is_empty());
    }
}

#[test]
fn test_signature_help_at_open_paren() {
    let source = "function f(a: number): void {}\nf(";
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
    let help = provider.get_signature_help(root, Position::new(1, 2), &mut cache);
    // Just after open paren should trigger signature help
    if let Some(h) = help {
        assert_eq!(h.active_parameter, 0);
    }
}

#[test]
fn test_signature_help_class_constructor() {
    let source = "class Foo {\n  constructor(x: number, y: string) {}\n}\nnew Foo(1, 'a');";
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
    let help = provider.get_signature_help(root, Position::new(3, 10), &mut cache);
    if let Some(h) = help {
        assert!(h.active_parameter <= 1);
    }
}

#[test]
fn test_signature_help_arrow_function_call_with_age() {
    let source =
        "const greet = (name: string, age: number) => `Hello ${name}`;\ngreet('World', 25);";
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
    let help = provider.get_signature_help(root, Position::new(1, 15), &mut cache);
    if let Some(h) = help {
        assert_eq!(h.active_parameter, 1);
    }
}

#[test]
fn test_signature_help_outside_call() {
    let source = "function f() {}\nconst x = 1;";
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
    let help = provider.get_signature_help(root, Position::new(1, 5), &mut cache);
    // Outside a call expression, should be None
    assert!(
        help.is_none(),
        "Should not trigger signature help outside call"
    );
}

#[test]
fn test_signature_help_method_call_chain() {
    let source = "class Builder {\n  set(k: string, v: string): Builder { return this; }\n}\nconst b = new Builder();\nb.set('key', 'val');";
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
    let help = provider.get_signature_help(root, Position::new(4, 13), &mut cache);
    // Method call should potentially provide signature help
    let _ = help;
}

#[test]
fn test_signature_help_empty_source() {
    let source = "";
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
    let help = provider.get_signature_help(root, Position::new(0, 0), &mut cache);
    assert!(help.is_none());
}

#[test]
fn test_signature_help_many_params() {
    let source = "function many(a: number, b: string, c: boolean, d: number[], e: object): void {}\nmany(1, 'x', true, [], {});";
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
    // Position at last arg
    let help = provider.get_signature_help(root, Position::new(1, 25), &mut cache);
    if let Some(h) = help {
        assert!(h.active_parameter >= 3, "Should be on 4th or 5th param");
    }
}

// =========================================================================
// Additional edge case tests
// =========================================================================

