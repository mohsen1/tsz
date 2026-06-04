#[test]
fn destructuring_array() {
    // `const [a, , b, ...rest] = arr;`
    let (parser, root) =
        parse_clean_source("const [a, , b, ...rest] = arr;", "array destructuring");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let var = arena.get_variable(stmt_node).expect("var");
    let decl_list_node = arena.get(var.declarations.nodes[0]).expect("decl list");
    let decl_list = arena.get_variable(decl_list_node).expect("decl list data");
    let decl_node = arena.get(decl_list.declarations.nodes[0]).expect("decl");
    let decl = arena
        .get_variable_declaration(decl_node)
        .expect("decl data");
    let name_node = arena.get(decl.name).expect("name");
    assert_eq!(
        name_node.kind,
        syntax_kind_ext::ARRAY_BINDING_PATTERN,
        "should be array binding"
    );
}

#[test]
fn array_rest_initializer_preserves_in_expression_in_for_header_recovery() {
    let source = "for (var [...x = a in b] ;;) {}";
    let (parser, root) = parse_source(source);
    let arena = parser.get_arena();
    let sf = arena.get_source_file_at(root).expect("source file");
    let for_node = arena
        .get(sf.statements.nodes[0])
        .expect("for statement node");
    let for_stmt = arena.get_loop(for_node).expect("for statement");
    let var_node = arena
        .get(for_stmt.initializer)
        .expect("for initializer node");
    let var_decl_list = arena
        .get_variable(var_node)
        .expect("for initializer declaration list");
    let decl_node = arena
        .get(var_decl_list.declarations.nodes[0])
        .expect("declaration");
    let decl = arena
        .get_variable_declaration(decl_node)
        .expect("declaration data");
    let binding_node = arena.get(decl.name).expect("binding pattern");
    let binding = arena
        .get_binding_pattern(binding_node)
        .expect("array binding pattern");
    let rest_node = arena.get(binding.elements.nodes[0]).expect("rest element");
    let rest = arena
        .get_binding_element(rest_node)
        .expect("rest binding element");
    let initializer_node = arena.get(rest.initializer).expect("initializer");

    assert_eq!(
        initializer_node.kind,
        syntax_kind_ext::BINARY_EXPRESSION,
        "rest initializer should preserve `a in b` as a binary expression"
    );
    let (_, op, _) = get_binary(arena, rest.initializer);
    assert_eq!(op, SyntaxKind::InKeyword as u16);
}

#[test]
fn destructuring_nested() {
    // `const { a: { b } } = obj;`
    let (_parser, _) = parse_clean_source("const { a: { b } } = obj;", "nested destructuring");
}

#[test]
fn destructuring_with_defaults() {
    // `const { a = 1, b = 2 } = obj;`
    let (_parser, _) = parse_clean_source(
        "const { a = 1, b = 2 } = obj;",
        "destructuring with defaults",
    );
}

#[test]
fn type_import() {
    // `type T = import('module').Foo`
    let (parser, root) = parse_clean_source("type T = import('module').Foo;", "import type");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("alias");
    let type_node = arena.get(alias.type_node).expect("type");
    assert!(type_node.kind != 0, "should have valid kind");
}

#[test]
fn type_reference_qualified_name_span_excludes_type_arguments() {
    let source = "type T = Foo.Bar<Baz>;";
    let (parser, root) = parse_clean_source(source, "qualified type reference span");

    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("alias");
    let type_node = arena.get(alias.type_node).expect("type");
    assert_eq!(type_node.kind, syntax_kind_ext::TYPE_REFERENCE);

    let type_ref = arena.get_type_ref(type_node).expect("type ref");
    assert_eq!(node_text(arena, source, type_ref.type_name), "Foo.Bar");
    assert_eq!(node_text(arena, source, alias.type_node), "Foo.Bar<Baz>");
}

#[test]
fn type_query_qualified_name_span_excludes_type_arguments() {
    let source = "type T = typeof ns.Foo<Bar>;";
    let (parser, root) = parse_clean_source(source, "type query span");

    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("alias");
    let type_node = arena.get(alias.type_node).expect("type");
    assert_eq!(type_node.kind, syntax_kind_ext::TYPE_QUERY);

    let type_query = arena.get_type_query(type_node).expect("type query");
    assert_eq!(node_text(arena, source, type_query.expr_name), "ns.Foo");
    assert_eq!(
        node_text(arena, source, alias.type_node),
        "typeof ns.Foo<Bar>"
    );
}

#[test]
fn import_type_qualified_name_span_excludes_type_arguments() {
    let source = "type T = import('m').Foo<Bar>;";
    let (parser, root) = parse_clean_source(source, "import type span");

    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("alias");
    let type_node = arena.get(alias.type_node).expect("type");
    assert_eq!(type_node.kind, syntax_kind_ext::TYPE_REFERENCE);

    let type_ref = arena.get_type_ref(type_node).expect("type ref");
    assert_eq!(
        node_text(arena, source, type_ref.type_name),
        "import('m').Foo"
    );
    assert_eq!(
        node_text(arena, source, alias.type_node),
        "import('m').Foo<Bar>"
    );
}

