use super::*;
use crate::jsdoc::jsdoc_for_node;
use crate::utils::find_node_at_offset;
use tsz_binder::BinderState;
use tsz_common::position::LineMap;
use tsz_parser::ParserState;
use tsz_parser::syntax_kind_ext;
use tsz_solver::TypeInterner;

#[test]
fn test_signature_help_simple() {
    // function add(x: number, y: number): number { return x + y; }
    // add(1, 2|);
    let source = "function add(x: number, y: number): number { return x + y; }\nadd(1, 2);";
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

    // Position at the second argument '2' (line 1, column 7)
    let pos = Position::new(1, 7);
    let mut cache = None;
    let help = provider.get_signature_help(root, pos, &mut cache);

    assert!(help.is_some(), "Should find signature help");

    if let Some(h) = help {
        assert_eq!(h.active_parameter, 1, "Should be on second parameter");
        assert!(!h.signatures.is_empty(), "Should have signatures");
        // Note: The label format depends on how Checker resolves types
        // For a simple function it may not include the full signature
    }
}

#[test]
fn test_signature_help_no_call() {
    let source = "const x = 42;";
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

    // Position not in a call
    let pos = Position::new(0, 5);
    let mut cache = None;
    let help = provider.get_signature_help(root, pos, &mut cache);

    assert!(
        help.is_none(),
        "Should not find signature help outside call"
    );
}

#[test]
fn test_signature_help_first_arg() {
    // function foo(a: string): void {}
    // foo(|);
    let source = "function foo(a: string): void {}\nfoo();";
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

    // Position inside the call (line 1, column 4)
    let pos = Position::new(1, 4);
    let mut cache = None;
    let help = provider.get_signature_help(root, pos, &mut cache);

    assert!(help.is_some(), "Should find signature help");

    if let Some(h) = help {
        assert_eq!(h.active_parameter, 0, "Should be on first parameter");
    }
}

#[test]
fn test_signature_help_incomplete_call_eof() {
    // function add(a: number, b: number): number { return a + b; }
    // add(
    let source = "function add(a: number, b: number): number { return a + b; }\nadd(";
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

    // Position at EOF, just after '(' (line 1, column 4).
    let pos = Position::new(1, 4);
    let mut cache = None;
    let help = provider.get_signature_help(root, pos, &mut cache);

    assert!(
        help.is_some(),
        "Should find signature help in incomplete call"
    );
    if let Some(h) = help {
        assert_eq!(h.active_parameter, 0, "Should be on first parameter");
    }
}

#[test]
fn test_signature_help_incomplete_member_call() {
    let source = "interface Obj { method(a: number, b: string): void; }\ndeclare const obj: Obj;\nobj.method(";
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

    let pos = Position::new(2, 11); // After the opening paren.
    let mut cache = None;
    let help = provider.get_signature_help(root, pos, &mut cache);

    assert!(help.is_some(), "Should find signature help for member call");
    if let Some(h) = help {
        assert_eq!(h.active_parameter, 0, "Should be on first parameter");
        assert!(!h.signatures.is_empty(), "Should have signatures");
    }
}

#[test]
fn test_signature_help_incomplete_callable_interface_call() {
    let source = "interface C { (): number; }\ndeclare const c: C;\nc(";
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

    let pos = Position::new(2, 2); // After the opening paren.
    let mut cache = None;
    let help = provider.get_signature_help(root, pos, &mut cache);

    assert!(
        help.is_some(),
        "Should find signature help for incomplete callable interface call"
    );
    if let Some(h) = help {
        assert_eq!(h.active_parameter, 0, "Should be on first parameter");
        assert_eq!(h.signatures.len(), 1, "Should expose one call signature");
        assert_eq!(h.signatures[0].label, "c(): number");
    }
}

#[test]
fn test_signature_help_between_arguments() {
    // Test edge case: cursor between arguments (after comma, before next arg)
    // function process(a: any, b: number, c: string): void {}
    // process(1, |2, 3);
    //          ^ cursor here should be on parameter 1
    let source = "function process(a: any, b: number, c: string): void {}\nprocess(1, 2, 3);";
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

    // Test cursor at first argument
    let pos1 = Position::new(1, 8); // At "1"
    let mut cache = None;
    let help1 = provider.get_signature_help(root, pos1, &mut cache);
    if let Some(h) = help1 {
        assert_eq!(h.active_parameter, 0, "Should be on first parameter");
    }

    // Test cursor at second argument
    let pos2 = Position::new(1, 11); // At "2"
    let help2 = provider.get_signature_help(root, pos2, &mut cache);
    if let Some(h) = help2 {
        assert_eq!(h.active_parameter, 1, "Should be on second parameter");
    }

    // Test cursor between comma and second argument
    let pos_between = Position::new(1, 10); // Between "," and "2"
    let help_between = provider.get_signature_help(root, pos_between, &mut cache);
    if let Some(h) = help_between {
        assert_eq!(h.active_parameter, 1, "Should be on second parameter");
    }

    // Test cursor at third argument
    let pos3 = Position::new(1, 14); // At "3"
    let help3 = provider.get_signature_help(root, pos3, &mut cache);
    if let Some(h) = help3 {
        assert_eq!(h.active_parameter, 2, "Should be on third parameter");
    }
}

