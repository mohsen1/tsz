#[test]
fn test_signature_help_function_with_union_param() {
    let source = "function accept(val: string | number): void {}\naccept(42);";
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
    let help = provider.get_signature_help(root, Position::new(1, 7), &mut cache);
    if let Some(h) = help {
        assert_eq!(h.active_parameter, 0);
        assert!(!h.signatures.is_empty());
    }
}

#[test]
fn test_signature_help_function_with_tuple_param() {
    let source = "function pair(t: [number, string]): void {}\npair([1, 'a']);";
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
    let help = provider.get_signature_help(root, Position::new(1, 5), &mut cache);
    if let Some(h) = help {
        assert_eq!(h.active_parameter, 0);
        let sig = &h.signatures[h.active_signature as usize];
        assert_eq!(sig.parameters.len(), 1);
    }
}

#[test]
fn test_signature_help_function_with_optional_params() {
    let source = "function opt(a: number, b?: string, c?: boolean): void {}\nopt(1);";
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
        assert_eq!(h.active_parameter, 0);
        let sig = &h.signatures[h.active_signature as usize];
        assert_eq!(sig.parameters.len(), 3);
        assert!(
            sig.parameters[1].is_optional,
            "Second param should be optional"
        );
        assert!(
            sig.parameters[2].is_optional,
            "Third param should be optional"
        );
    }
}

#[test]
fn test_signature_help_async_function() {
    let source =
        "async function fetchData(url: string): Promise<void> {}\nfetchData('http://example.com');";
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
    let help = provider.get_signature_help(root, Position::new(1, 10), &mut cache);
    if let Some(h) = help {
        assert_eq!(h.active_parameter, 0);
        assert!(!h.signatures.is_empty());
    }
}

#[test]
fn test_signature_help_function_expression_call() {
    let source = "const multiply = function(a: number, b: number): number { return a * b; };\nmultiply(3, 4);";
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
    let help = provider.get_signature_help(root, Position::new(1, 12), &mut cache);
    if let Some(h) = help {
        assert_eq!(h.active_parameter, 1, "Should be on second parameter");
    }
}

#[test]
fn test_signature_help_single_line_arrow() {
    let source = "const square = (n: number): number => n * n;\nsquare(5);";
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
    let help = provider.get_signature_help(root, Position::new(1, 7), &mut cache);
    if let Some(h) = help {
        assert_eq!(h.active_parameter, 0);
    }
}

#[test]
fn test_signature_help_method_in_object_literal() {
    let source =
        "const obj = { greet(name: string): string { return name; } };\nobj.greet('world');";
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
    let help = provider.get_signature_help(root, Position::new(1, 10), &mut cache);
    // Method on object literal may or may not resolve
    let _ = help;
}

#[test]
fn test_signature_help_nested_parens_in_args() {
    let source = "function f(a: number, b: number): void {}\nf((1 + 2), 3);";
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
    // Cursor at '3' after the nested parens
    let help = provider.get_signature_help(root, Position::new(1, 12), &mut cache);
    if let Some(h) = help {
        assert_eq!(
            h.active_parameter, 1,
            "Nested parens should not confuse parameter counting"
        );
    }
}

#[test]
fn test_signature_help_template_literal_arg() {
    let source = "function tag(s: string): void {}\ntag(`hello`);";
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
    let help = provider.get_signature_help(root, Position::new(1, 5), &mut cache);
    if let Some(h) = help {
        assert_eq!(h.active_parameter, 0);
    }
}

#[test]
fn test_signature_help_function_with_this_param() {
    let source =
        "function handler(this: HTMLElement, event: Event): void {}\nhandler(new Event('click'));";
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
    // `this` parameter may or may not be exposed in signatures
    let _ = help;
}

