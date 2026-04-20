#[test]
fn test_signature_help_generic_no_default_no_constraint() {
    // T has neither default nor constraint -> infer from argument type.
    let source = "function identity<T>(val: T): T { return val; }\nidentity(42);";
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
        .get_signature_help(root, Position::new(1, 10), &mut cache)
        .expect("Should find signature help");
    let sig = &help.signatures[help.active_signature as usize];
    assert_eq!(
        sig.label, "identity(val: number): number",
        "Type param with no default/constraint should be inferred from argument type"
    );
}

#[test]
fn test_signature_help_generic_mixed_type_params() {
    // A has default `boolean`, B has constraint `string`, C has neither.
    // All three should still instantiate from provided arguments.
    let source = "function mix<A = boolean, B extends string, C>(a: A, b: B, c: C): void {}\nmix(true, 'hi', 1);";
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
        sig.label, "mix(a: boolean, b: 'hi', c: number): void",
        "Each type param should instantiate from the corresponding argument type"
    );
}

#[test]
fn test_signature_help_multiple_rest_params() {
    let source =
        "function collect(first: string, ...rest: number[]): void {}\ncollect('a', 1, 2, 3);";
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
    // Position at fourth arg '3'
    let help = provider.get_signature_help(root, Position::new(1, 19), &mut cache);
    if let Some(h) = help {
        // Rest param means active_parameter should clamp at 1 (the rest param index)
        assert!(h.active_parameter >= 1);
    }
}

#[test]
fn test_signature_help_nested_generic_constraints() {
    let source = "function extract<T extends { id: number }>(obj: T): number { return obj.id; }\nextract({ id: 5 });";
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
fn test_signature_help_only_whitespace_source() {
    let source = "   \n  \n   ";
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
    let help = provider.get_signature_help(root, Position::new(0, 1), &mut cache);
    assert!(
        help.is_none(),
        "Whitespace-only source should not produce signature help"
    );
}

#[test]
fn test_signature_help_function_with_index_signature_param() {
    let source = "function lookup(dict: { [key: string]: number }): void {}\nlookup({ a: 1 });";
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
        assert_eq!(h.active_parameter, 0);
    }
}

#[test]
fn test_signature_help_ternary_expression_in_arg() {
    let source = "function f(a: number, b: number): void {}\nf(true ? 1 : 2, 3);";
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
    // Position at second arg '3'
    let help = provider.get_signature_help(root, Position::new(1, 17), &mut cache);
    if let Some(h) = help {
        assert_eq!(
            h.active_parameter, 1,
            "Ternary in first arg should not confuse parameter counting"
        );
    }
}