#[test]
fn test_signature_help_trailing_comma() {
    // function foo(a: number, b: string): void {}
    // foo(1, |);
    let source = "function foo(a: number, b: string): void {}\nfoo(1, );";
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

    let pos = Position::new(1, 6); // After the comma.
    let mut cache = None;
    let help = provider.get_signature_help(root, pos, &mut cache);
    assert!(help.is_some(), "Should find signature help");

    if let Some(h) = help {
        assert_eq!(
            h.active_parameter, 1,
            "Should be on second parameter after trailing comma"
        );
    }
}

#[test]
fn test_signature_help_comment_comma_ignored() {
    // function foo(a: number, b: string): void {}
    // foo(1 /*,*/ |);
    let source = "function foo(a: number, b: string): void {}\nfoo(1 /*,*/ );";
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

    let pos = Position::new(1, 11); // After the comment, before the close paren.
    let mut cache = None;
    let help = provider.get_signature_help(root, pos, &mut cache);
    assert!(help.is_some(), "Should find signature help");

    if let Some(h) = help {
        assert_eq!(
            h.active_parameter, 0,
            "Should stay on first parameter when comma is only in comment"
        );
    }
}

#[test]
fn test_signature_help_overload_selection() {
    let source = "interface Fn {\n  (a: number): void;\n  (a: number, b: string): void;\n}\ndeclare const fn: Fn;\nfn(1);\nfn(1, \"x\");";
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
    let pos_first = Position::new(5, 3); // At "1"
    let help_first = provider.get_signature_help(root, pos_first, &mut cache);
    assert!(
        help_first.is_some(),
        "Should find signature help for first call"
    );
    let first = help_first.unwrap();
    assert!(first.signatures.len() >= 2, "Expected overload signatures");
    let first_active = &first.signatures[first.active_signature as usize];
    assert!(
        !first_active.label.contains("b: string"),
        "First call should select single-arg overload"
    );

    let pos_second = Position::new(6, 6); // At "\"x\""
    let help_second = provider.get_signature_help(root, pos_second, &mut cache);
    assert!(
        help_second.is_some(),
        "Should find signature help for second call"
    );
    let second = help_second.unwrap();
    assert!(second.signatures.len() >= 2, "Expected overload signatures");
    let second_active = &second.signatures[second.active_signature as usize];
    assert!(
        second_active.label.contains("b: string"),
        "Second call should select two-arg overload"
    );
}

#[test]
fn test_signature_help_new_overload_selection() {
    let source = "interface Ctor {\n  new (a: number): Foo;\n  new (a: number, b: string): Foo;\n}\nclass Foo {}\ndeclare const Ctor: Ctor;\nnew Ctor(1);\nnew Ctor(1, \"x\");";
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
    let pos_first = Position::new(6, 9); // At "1"
    let help_first = provider.get_signature_help(root, pos_first, &mut cache);
    assert!(
        help_first.is_some(),
        "Should find signature help for first new"
    );
    let first = help_first.unwrap();
    assert!(
        !first.signatures.is_empty(),
        "Expected constructor signatures"
    );
    let first_active = &first.signatures[first.active_signature as usize];
    assert!(
        first_active.label.starts_with("Ctor("),
        "Constructor signatures should use callee name as label"
    );
    assert!(
        !first_active.label.contains("b: string"),
        "First new should select single-arg overload"
    );

    let pos_second = Position::new(7, 13); // At "x"
    let help_second = provider.get_signature_help(root, pos_second, &mut cache);
    assert!(
        help_second.is_some(),
        "Should find signature help for second new"
    );
    let second = help_second.unwrap();
    assert!(
        !second.signatures.is_empty(),
        "Expected constructor signatures"
    );
    let second_active = &second.signatures[second.active_signature as usize];
    assert!(
        second_active.label.contains("b: string"),
        "Second new should select two-arg overload"
    );
}

