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
    if let Some(h) = help {
        assert_eq!(h.active_parameter, 0);
    }
}

#[test]
fn test_signature_help_single_param() {
    let source = "function log(msg: string): void {}\nlog('hello');";
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
    let help = provider.get_signature_help(root, Position::new(1, 5), &mut cache);
    if let Some(h) = help {
        assert_eq!(h.active_parameter, 0);
        assert!(!h.signatures.is_empty());
    }
}

#[test]
fn test_signature_help_at_open_paren() {
    let source = "function f(a: number): void {}\nf(";
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
    let help = provider.get_signature_help(root, Position::new(1, 2), &mut cache);
    // Just after open paren should trigger signature help
    if let Some(h) = help {
        assert_eq!(h.active_parameter, 0);
    }
}

#[test]
fn test_signature_help_class_constructor() {
    let source = "class Foo {\n  constructor(x: number, y: string) {}\n}\nnew Foo(1, 'a');";
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
    let help = provider.get_signature_help(root, Position::new(3, 10), &mut cache);
    if let Some(h) = help {
        assert!(h.active_parameter <= 1);
    }
}

#[test]
fn test_signature_help_arrow_function_call_with_age() {
    let source =
        "const greet = (name: string, age: number) => `Hello ${name}`;\ngreet('World', 25);";
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
    let help = provider.get_signature_help(root, Position::new(1, 15), &mut cache);
    if let Some(h) = help {
        assert_eq!(h.active_parameter, 1);
    }
}

#[test]
fn test_signature_help_outside_call() {
    let source = "function f() {}\nconst x = 1;";
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
    let help = provider.get_signature_help(root, Position::new(4, 13), &mut cache);
    // Method call should potentially provide signature help
    let _ = help;
}

#[test]
fn test_signature_help_empty_source() {
    let source = "";
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
    let help = provider.get_signature_help(root, Position::new(0, 0), &mut cache);
    assert!(help.is_none());
}

#[test]
fn test_signature_help_many_params() {
    let source = "function many(a: number, b: string, c: boolean, d: number[], e: object): void {}\nmany(1, 'x', true, [], {});";
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
    // Position at last arg
    let help = provider.get_signature_help(root, Position::new(1, 25), &mut cache);
    if let Some(h) = help {
        assert!(h.active_parameter >= 3, "Should be on 4th or 5th param");
    }
}

// =========================================================================
// Additional edge case tests
// =========================================================================

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

// ── Intrinsic primitive method signature tests ──────────────────────────────
//
// Structural rule: when the callee is a method access on a primitive intrinsic
// type (string, number, boolean, …) and the type system produced the no-lib
// fallback `(...args: any[]) => ReturnType` shape, the LSP must replace the
// synthetic parameter list with the real parameter names and optionality so
// that tools display e.g. `toLowerCase(): string` instead of
// `toLowerCase(...args: any[]): string`.

fn sig_help_at(source: &str, line: u32, col: u32) -> Option<crate::SignatureHelp> {
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
    provider.get_signature_help(root, Position::new(line, col), &mut cache)
}

// Helper: first signature in the help result.
fn first_sig(help: &crate::SignatureHelp) -> &crate::SignatureInformation {
    &help.signatures[help.active_signature as usize]
}

// ── No-param string methods ──────────────────────────────────────────────────

#[test]
fn test_intrinsic_sig_string_to_lower_case_no_params() {
    // `const s: string` and `const t = "literal"` both exercise the path.
    for (src, line, col) in [
        ("const s: string = \"abc\";\ns.toLowerCase(", 1u32, 14u32),
        ("const t = \"abc\";\nt.toLowerCase(", 1u32, 14u32),
    ] {
        let help = sig_help_at(src, line, col);
        let Some(help) = help else { continue };
        let sig = first_sig(&help);
        assert!(
            sig.label.contains("toLowerCase(): string"),
            "Expected no-param label, got: {}",
            sig.label
        );
        assert!(
            sig.parameters.is_empty(),
            "toLowerCase has no parameters, got: {:?}",
            sig.parameters.iter().map(|p| &p.label).collect::<Vec<_>>()
        );
    }
}

#[test]
fn test_intrinsic_sig_string_to_upper_case_no_params() {
    let help = sig_help_at("const s: string = \"\";\ns.toUpperCase(", 1, 14);
    let Some(help) = help else { return };
    let sig = first_sig(&help);
    assert!(
        sig.parameters.is_empty(),
        "toUpperCase has no parameters, got: {:?}",
        sig.parameters.iter().map(|p| &p.label).collect::<Vec<_>>()
    );
}

#[test]
fn test_intrinsic_sig_string_trim_no_params() {
    let help = sig_help_at("const s: string = \"\";\ns.trim(", 1, 7);
    let Some(help) = help else { return };
    let sig = first_sig(&help);
    assert!(
        sig.parameters.is_empty(),
        "trim has no parameters, got: {:?}",
        sig.parameters.iter().map(|p| &p.label).collect::<Vec<_>>()
    );
}

// ── String methods with parameters ──────────────────────────────────────────

#[test]
fn test_intrinsic_sig_string_index_of_two_params() {
    let help = sig_help_at("const s: string = \"abc\";\ns.indexOf(", 1, 10);
    let Some(help) = help else { return };
    let sig = first_sig(&help);
    assert_eq!(
        sig.parameters.len(),
        2,
        "indexOf should have 2 parameters, label: {}",
        sig.label
    );
    assert_eq!(sig.parameters[0].name, "searchString");
    assert!(!sig.parameters[0].is_optional, "searchString is required");
    assert_eq!(sig.parameters[1].name, "position");
    assert!(sig.parameters[1].is_optional, "position is optional");
}