#[test]
fn intrinsic_type_keyword_recovery_stops_before_qualified_name() {
    let source = "var v: void.x;";
    let (parser, root) = parse_source(source);
    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        codes.contains(&diagnostic_codes::EXPECTED),
        "expected TS1005 for malformed intrinsic qualified name, got {:?}",
        parser.get_diagnostics()
    );

    let arena = parser.get_arena();
    let type_annotation = get_var_type_annotation(arena, root);
    let type_node = arena.get(type_annotation).expect("type");
    assert_eq!(type_node.kind, SyntaxKind::VoidKeyword as u16);
    assert_eq!(node_text(arena, source, type_annotation), "void");
}

#[test]
fn unique_symbol_keeps_symbol_as_type_reference() {
    let source = "type T = unique symbol;";
    let (parser, root) = parse_clean_source(source, "unique symbol");

    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("alias");
    let type_node = arena.get(alias.type_node).expect("type");
    assert_eq!(type_node.kind, syntax_kind_ext::TYPE_OPERATOR);

    let type_op = arena.get_type_operator(type_node).expect("type operator");
    assert_eq!(type_op.operator, SyntaxKind::UniqueKeyword as u16);

    let inner_node = arena.get(type_op.type_node).expect("inner type");
    assert_eq!(inner_node.kind, syntax_kind_ext::TYPE_REFERENCE);
    let type_ref = arena.get_type_ref(inner_node).expect("type ref");
    assert_eq!(node_text(arena, source, type_ref.type_name), "symbol");
}

#[test]
fn super_type_arguments_report_parser_error_and_recover_to_call() {
    let source = "class Derived extends Base { method() { super<T>(0); } }";
    let (parser, root) = parse_source(source);
    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        codes.contains(&diagnostic_codes::SUPER_MAY_NOT_USE_TYPE_ARGUMENTS),
        "expected TS2754 for super type arguments, got {:?}",
        parser.get_diagnostics()
    );

    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    let member_node = arena.get(class.members.nodes[0]).expect("member");
    let method = arena.get_method_decl(member_node).expect("method");
    let body_node = arena.get(method.body).expect("body");
    let block = arena.get_block(body_node).expect("block");
    let expr_stmt_node = arena
        .get(block.statements.nodes[0])
        .expect("expr stmt node");
    let expr_stmt = arena
        .get_expression_statement(expr_stmt_node)
        .expect("expr stmt");
    let call_node = arena.get(expr_stmt.expression).expect("call");
    assert_eq!(call_node.kind, syntax_kind_ext::CALL_EXPRESSION);

    let call = arena.get_call_expr(call_node).expect("call data");
    assert!(
        call.type_arguments.is_some(),
        "recovery should preserve type arguments on super calls for later checker recovery"
    );
    let callee_node = arena.get(call.expression).expect("callee");
    assert_eq!(callee_node.kind, SyntaxKind::SuperKeyword as u16);
}

#[test]
fn accessor_children_include_body_once() {
    let source = "class C { get x() { return 1; } }";
    let (parser, root) = parse_clean_source(source, "class accessor");

    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    let accessor_idx = class.members.nodes[0];
    let accessor_node = arena.get(accessor_idx).expect("accessor");
    let accessor = arena.get_accessor(accessor_node).expect("accessor data");

    let children = arena.get_children(accessor_idx);
    assert_eq!(
        children
            .iter()
            .filter(|&&child| child == accessor.body)
            .count(),
        1,
        "accessor body should appear exactly once in traversal children"
    );
}