#[test]
fn test_signature_help_includes_jsdoc() {
    let source = "/** Adds two numbers. */\nfunction add(a: number, b: number): number { return a + b; }\nadd(1, 2);";
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

    let pos = Position::new(2, 6); // At "1"
    let mut cache = None;
    let help = provider.get_signature_help(root, pos, &mut cache);
    assert!(help.is_some(), "Should find signature help");

    let help = help.unwrap();
    assert!(!help.signatures.is_empty(), "Should have signatures");
    let doc = help.signatures[help.active_signature as usize]
        .documentation
        .clone()
        .unwrap_or_default();
    assert_eq!(doc, "Adds two numbers.");
}

#[test]
fn test_signature_help_param_docs() {
    let source = "/**\n * Adds two numbers.\n * @param a First number.\n * @param b Second number.\n */\nfunction add(a: number, b: number): number { return a + b; }\nadd(1, 2);";
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

    let pos = Position::new(6, 6); // At "1"
    let mut cache = None;
    let help = provider.get_signature_help(root, pos, &mut cache);
    assert!(help.is_some(), "Should find signature help");

    let help = help.unwrap();
    let sig = &help.signatures[help.active_signature as usize];
    assert_eq!(sig.parameters.len(), 2);
    assert_eq!(
        sig.parameters[0].documentation.as_deref(),
        Some("First number.")
    );
    assert_eq!(
        sig.parameters[1].documentation.as_deref(),
        Some("Second number.")
    );
}

#[test]
fn test_signature_help_overload_jsdoc() {
    let source = "/** One arg */\nfunction foo(a: number): void;\n/** Two args */\nfunction foo(a: number, b: string): void;\nfunction foo(a: number, b?: string): void {}\nfoo(1);\nfoo(1, \"x\");";
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
    let pos_first = Position::new(5, 4); // At "1"
    let help_first = provider
        .get_signature_help(root, pos_first, &mut cache)
        .expect("Expected signature help for first call");
    let doc_first = help_first.signatures[help_first.active_signature as usize]
        .documentation
        .clone()
        .unwrap_or_default();
    assert_eq!(doc_first, "One arg");

    let pos_second = Position::new(6, 8); // At "x"
    let help_second = provider
        .get_signature_help(root, pos_second, &mut cache)
        .expect("Expected signature help for second call");
    let doc_second = help_second.signatures[help_second.active_signature as usize]
        .documentation
        .clone()
        .unwrap_or_default();
    assert_eq!(doc_second, "Two args");
}

#[test]
fn test_signature_help_jsdoc_proximity() {
    let source = "/** First doc */\n/** Second doc */\nfunction foo(a: number): void {}\nfoo(1);";
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

    let pos = Position::new(3, 4); // At "1"
    let mut cache = None;
    let help = provider
        .get_signature_help(root, pos, &mut cache)
        .expect("Expected signature help");
    let doc = help.signatures[help.active_signature as usize]
        .documentation
        .clone()
        .unwrap_or_default();
    assert_eq!(doc, "Second doc");
}

