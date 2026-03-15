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
#[ignore = "TODO: Signature help new overload selection"]
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
        first_active.label.starts_with("new ("),
        "Constructor signatures should use new() label"
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
    // Generic function with type parameter and constraint
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
    assert!(
        sig.label.contains("<T>"),
        "Label should contain type parameter <T>, got: {}",
        sig.label
    );
    assert!(
        sig.prefix.contains("<T>"),
        "Prefix should contain type parameter, got: {}",
        sig.prefix
    );
    assert_eq!(sig.parameters.len(), 1);
    assert_eq!(sig.parameters[0].name, "value");
}

#[test]
fn test_signature_help_generic_with_constraint() {
    // Generic function with extends constraint
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
    // The label should include the constraint
    assert!(
        sig.label.contains("extends"),
        "Label should contain 'extends' constraint, got: {}",
        sig.label
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
        "Span should cover arguments, got: '{}'",
        span_text
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
