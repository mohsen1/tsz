use super::SignatureHelpProvider;
use super::parse_test_source;
use tsz_binder::BinderState;
use tsz_common::position::{LineMap, Position};
use tsz_parser::ParserState;
use tsz_solver::construction::TypeInterner;

#[test]
fn split_top_level_text_keeps_function_type_commas_grouped() {
    let parts =
        SignatureHelpProvider::<'_>::split_top_level_text("(err: Error) => void, ...object[]", ',');
    assert_eq!(parts, vec!["(err: Error) => void", "...object[]"]);
}

#[test]
fn tuple_variant_parameters_names_unlabeled_entries() {
    let params = SignatureHelpProvider::<'_>::tuple_variant_parameters(
        "[object, (err: Error) => void]",
        "rest",
    )
    .expect("tuple should parse");
    let labels: Vec<String> = params.into_iter().map(|param| param.label).collect();
    assert_eq!(
        labels,
        vec![
            "rest_0: object".to_string(),
            "rest_1: (err: Error) => void".to_string(),
        ]
    );
}

#[test]
fn textual_nested_incomplete_call_prefers_inner_callee() {
    let source = "declare function foo<T>(x: T, y: T): T;\ndeclare function bar<U>(x: U, y: U): U;\nfoo(bar(";
    let (parser, root) = parse_test_source(source);

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

    let offset = source.find("bar(").expect("bar(") + "bar(".len();
    let trigger = provider
        .find_textual_call_trigger(offset as u32)
        .expect("textual call trigger");
    assert_eq!(trigger.callee_name, "bar");
    assert_eq!(trigger.active_parameter, 0);

    let mut cache = None;
    let help = provider
        .signature_help_for_textual_call(root, offset as u32, &mut cache)
        .expect("textual call help");
    assert!(
        !help.signatures.is_empty(),
        "Should provide signatures for incomplete inner call"
    );
    assert_eq!(
        help.signatures[help.active_signature as usize].label,
        "bar(x: unknown, y: unknown): unknown"
    );
}

#[test]
fn nested_call_with_outer_unclosed_context_still_has_inner_signature_help() {
    let source = "declare function foo<T>(x: T, y: T): T;\ndeclare function bar<U>(x: U, y: U): U;\nfoo(bar()";
    let (parser, root) = parse_test_source(source);

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
    let help = provider
        .get_signature_help(root, Position::new(2, 8), &mut cache)
        .expect("nested incomplete call should return signature help");
    let typed_pos = source.find("bar(").expect("bar(") + "bar".len();
    assert_eq!(
        help.applicable_span_start as usize,
        typed_pos + 1,
        "Applicable span should start immediately after inner call '('"
    );
    assert!(
        help.signatures[help.active_signature as usize]
            .label
            .starts_with("bar("),
        "Expected active signature for inner callee `bar`"
    );
}

#[test]
fn trigger_sequence_for_nested_generic_call_keeps_signature_help_available() {
    let cases = [
        (
            "declare function foo<T>(x: T, y: T): T;\ndeclare function bar<U>(x: U, y: U): U;\nfoo(bar()",
            Position::new(2, 8),
            "bar(x: unknown, y: unknown): unknown",
        ),
        (
            "declare function foo<T>(x: T, y: T): T;\ndeclare function bar<U>(x: U, y: U): U;\nfoo(bar<)",
            Position::new(2, 8),
            "bar<U>(x: U, y: U): U",
        ),
        (
            "declare function foo<T>(x: T, y: T): T;\ndeclare function bar<U>(x: U, y: U): U;\nfoo(bar,)",
            Position::new(2, 8),
            "foo(x: <U>(x: U, y: U) => U, y: <U>(x: U, y: U) => U): <U>(x: U, y: U) => U",
        ),
    ];

    for (source, position, expected_label) in cases {
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
        let help = provider
            .get_signature_help(root, position, &mut cache)
            .expect("signature help should be available");
        let actual = &help.signatures[help.active_signature as usize].label;
        assert_eq!(actual, expected_label);
    }
}