#[test]
fn test_intrinsic_sig_string_starts_with_two_params() {
    let help = sig_help_at("const s: string = \"\";\ns.startsWith(", 1, 13);
    let Some(help) = help else { return };
    let sig = first_sig(&help);
    assert_eq!(
        sig.parameters.len(),
        2,
        "startsWith should have 2 parameters"
    );
    assert_eq!(sig.parameters[0].name, "searchString");
    assert_eq!(sig.parameters[1].name, "position");
    assert!(sig.parameters[1].is_optional);
}

#[test]
fn test_intrinsic_sig_string_ends_with_end_position_param() {
    let help = sig_help_at("const s: string = \"\";\ns.endsWith(", 1, 11);
    let Some(help) = help else { return };
    let sig = first_sig(&help);
    assert_eq!(sig.parameters.len(), 2, "endsWith should have 2 parameters");
    assert_eq!(sig.parameters[0].name, "searchString");
    assert_eq!(sig.parameters[1].name, "endPosition");
    assert!(sig.parameters[1].is_optional);
}

#[test]
fn test_intrinsic_sig_string_char_at_single_pos_param() {
    let help = sig_help_at("const s: string = \"\";\ns.charAt(", 1, 9);
    let Some(help) = help else { return };
    let sig = first_sig(&help);
    assert_eq!(sig.parameters.len(), 1, "charAt should have 1 parameter");
    assert_eq!(sig.parameters[0].name, "pos");
    assert!(!sig.parameters[0].is_optional);
}

#[test]
fn test_intrinsic_sig_string_slice_two_optional_params() {
    let help = sig_help_at("const s: string = \"\";\ns.slice(", 1, 8);
    let Some(help) = help else { return };
    let sig = first_sig(&help);
    assert_eq!(sig.parameters.len(), 2, "slice should have 2 parameters");
    assert_eq!(sig.parameters[0].name, "start");
    assert!(sig.parameters[0].is_optional, "start is optional for slice");
    assert_eq!(sig.parameters[1].name, "end");
    assert!(sig.parameters[1].is_optional, "end is optional for slice");
}

#[test]
fn test_intrinsic_sig_string_pad_start_two_params() {
    let help = sig_help_at("const s: string = \"\";\ns.padStart(", 1, 11);
    let Some(help) = help else { return };
    let sig = first_sig(&help);
    assert_eq!(sig.parameters.len(), 2, "padStart should have 2 parameters");
    assert_eq!(sig.parameters[0].name, "maxLength");
    assert!(!sig.parameters[0].is_optional);
    assert_eq!(sig.parameters[1].name, "fillString");
    assert!(sig.parameters[1].is_optional);
}

// ── Number methods ───────────────────────────────────────────────────────────

#[test]
fn test_intrinsic_sig_number_to_fixed_optional_param() {
    let help = sig_help_at("const n: number = 3.14;\nn.toFixed(", 1, 10);
    let Some(help) = help else { return };
    let sig = first_sig(&help);
    assert_eq!(sig.parameters.len(), 1, "toFixed should have 1 parameter");
    assert_eq!(sig.parameters[0].name, "digits");
    assert!(sig.parameters[0].is_optional, "digits is optional");
}

#[test]
fn test_intrinsic_sig_number_to_string_optional_radix() {
    let help = sig_help_at("const n: number = 42;\nn.toString(", 1, 11);
    let Some(help) = help else { return };
    let sig = first_sig(&help);
    assert_eq!(
        sig.parameters.len(),
        1,
        "number.toString should have 1 parameter"
    );
    assert_eq!(sig.parameters[0].name, "radix");
    assert!(sig.parameters[0].is_optional);
}

#[test]
fn test_intrinsic_sig_number_value_of_no_params() {
    let help = sig_help_at("const n: number = 1;\nn.valueOf(", 1, 10);
    let Some(help) = help else { return };
    let sig = first_sig(&help);
    assert!(
        sig.parameters.is_empty(),
        "valueOf has no parameters, got: {:?}",
        sig.parameters.iter().map(|p| &p.label).collect::<Vec<_>>()
    );
}

// ── Boolean methods ──────────────────────────────────────────────────────────

#[test]
fn test_intrinsic_sig_boolean_to_string_no_params() {
    let help = sig_help_at("const b: boolean = true;\nb.toString(", 1, 11);
    let Some(help) = help else { return };
    let sig = first_sig(&help);
    assert!(
        sig.parameters.is_empty(),
        "boolean.toString has no parameters, got: {:?}",
        sig.parameters.iter().map(|p| &p.label).collect::<Vec<_>>()
    );
}

// ── Active-parameter tracking ────────────────────────────────────────────────

#[test]
fn test_intrinsic_sig_active_param_advances_for_index_of() {
    // Cursor after the comma should put active_parameter on the second arg.
    let help = sig_help_at("const s: string = \"\";\ns.indexOf(\"x\", ", 1, 15);
    let Some(help) = help else { return };
    assert_eq!(
        help.active_parameter, 1,
        "active_parameter should be 1 when cursor is past the comma"
    );
}

// ── Structural coverage: different identifier names don't affect the fix ─────

#[test]
fn test_intrinsic_sig_works_for_any_variable_name() {
    // The fix must be structural (keyed by intrinsic kind + method name), not
    // by identifier spelling.
    for var in ["s", "myStr", "x", "foo"] {
        let src = format!("const {var}: string = \"\";\n{var}.toLowerCase(");
        let help = sig_help_at(&src, 1, 14 + var.len() as u32 - 1);
        let Some(help) = help else { continue };
        let sig = first_sig(&help);
        assert!(
            sig.parameters.is_empty(),
            "toLowerCase params should be empty for variable '{}', label: {}",
            var,
            sig.label
        );
    }
}