#[test]
fn class_field_type_annotation_dot_reports_ts1442() {
    let source = "class C { a: this.foo; }";
    let (parser, _) = parse_source(source);
    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();

    assert!(
        codes.contains(&diagnostic_codes::EXPECTED_FOR_PROPERTY_INITIALIZER),
        "expected TS1442 for class field type annotation followed by dot access, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn class_field_type_annotation_call_reports_ts1441() {
    let source = "class Base {} class C extends Base { a: super(); }";
    let (parser, _) = parse_source(source);
    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();

    assert!(
        codes.contains(&diagnostic_codes::CANNOT_START_A_FUNCTION_CALL_IN_A_TYPE_ANNOTATION),
        "expected TS1441 for class field type annotation followed by call syntax, got {:?}",
        parser.get_diagnostics()
    );
    assert!(
        !codes.contains(&diagnostic_codes::EXPECTED_FOR_PROPERTY_INITIALIZER),
        "did not expect TS1442 once call syntax is classified as TS1441, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn type_mapped_with_modifiers() {
    // `type T = { readonly [K in keyof T]-?: T[K] }`
    let (parser, root) = parse_clean_source(
        "type T = { readonly [K in keyof T]-?: T[K] };",
        "mapped type with modifiers",
    );
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("alias");
    let type_node = arena.get(alias.type_node).expect("type");
    assert_eq!(type_node.kind, syntax_kind_ext::MAPPED_TYPE);
    let mapped = arena.get_mapped_type(type_node).expect("mapped");
    assert!(mapped.readonly_token.is_some(), "should have readonly");
    assert!(mapped.question_token.is_some(), "should have question");
}

#[test]
fn type_type_literal() {
    // `type T = { x: string; y: number }`
    let (parser, root) = parse_clean_source("type T = { x: string; y: number };", "type literal");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("alias");
    let type_node = arena.get(alias.type_node).expect("type");
    assert_eq!(
        type_node.kind,
        syntax_kind_ext::TYPE_LITERAL,
        "should be type literal"
    );
    let lit = arena
        .get_type_literal(type_node)
        .expect("type literal data");
    assert_eq!(lit.members.nodes.len(), 2, "should have 2 members");
}

#[test]
fn type_union_intersection_precedence() {
    // `A & B | C & D` should parse as `(A & B) | (C & D)` — intersection binds tighter
    let (parser, root) =
        parse_clean_source("type T = A & B | C & D;", "union/intersection precedence");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("alias");
    let type_node = arena.get(alias.type_node).expect("type");
    assert_eq!(
        type_node.kind,
        syntax_kind_ext::UNION_TYPE,
        "top should be union (lower precedence)"
    );
    let composite = arena.get_composite_type(type_node).expect("composite");
    assert_eq!(
        composite.types.nodes.len(),
        2,
        "union should have 2 branches"
    );
    // Each branch should be an intersection
    let left = arena.get(composite.types.nodes[0]).expect("left");
    assert_eq!(
        left.kind,
        syntax_kind_ext::INTERSECTION_TYPE,
        "left should be intersection"
    );
    let right = arena.get(composite.types.nodes[1]).expect("right");
    assert_eq!(
        right.kind,
        syntax_kind_ext::INTERSECTION_TYPE,
        "right should be intersection"
    );
}

#[test]
fn template_no_substitution() {
    // `const x = \`hello\``
    let (parser, init) = parse_clean_var_initializer("const x = `hello`;", "no-sub template");
    let arena = parser.get_arena();
    let node = arena.get(init).expect("init");
    assert_eq!(
        node.kind,
        SyntaxKind::NoSubstitutionTemplateLiteral as u16,
        "should be no-sub template"
    );
}

#[test]
fn template_with_substitution() {
    // `const x = \`hello ${name} world\``
    let (parser, init) = parse_clean_var_initializer(
        "const x = `hello ${name} world`;",
        "template with substitution",
    );
    let arena = parser.get_arena();
    let node = arena.get(init).expect("init");
    assert_eq!(
        node.kind,
        syntax_kind_ext::TEMPLATE_EXPRESSION,
        "should be template expression"
    );
}

#[test]
fn template_empty_span_at_eof_anchors_expression_before_missing_brace() {
    let source = "f `123qdawdrqw${ 1 }${ 2 }${ ";
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();

    let ts1109 = diagnostics
        .iter()
        .find(|diag| diag.code == diagnostic_codes::EXPRESSION_EXPECTED)
        .expect("expected TS1109 for empty template span expression");
    assert_eq!(
        ts1109.start,
        source.len() as u32 - 1,
        "TS1109 should anchor at trailing trivia before EOF: {diagnostics:?}"
    );

    let missing_brace = diagnostics
        .iter()
        .find(|diag| diag.code == diagnostic_codes::EXPECTED && diag.message == "'}' expected.")
        .expect("expected TS1005 for the missing template span close brace");
    assert_eq!(
        missing_brace.start,
        source.len() as u32,
        "TS1005 should anchor at EOF: {diagnostics:?}"
    );
}

/// Parser-supplied `LiteralData::raw_text` must carry the full template
/// token slice — including delimiters — so the emitter never re-scans the
/// source bytes to recover escape sequences. The contract holds for both
/// terminated and unterminated literals and for invalid escape sequences.
#[test]
fn no_substitution_template_records_raw_token_text() {
    let cases = [
        // Terminated, ordinary contents.
        ("`hello`;", "`hello`"),
        // Terminated, invalid `\u` escape — raw bytes preserved verbatim.
        ("`\\u`;", "`\\u`"),
        // Unterminated — raw text has no trailing backtick.
        ("`abc", "`abc"),
        // Unterminated with escaped backtick (`\``) — the backtick is content.
        ("`\\`", "`\\`"),
    ];
    for (source, expected_raw) in cases {
        let (parser, root) = parse_source(source);
        let arena = parser.get_arena();
        let init = get_first_expression_statement_expr(arena, root);
        let node = arena.get(init).expect("init");
        assert_eq!(
            node.kind,
            SyntaxKind::NoSubstitutionTemplateLiteral as u16,
            "source `{source}` should parse as a no-sub template",
        );
        let lit = arena.get_literal(node).expect("literal data");
        assert_eq!(
            lit.raw_text.as_deref(),
            Some(expected_raw),
            "raw_text for `{source}` should match the scanner token slice",
        );
    }
}