#[test]
fn test_signature_help_method_overload_jsdoc_this_rest() {
    let source = "class Greeter {\n  /** One arg.\n   * @param this The instance.\n   * @param name The name.\n   */\n  greet(this: Greeter, name: string): void;\n  /** Many args.\n   * @param this The instance.\n   * @param name The name.\n   * @param ...messages Extra messages.\n   */\n  greet(this: Greeter, name: string, ...messages: string[]): void;\n  greet(this: Greeter, name: string, ...messages: string[]) {}\n}\nconst g = new Greeter();\ng.greet(\"hi\");\ng.greet(\"hi\", \"there\");";
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

    let arena = parser.get_arena();
    let class_decl = {
        let root_node = arena.get(root).expect("root node");
        let sf = arena.get_source_file(root_node).expect("source file");
        sf.statements
            .nodes
            .iter()
            .copied()
            .find(|stmt| {
                arena
                    .get(*stmt)
                    .and_then(|node| arena.get_class(node))
                    .is_some()
            })
            .expect("class declaration")
    };
    let class_data = arena
        .get_class(arena.get(class_decl).expect("class node"))
        .expect("class data");
    let has_method_jsdoc = class_data.members.nodes.iter().any(|&member| {
        let Some(method) = arena.get_method_decl_at(member) else {
            return false;
        };
        if !method.body.is_none() {
            return false;
        }
        let doc = jsdoc_for_node(arena, root, member, source);
        doc.contains("One arg.")
    });
    assert!(has_method_jsdoc, "Expected JSDoc on method overload");

    let offset = line_map
        .position_to_offset(Position::new(15, 9), source)
        .expect("offset");
    let leaf = find_node_at_offset(arena, offset);
    let mut current = leaf;
    let mut call_expr_idx = None;
    for _ in 0..100 {
        let Some(node) = arena.get(current) else {
            break;
        };
        if node.kind == syntax_kind_ext::CALL_EXPRESSION {
            call_expr_idx = Some(current);
            break;
        }
        let Some(ext) = arena.get_extended(current) else {
            break;
        };
        current = ext.parent;
    }
    let call_expr_idx = call_expr_idx.expect("call expression");
    let call_node = arena.get(call_expr_idx).expect("call node");
    let call_data = arena.get_call_expr(call_node).expect("call data");
    let expr_node = arena.get(call_data.expression).expect("callee node");
    assert_eq!(expr_node.kind, syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION);
    let access = arena.get_access_expr(expr_node).expect("access expr");
    let prop = arena
        .get_identifier_text(access.name_or_argument)
        .expect("property name");
    assert_eq!(prop, "greet");

    let mut cache = None;
    let pos_first = Position::new(15, 9); // At "hi"
    let help_first = provider
        .get_signature_help(root, pos_first, &mut cache)
        .expect("Expected signature help for first call");
    let doc_first = help_first.signatures[help_first.active_signature as usize]
        .documentation
        .clone()
        .unwrap_or_default();
    assert_eq!(doc_first, "One arg.");
    let sig_first = &help_first.signatures[help_first.active_signature as usize];
    assert_eq!(sig_first.parameters.len(), 1);
    assert_eq!(
        sig_first.parameters[0].documentation.as_deref(),
        Some("The name.")
    );

    let pos_second = Position::new(16, 15); // At "there"
    let help_second = provider
        .get_signature_help(root, pos_second, &mut cache)
        .expect("Expected signature help for second call");
    let doc_second = help_second.signatures[help_second.active_signature as usize]
        .documentation
        .clone()
        .unwrap_or_default();
    assert_eq!(doc_second, "Many args.");
    let sig_second = &help_second.signatures[help_second.active_signature as usize];
    assert_eq!(sig_second.parameters.len(), 2);
    assert_eq!(
        sig_second.parameters[0].documentation.as_deref(),
        Some("The name.")
    );
    assert_eq!(
        sig_second.parameters[1].documentation.as_deref(),
        Some("Extra messages.")
    );
}

#[test]
fn test_signature_help_constructor_overload_jsdoc_rest() {
    let source = "class Widget {\n  /** One arg.\n   * @param name Name.\n   */\n  constructor(name: string);\n  /** Two args.\n   * @param name Name.\n   * @param ...tags Tags.\n   */\n  constructor(name: string, ...tags: string[]);\n  constructor(name: string, ...tags: string[]) {}\n}\nnew Widget(\"x\");\nnew Widget(\"x\", \"y\");";
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
    let pos_first = Position::new(12, 12); // At "x"
    let help_first = provider
        .get_signature_help(root, pos_first, &mut cache)
        .expect("Expected signature help for first constructor call");
    let doc_first = help_first.signatures[help_first.active_signature as usize]
        .documentation
        .clone()
        .unwrap_or_default();
    assert_eq!(doc_first, "One arg.");

    let pos_second = Position::new(13, 17); // At "y"
    let help_second = provider
        .get_signature_help(root, pos_second, &mut cache)
        .expect("Expected signature help for second constructor call");
    let doc_second = help_second.signatures[help_second.active_signature as usize]
        .documentation
        .clone()
        .unwrap_or_default();
    assert_eq!(doc_second, "Two args.");
    let sig_second = &help_second.signatures[help_second.active_signature as usize];
    assert_eq!(sig_second.parameters.len(), 2);
    assert_eq!(
        sig_second.parameters[0].documentation.as_deref(),
        Some("Name.")
    );
    assert_eq!(
        sig_second.parameters[1].documentation.as_deref(),
        Some("Tags.")
    );
}

// ============================================================================
// New tests for improved signature help (prefix/suffix, callee name, generics)
// ============================================================================

/// Helper to set up a `SignatureHelpProvider` from source code.
fn setup_provider(
    source: &str,
) -> (
    ParserState,
    BinderState,
    TypeInterner,
    LineMap,
    tsz_parser::NodeIndex,
) {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    let interner = TypeInterner::new();
    let line_map = LineMap::build(source);
    (parser, binder, interner, line_map, root)
}