#[test]
fn contextual_object_member_signature_preferred_over_outer_call() {
    let source = "interface I { m(n: number, s: string): void; }\ndeclare function takesObj(i: I): void;\ntakesObj({ m: () });";
    let (parser, root) = parse_test_source(source);

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
    let help = provider
        .get_signature_help(root, Position::new(2, 15), &mut cache)
        .expect("contextual signature help should be available");
    let active = &help.signatures[help.active_signature as usize].label;
    assert_eq!(active, "m(n: number, s: string): void");
}

#[test]
fn contextual_object_member_signature_accepts_unicode_member_name() {
    let source = "interface I { café(n: number, s: string): void; }\ndeclare function takesObj(i: I): void;\ntakesObj({ café: () });";
    let (parser, root) = parse_test_source(source);

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
    let cursor_offset = source.find("café: (").expect("unicode member") + "café: (".len();
    let position = line_map.offset_to_position(cursor_offset as u32, source);
    let mut cache = None;
    let help = provider
        .get_signature_help(root, position, &mut cache)
        .expect("unicode contextual signature help should be available");
    let active = &help.signatures[help.active_signature as usize].label;
    assert_eq!(active, "café(n: number, s: string): void");
}

#[test]
fn contextual_variable_initializer_type_alias_and_function_type() {
    let source = "type Cb = () => void;\nconst cb: Cb = ();\nconst cb2: () => void = ();";
    let (parser, root) = parse_test_source(source);

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
    let alias_help = provider
        .get_signature_help(root, Position::new(1, 16), &mut cache)
        .expect("alias contextual signature help");
    assert_eq!(
        alias_help.signatures[alias_help.active_signature as usize].label,
        "Cb(): void"
    );

    let fn_type_help = provider
        .get_signature_help(root, Position::new(2, 24), &mut cache)
        .expect("function type contextual signature help");
    assert_eq!(
        fn_type_help.signatures[fn_type_help.active_signature as usize].label,
        "cb2(): void"
    );
}

#[test]
fn contextual_variable_initializer_gives_function_type_help() {
    // Cursor sits inside the empty parens of a parenthesized expression that
    // is the initializer of a variable with a contextual function type.
    let source = "const cb2: () => void = ()";
    let cursor_offset = (source.rfind('(').expect("open paren") + 1) as u32;
    let (parser, root) = parse_test_source(source);

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

    let position = line_map.offset_to_position(cursor_offset, source);
    let mut cache = None;
    let help = provider
        .get_signature_help(root, position, &mut cache)
        .expect("function type contextual signature help");
    assert_eq!(
        help.signatures[help.active_signature as usize].label,
        "cb2(): void"
    );
}

#[test]
fn textual_type_argument_trigger_skips_function_declaration_name() {
    // After `<` in a function declaration head we are naming a new type
    // parameter, which must not trigger signature help.
    let source = "function f<\nx";
    let cursor_offset = (source.find('<').expect("less than") + 1) as u32;
    let (parser, root) = parse_test_source(source);

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

    let position = line_map.offset_to_position(cursor_offset, source);
    let mut cache = None;
    let help = provider.get_signature_help(root, position, &mut cache);
    assert!(
        help.is_none(),
        "type parameter declaration position should not produce signature help"
    );
}

#[test]
fn contextual_object_literal_method_from_typed_initializer() {
    // Cursor sits inside the parameter list of a method in an object literal
    // whose contextual type has a matching method signature.
    let source = "interface Obj { optionalMethod?: (current: any) => any; }\nconst o: Obj = {\n  optionalMethod() { return {}; }\n};";
    let cursor_offset =
        (source.find("optionalMethod()").expect("call") + "optionalMethod(".len()) as u32;
    let (parser, root) = parse_test_source(source);

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

    let position = line_map.offset_to_position(cursor_offset, source);
    let mut cache = None;
    let help = provider
        .get_signature_help(root, position, &mut cache)
        .expect("contextual object literal method signature help");
    assert_eq!(
        help.signatures[help.active_signature as usize].label,
        "optionalMethod(current: any): any"
    );
    assert_eq!(help.active_parameter, 0);
}

