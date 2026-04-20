#[test]
fn test_signature_help_mapped_type_param() {
    let source = "function keys<T>(obj: { [K in keyof T]: T[K] }): void {}\nkeys({ a: 1 });";
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
fn test_signature_help_unicode_function_name() {
    let source =
        "function \u{00e4}\u{00f6}\u{00fc}(x: number): void {}\n\u{00e4}\u{00f6}\u{00fc}(42);";
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
    // Should not crash; if found, should have active_parameter 0
    if let Some(h) = help {
        assert_eq!(h.active_parameter, 0);
    }
}

#[test]
fn test_signature_help_readonly_array_param() {
    let source = "function process(items: readonly number[]): void {}\nprocess([1, 2]);";
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
fn test_signature_help_tuple_param() {
    let source = "function pair(t: [string, number]): void {}\npair(['a', 1]);";
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
        assert_eq!(h.active_parameter, 0);
    }
}

#[test]
fn test_signature_help_never_return_type() {
    let source =
        "function throwErr(msg: string): never { throw new Error(msg); }\nthrowErr('oops');";
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
fn test_signature_help_function_with_literal_type_params() {
    let source = "function tag(kind: 'info' | 'warn' | 'error', msg: string): void {}\ntag('info', 'hello');";
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
    // Position at second arg
    let help = provider.get_signature_help(root, Position::new(1, 14), &mut cache);
    if let Some(h) = help {
        assert_eq!(h.active_parameter, 1, "Should be on second parameter");
    }
}

#[test]
fn test_signature_help_promise_return_type() {
    let source = "async function fetchData(url: string): Promise<string> { return ''; }\nfetchData('http://x');";
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
    let help = provider.get_signature_help(root, Position::new(1, 11), &mut cache);
    if let Some(h) = help {
        assert_eq!(h.active_parameter, 0);
    }
}

#[test]
fn test_signature_help_four_params_third_arg() {
    let source = "function quad(a: number, b: string, c: boolean, d: object): void {}\nquad(1, 'x', true, {});";
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
    // Position at third arg 'true'
    let help = provider.get_signature_help(root, Position::new(1, 14), &mut cache);
    if let Some(h) = help {
        assert_eq!(h.active_parameter, 2, "Should be on third parameter");
    }
}

#[test]
fn test_signature_help_generic_with_default_type() {
    // T has a default of `string`, but a concrete argument should still infer `number`.
    let source = "function create<T = string>(val: T): T { return val; }\ncreate(42);";
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
        .get_signature_help(root, Position::new(1, 8), &mut cache)
        .expect("Should find signature help");
    let sig = &help.signatures[help.active_signature as usize];
    assert_eq!(help.active_parameter, 0);
    assert_eq!(
        sig.label, "create(val: number): number",
        "Type param should be instantiated from argument type when inference is available"
    );
}

#[test]
fn test_signature_help_generic_default_overrides_constraint() {
    // V has both a constraint and a default, but a concrete argument still infers `number`.
    let source = "function pick<V extends number = 42>(val: V): V { return val; }\npick(1);";
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
        .expect("Should find signature help");
    let sig = &help.signatures[help.active_signature as usize];
    assert_eq!(
        sig.label, "pick(val: number): number",
        "Type param should be instantiated from argument type when inference is available"
    );
}