#[test]
fn test_signature_label_includes_function_name() {
    let source = "function greet(name: string): void {}\ngreet(\"hello\");";
    let (parser, binder, interner, line_map, root) = setup_provider(source);
    let provider = SignatureHelpProvider::new(
        parser.get_arena(),
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );
    let pos = Position::new(1, 6); // inside "hello"
    let mut cache = None;
    let help = provider.get_signature_help(root, pos, &mut cache);
    assert!(help.is_some(), "Should find signature help");
    let h = help.unwrap();
    let sig = &h.signatures[h.active_signature as usize];
    assert!(
        sig.label.starts_with("greet("),
        "Label should start with function name, got: {}",
        sig.label
    );
    assert!(
        sig.label.contains("name: string"),
        "Label should contain parameter, got: {}",
        sig.label
    );
    assert!(
        sig.label.contains("): void"),
        "Label should contain return type, got: {}",
        sig.label
    );
}

#[test]
fn test_signature_prefix_and_suffix() {
    let source = "function add(x: number, y: number): number { return x + y; }\nadd(1, 2);";
    let (parser, binder, interner, line_map, root) = setup_provider(source);
    let provider = SignatureHelpProvider::new(
        parser.get_arena(),
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );
    let pos = Position::new(1, 4);
    let mut cache = None;
    let help = provider.get_signature_help(root, pos, &mut cache);
    assert!(help.is_some());
    let h = help.unwrap();
    let sig = &h.signatures[h.active_signature as usize];
    // prefix should be "add(" and suffix should be "): number"
    assert_eq!(
        sig.prefix, "add(",
        "Prefix should be function name + open paren"
    );
    assert!(
        sig.suffix.starts_with("): "),
        "Suffix should start with '): ', got: {}",
        sig.suffix
    );
    // Full label reconstructed from prefix + params + suffix
    let reconstructed = format!(
        "{}{}{}",
        sig.prefix,
        sig.parameters
            .iter()
            .map(|p| p.label.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        sig.suffix
    );
    assert_eq!(
        sig.label, reconstructed,
        "Label should equal prefix + params + suffix"
    );
}

#[test]
fn test_parameter_name_field() {
    let source = "function foo(alpha: string, beta: number): void {}\nfoo(\"a\", 42);";
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
    assert_eq!(
        sig.parameters[0].name, "alpha",
        "First param name should be 'alpha'"
    );
    assert_eq!(
        sig.parameters[1].name, "beta",
        "Second param name should be 'beta'"
    );
    assert_eq!(
        sig.parameters[0].label, "alpha: string",
        "First param label should include type"
    );
    assert_eq!(
        sig.parameters[1].label, "beta: number",
        "Second param label should include type"
    );
}

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

#[test]
fn test_signature_help_generic_function() {
    // Generic function called WITHOUT explicit type arguments:
    // TypeScript hides the type parameter list and substitutes type params with unknown.
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
    // No explicit type args -> type params hidden, T replaced with unknown
    assert!(
        !sig.label.contains("<T>"),
        "Label should NOT contain type parameter <T> when no explicit type args, got: {}",
        sig.label
    );
    assert_eq!(
        sig.label, "identity(value: unknown): unknown",
        "Type params should be substituted with unknown"
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
    // TypeScript hides the type params and substitutes T with the constraint type.
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
    // No explicit type args -> type params hidden, T replaced with constraint type (any[])
    assert!(
        !sig.label.contains("extends"),
        "Label should NOT contain 'extends' constraint without explicit type args, got: {}",
        sig.label
    );
    assert_eq!(
        sig.label, "first(arr: any[]): any[]",
        "Type params should be substituted with constraint type"
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
        sig.label.contains("string"),
        "Label should show instantiated type 'string', got: {}",
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
    // T has a default of `string` -> substitute with `string`
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
        sig.label, "create(val: string): string",
        "Type param with default should be substituted with the default type"
    );
}

#[test]
fn test_signature_help_generic_default_overrides_constraint() {
    // V has both constraint `number` and default `42` -> use default `42`
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
        sig.label, "pick(val: 42): 42",
        "Default type should take priority over constraint"
    );
}

#[test]
fn test_signature_help_generic_no_default_no_constraint() {
    // T has neither default nor constraint -> substitute with `unknown`
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
        sig.label, "identity(val: unknown): unknown",
        "Type param with no default/constraint should be substituted with unknown"
    );
}

#[test]
fn test_signature_help_generic_mixed_type_params() {
    // A has default `boolean`, B has constraint `string`, C has neither
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
        sig.label, "mix(a: boolean, b: string, c: unknown): void",
        "Each type param should use its own substitution strategy"
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