#[test]
fn overload_selection_prefers_matching_string_literal_signatures() {
    let source = "function x1(x: \"hi\");\nfunction x1(y: \"bye\");\nfunction x1(z: string);\nfunction x1(a: any) {}\nx1('');\nx1('hi');\nx1('bye');";
    let (parser, root) = parse_test_source(source);

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

    let empty_call = provider
        .get_signature_help(root, Position::new(4, 4), &mut cache)
        .expect("signature help for x1('')");
    assert_eq!(
        empty_call.signatures[empty_call.active_signature as usize].parameters[0].name,
        "z"
    );

    let hi_call = provider
        .get_signature_help(root, Position::new(5, 6), &mut cache)
        .expect("signature help for x1('hi')");
    assert_eq!(
        hi_call.signatures[hi_call.active_signature as usize].parameters[0].name,
        "x"
    );

    let bye_call = provider
        .get_signature_help(root, Position::new(6, 7), &mut cache)
        .expect("signature help for x1('bye')");
    assert_eq!(
        bye_call.signatures[bye_call.active_signature as usize].parameters[0].name,
        "y"
    );
}

#[test]
fn generic_inference_uses_argument_literal_for_signature_display() {
    let source = "declare function f<T extends string>(a: T, b: T, c: T): void;\nf(\"x\", );";
    let (parser, root) = parse_test_source(source);

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
    let help = provider
        .get_signature_help(root, Position::new(1, 7), &mut cache)
        .expect("signature help for generic inference");
    assert_eq!(
        help.signatures[help.active_signature as usize].label,
        "f(a: \"x\", b: \"x\", c: \"x\"): void"
    );
}

#[test]
fn no_signature_help_while_editing_identifier_before_call_open_paren() {
    let source = "/**\n * @param start The start\n * @param end The end\n * More text\n */\ndeclare function foo(start: number, end?: number);\n\nfo";
    let (parser, root) = parse_test_source(source);

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
    let help = provider.get_signature_help(root, Position::new(7, 2), &mut cache);
    assert!(
        help.is_none(),
        "expected no help before '(' while editing identifier, got {}",
        help.as_ref()
            .map(|h| h.signatures[h.active_signature as usize].label.as_str())
            .unwrap_or_default()
    );
}

#[test]
fn no_signature_help_after_closing_paren() {
    let source = "declare function foo(start: number, end?: number): void;\nfoo(10)";
    let (parser, root) = parse_test_source(source);

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
    assert!(
        help.is_none(),
        "expected no help after closing paren, got {}",
        help.as_ref()
            .map(|h| h.signatures[h.active_signature as usize].label.as_str())
            .unwrap_or_default()
    );
}

#[test]
fn no_signature_help_for_private_constructor_new_call() {
    let source = "class A { private constructor() {} }\nnew A(";
    let (parser, root) = parse_test_source(source);

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
    let help = provider.get_signature_help(root, Position::new(1, 6), &mut cache);
    assert!(
        help.is_none(),
        "expected no help for private constructor call, got {}",
        help.as_ref()
            .map(|h| h.signatures[h.active_signature as usize].label.as_str())
            .unwrap_or_default()
    );
}

#[test]
fn no_signature_help_for_protected_constructor_new_call() {
    let source = "class A { protected constructor() {} }\nnew A(";
    let (parser, root) = parse_test_source(source);

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
    let help = provider.get_signature_help(root, Position::new(1, 6), &mut cache);
    assert!(
        help.is_none(),
        "expected no help for protected constructor call, got {}",
        help.as_ref()
            .map(|h| h.signatures[h.active_signature as usize].label.as_str())
            .unwrap_or_default()
    );
}
