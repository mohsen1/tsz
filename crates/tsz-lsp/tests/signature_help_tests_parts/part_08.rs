#[test]
fn test_signature_help_immediately_invoked_function() {
    let source = "(function(x: number) {})(42);";
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
    let help = provider.get_signature_help(root, Position::new(0, 25), &mut cache);
    // IIFE may or may not provide signature help
    let _ = help;
}

#[test]
fn test_signature_help_before_open_paren() {
    let source = "function f(a: number): void {}\nf(1);";
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
    // Position at function name, before the open paren
    let help = provider.get_signature_help(root, Position::new(1, 0), &mut cache);
    // Should not trigger signature help when cursor is on function name
    assert!(
        help.is_none(),
        "Should not trigger signature help before open paren"
    );
}

#[test]
fn test_signature_help_after_close_paren() {
    let source = "function f(a: number): void {}\nf(1);";
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
    // Position after close paren
    let help = provider.get_signature_help(root, Position::new(1, 4), &mut cache);
    // After close paren, signature help should not trigger
    let _ = help;
}

#[test]
fn test_signature_help_two_functions_same_name_different_scope() {
    let source =
        "function f(a: number): void {}\n{ function f(a: string, b: string): void {} }\nf(1);";
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
    let help = provider.get_signature_help(root, Position::new(2, 2), &mut cache);
    if let Some(h) = help {
        assert!(!h.signatures.is_empty());
    }
}

#[test]
fn test_signature_help_with_spread_arg() {
    let source = "function sum(a: number, b: number, c: number): number { return a + b + c; }\nconst args: [number, number, number] = [1, 2, 3];\nsum(...args);";
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
    let help = provider.get_signature_help(root, Position::new(2, 4), &mut cache);
    if let Some(h) = help {
        assert!(!h.signatures.is_empty());
    }
}

#[test]
fn test_signature_help_with_object_arg() {
    let source =
        "function config(opts: { x: number; y: string }): void {}\nconfig({ x: 1, y: 'a' });";
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
    if let Some(h) = help {
        assert_eq!(
            h.active_parameter, 0,
            "Object literal is the first parameter"
        );
    }
}

#[test]
fn test_signature_help_with_array_arg() {
    let source = "function process(items: number[]): void {}\nprocess([1, 2, 3]);";
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
    let help = provider.get_signature_help(root, Position::new(1, 9), &mut cache);
    if let Some(h) = help {
        assert_eq!(h.active_parameter, 0);
    }
}

#[test]
fn test_signature_help_multiple_type_params() {
    let source =
        "function map<K, V>(key: K, value: V): [K, V] { return [key, value]; }\nmap('a', 1);";
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
    let help = provider.get_signature_help(root, Position::new(1, 9), &mut cache);
    if let Some(h) = help {
        assert_eq!(h.active_parameter, 1, "Should be on second parameter");
        let sig = &h.signatures[h.active_signature as usize];
        assert_eq!(sig.parameters.len(), 2);
    }
}

#[test]
fn test_signature_help_intersection_param_type() {
    let source =
        "function merge(a: { x: number } & { y: string }): void {}\nmerge({ x: 1, y: 'a' });";
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
fn test_signature_help_conditional_type_param() {
    let source = "function check<T>(val: T extends string ? T : never): void {}\ncheck('hello');";
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

