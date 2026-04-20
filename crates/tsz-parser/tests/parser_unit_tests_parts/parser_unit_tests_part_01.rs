#[test]
fn error_recovery_missing_semicolon_asi() {
    // ASI should insert semicolons
    let (parser, root) = parse_source("const x = 1\nconst y = 2");
    assert_no_errors(&parser, "ASI");
    let arena = parser.get_arena();
    let stmts = get_statements(arena, root);
    assert_eq!(stmts.len(), 2, "should have 2 statements via ASI");
}
#[test]
fn error_recovery_missing_closing_brace() {
    // Missing closing brace should not panic
    let (parser, root) = parse_source("function f() { const x = 1;");
    assert_has_errors(&parser, "missing closing brace");
    let arena = parser.get_arena();
    let sf = arena.get_source_file_at(root).expect("source file");
    assert!(
        !sf.statements.nodes.is_empty(),
        "should still produce some statements"
    );
}
#[test]
fn error_recovery_unexpected_token() {
    // Unexpected token should not panic
    let (parser, _) = parse_source("const x = @@@;");
    assert_has_errors(&parser, "unexpected token");
}
#[test]
fn error_recovery_multiple_errors_continue_parsing() {
    // Multiple errors — parser should recover and continue
    let (parser, root) = parse_source("const x = ; const y = 2; const z = ;");
    let arena = parser.get_arena();
    let stmts = get_statements(arena, root);
    // Should at least parse `const y = 2` properly
    assert!(
        stmts.len() >= 2,
        "parser should recover and parse subsequent statements"
    );
}
#[test]
fn error_recovery_no_panic_on_empty_input() {
    // Empty input should not panic
    let (parser, root) = parse_source("");
    assert_no_errors(&parser, "empty input");
    let arena = parser.get_arena();
    let sf = arena.get_source_file_at(root).expect("source file");
    assert!(sf.statements.nodes.is_empty(), "should have no statements");
}
#[test]
fn error_recovery_no_panic_on_only_whitespace() {
    let (parser, root) = parse_source("   \n\n  \t  ");
    assert_no_errors(&parser, "whitespace only");
    let arena = parser.get_arena();
    let sf = arena.get_source_file_at(root).expect("source file");
    assert!(sf.statements.nodes.is_empty());
}
#[test]
fn error_recovery_deeply_nested_parens() {
    // Deeply nested parentheses should not overflow the stack
    let depth = 100;
    let mut source = String::new();
    source.push_str("const x = ");
    for _ in 0..depth {
        source.push('(');
    }
    source.push('1');
    for _ in 0..depth {
        source.push(')');
    }
    source.push(';');
    let (parser, _root) = parse_source(&source);
    // Should not panic; may or may not have errors depending on recursion limits
    let _ = parser.get_diagnostics();
}

// =============================================================================
// 8. Expression Miscellaneous Tests
// =============================================================================
#[test]
fn expr_new_expression() {
    // `new Foo(1, 2)`
    let (parser, root) = parse_source("const x = new Foo(1, 2);");
    assert_no_errors(&parser, "new expression");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(
        node.kind,
        syntax_kind_ext::NEW_EXPRESSION,
        "should be new expression"
    );
    let call = arena.get_call_expr(node).expect("call data");
    let args = call.arguments.as_ref().expect("args");
    assert_eq!(args.nodes.len(), 2, "should have 2 arguments");
}
#[test]
fn expr_new_without_parens() {
    // `new Foo` (without arguments)
    let (parser, root) = parse_source("const x = new Foo;");
    assert_no_errors(&parser, "new without parens");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(node.kind, syntax_kind_ext::NEW_EXPRESSION);
}
#[test]
fn expr_tagged_template() {
    // tag`hello ${x} world`
    let (parser, root) = parse_source("const x = tag`hello ${y} world`;");
    assert_no_errors(&parser, "tagged template");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(
        node.kind,
        syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION,
        "should be tagged template"
    );
}
#[test]
fn expr_spread_element() {
    // `[1, ...arr, 2]`
    let (parser, root) = parse_source("const x = [1, ...arr, 2];");
    assert_no_errors(&parser, "spread element");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(
        node.kind,
        syntax_kind_ext::ARRAY_LITERAL_EXPRESSION,
        "should be array literal"
    );
    let lit_expr = arena.get_literal_expr(node).expect("array lit");
    assert_eq!(lit_expr.elements.nodes.len(), 3, "should have 3 elements");
    let spread = arena.get(lit_expr.elements.nodes[1]).expect("spread");
    assert_eq!(
        spread.kind,
        syntax_kind_ext::SPREAD_ELEMENT,
        "middle should be spread"
    );
}
#[test]
fn expr_object_literal() {
    // `{ a: 1, b, ...c, [d]: 2 }`
    let (parser, root) = parse_source("const x = { a: 1, b, ...c, [d]: 2 };");
    assert_no_errors(&parser, "object literal");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(node.kind, syntax_kind_ext::OBJECT_LITERAL_EXPRESSION);
    let lit_expr = arena.get_literal_expr(node).expect("object lit");
    assert_eq!(
        lit_expr.elements.nodes.len(),
        4,
        "should have 4 properties/spread"
    );
    // Verify different property types
    let prop_a = arena.get(lit_expr.elements.nodes[0]).expect("a");
    assert_eq!(prop_a.kind, syntax_kind_ext::PROPERTY_ASSIGNMENT);
    let prop_b = arena.get(lit_expr.elements.nodes[1]).expect("b");
    assert_eq!(prop_b.kind, syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT);
    let prop_c = arena.get(lit_expr.elements.nodes[2]).expect("c");
    assert_eq!(prop_c.kind, syntax_kind_ext::SPREAD_ASSIGNMENT);
    let prop_d = arena.get(lit_expr.elements.nodes[3]).expect("d");
    assert_eq!(prop_d.kind, syntax_kind_ext::PROPERTY_ASSIGNMENT);
}
#[test]
fn expr_yield() {
    // `function* gen() { yield 1; yield* other(); }`
    let (parser, _) = parse_source("function* gen() { yield 1; yield* other(); }");
    assert_no_errors(&parser, "yield expression");
}
#[test]
fn expr_void_typeof_delete() {
    // `void 0; typeof x; delete obj.x;`
    // These are parsed as PREFIX_UNARY_EXPRESSION with the keyword as operator
    let (parser, root) = parse_source("void 0; typeof x; delete obj.x;");
    assert_no_errors(&parser, "void/typeof/delete");
    let arena = parser.get_arena();
    let stmts = get_statements(arena, root);
    assert_eq!(stmts.len(), 3);

    let s0 = arena.get(stmts[0]).expect("s0");
    let e0 = arena.get_expression_statement(s0).expect("es0");
    let void_node = arena.get(e0.expression).expect("void");
    assert_eq!(
        void_node.kind,
        syntax_kind_ext::PREFIX_UNARY_EXPRESSION,
        "void should be prefix unary"
    );
    let void_unary = arena.get_unary_expr(void_node).expect("void unary");
    assert_eq!(void_unary.operator, SyntaxKind::VoidKeyword as u16);

    let s1 = arena.get(stmts[1]).expect("s1");
    let e1 = arena.get_expression_statement(s1).expect("es1");
    let typeof_node = arena.get(e1.expression).expect("typeof");
    assert_eq!(
        typeof_node.kind,
        syntax_kind_ext::PREFIX_UNARY_EXPRESSION,
        "typeof should be prefix unary"
    );
    let typeof_unary = arena.get_unary_expr(typeof_node).expect("typeof unary");
    assert_eq!(typeof_unary.operator, SyntaxKind::TypeOfKeyword as u16);

    let s2 = arena.get(stmts[2]).expect("s2");
    let e2 = arena.get_expression_statement(s2).expect("es2");
    let delete_node = arena.get(e2.expression).expect("delete");
    assert_eq!(
        delete_node.kind,
        syntax_kind_ext::PREFIX_UNARY_EXPRESSION,
        "delete should be prefix unary"
    );
    let delete_unary = arena.get_unary_expr(delete_node).expect("delete unary");
    assert_eq!(delete_unary.operator, SyntaxKind::DeleteKeyword as u16);
}
#[test]
fn expr_prefix_postfix_unary() {
    // `++x; x--;`
    let (parser, root) = parse_source("++x; x--;");
    assert_no_errors(&parser, "prefix/postfix unary");
    let arena = parser.get_arena();
    let stmts = get_statements(arena, root);
    let s0 = arena.get(stmts[0]).expect("s0");
    let e0 = arena.get_expression_statement(s0).expect("es0");
    let pre = arena.get(e0.expression).expect("prefix");
    assert_eq!(pre.kind, syntax_kind_ext::PREFIX_UNARY_EXPRESSION);

    let s1 = arena.get(stmts[1]).expect("s1");
    let e1 = arena.get_expression_statement(s1).expect("es1");
    let post = arena.get(e1.expression).expect("postfix");
    assert_eq!(post.kind, syntax_kind_ext::POSTFIX_UNARY_EXPRESSION);
}
#[test]
fn expr_prefix_update_delete_recovery_drops_outer_update() {
    let source = "++ delete foo.bar;";
    let (parser, root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    let ts1109 = diagnostics
        .iter()
        .find(|diag| diag.code == diagnostic_codes::EXPRESSION_EXPECTED)
        .expect("expected TS1109 for invalid prefix update operand");
    assert_eq!(
        ts1109.start,
        source.find("delete").expect("delete") as u32,
        "TS1109 should anchor at `delete`: {diagnostics:?}"
    );

    let arena = parser.get_arena();
    let stmt = arena.get(get_first_statement(arena, root)).expect("stmt");
    let expr_stmt = arena
        .get_expression_statement(stmt)
        .expect("expression statement");
    let expr = arena.get(expr_stmt.expression).expect("expression");
    assert_eq!(
        expr.kind,
        syntax_kind_ext::PREFIX_UNARY_EXPRESSION,
        "recovered expression should still be unary"
    );
    let unary = arena.get_unary_expr(expr).expect("unary expression");
    assert_eq!(
        unary.operator,
        SyntaxKind::DeleteKeyword as u16,
        "outer prefix update should be dropped during recovery"
    );
}
#[test]
fn expr_prefix_update_repeated_operator_recovers_to_inner_update() {
    let source = "++\n++y;";
    let (parser, root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    let ts1109 = diagnostics
        .iter()
        .find(|diag| diag.code == diagnostic_codes::EXPRESSION_EXPECTED)
        .expect("expected TS1109 for repeated prefix update");
    assert_eq!(
        ts1109.start,
        source.find("\n++").expect("inner update") as u32 + 1,
        "TS1109 should anchor at the inner `++`: {diagnostics:?}"
    );

    let arena = parser.get_arena();
    let stmt = arena.get(get_first_statement(arena, root)).expect("stmt");
    let expr_stmt = arena
        .get_expression_statement(stmt)
        .expect("expression statement");
    let expr = arena.get(expr_stmt.expression).expect("expression");
    assert_eq!(
        expr.kind,
        syntax_kind_ext::PREFIX_UNARY_EXPRESSION,
        "recovered expression should keep the inner prefix update"
    );
    let unary = arena.get_unary_expr(expr).expect("unary expression");
    assert_eq!(unary.operator, SyntaxKind::PlusPlusToken as u16);
    let operand = arena.get(unary.operand).expect("inner operand");
    assert_eq!(
        operand.kind,
        SyntaxKind::Identifier as u16,
        "inner update should still target the identifier"
    );
}
#[test]
fn expr_prefix_update_repeated_operator_same_line_anchors_inner_update() {
    let source = "++++y;";
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    let ts1109 = diagnostics
        .iter()
        .find(|diag| diag.code == diagnostic_codes::EXPRESSION_EXPECTED)
        .expect("expected TS1109 for repeated prefix update");
    assert_eq!(
        ts1109.start, 2,
        "TS1109 should anchor at the second `++`: {diagnostics:?}"
    );
}
#[test]
fn object_spread_invalid_asterisk_recovers_to_operand_expression() {
    let source = "let o8 = { ...*o };";
    let (parser, root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    let ts1109 = diagnostics
        .iter()
        .find(|diag| diag.code == diagnostic_codes::EXPRESSION_EXPECTED)
        .expect("expected TS1109 for invalid spread operand");
    assert_eq!(
        ts1109.start,
        source.find('*').expect("asterisk") as u32,
        "TS1109 should anchor at the stray `*`: {diagnostics:?}"
    );

    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let object = arena.get(init).expect("object literal");
    let object_data = arena
        .get_literal_expr(object)
        .expect("object literal expression");
    assert_eq!(
        object_data.elements.nodes.len(),
        1,
        "expected one spread property"
    );

    let spread = arena
        .get(object_data.elements.nodes[0])
        .expect("spread assignment");
    assert_eq!(spread.kind, syntax_kind_ext::SPREAD_ASSIGNMENT);
    let spread_data = arena.get_spread(spread).expect("spread data");
    let operand = arena.get(spread_data.expression).expect("spread operand");
    assert_eq!(
        operand.kind,
        SyntaxKind::Identifier as u16,
        "recovery should keep the identifier operand after skipping the stray `*`"
    );
    assert_eq!(node_text(arena, source, spread_data.expression), "o");
}
#[test]
fn expr_prefix_update_repeated_operator_after_line_break_matches_sputnik_anchor() {
    let source = "var x=0, y=0;\nvar z=\nx\n++\n++\ny\n";
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    let ts1109 = diagnostics
        .iter()
        .find(|diag| diag.code == diagnostic_codes::EXPRESSION_EXPECTED)
        .expect("expected TS1109 for repeated prefix update after line break");
    assert_eq!(
        ts1109.start,
        source.rfind("\n++\n").expect("second update line") as u32 + 1,
        "TS1109 should anchor at the second `++`: {diagnostics:?}"
    );
}
#[test]
fn expr_element_access() {
    // `a[0]`
    let (parser, root) = parse_source("const x = a[0];");
    assert_no_errors(&parser, "element access");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(
        node.kind,
        syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION,
        "should be element access"
    );
}
#[test]
fn expr_optional_element_access() {
    // `a?.[0]`
    let (parser, root) = parse_source("const x = a?.[0];");
    assert_no_errors(&parser, "optional element access");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(node.kind, syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION);
    let access = arena.get_access_expr(node).expect("access");
    assert!(access.question_dot_token, "should have ?.");
}
#[test]
fn expr_optional_call() {
    // `a?.(1)`
    let (parser, root) = parse_source("const x = a?.(1);");
    assert_no_errors(&parser, "optional call");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(node.kind, syntax_kind_ext::CALL_EXPRESSION);
}

// =============================================================================
// 9. Destructuring Tests
// =============================================================================
#[test]
fn destructuring_object() {
    // `const { a, b: c, ...rest } = obj;`
    let (parser, root) = parse_source("const { a, b: c, ...rest } = obj;");
    assert_no_errors(&parser, "object destructuring");
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
        syntax_kind_ext::OBJECT_BINDING_PATTERN,
        "should be object binding"
    );
}
#[test]
fn destructuring_array() {
    // `const [a, , b, ...rest] = arr;`
    let (parser, root) = parse_source("const [a, , b, ...rest] = arr;");
    assert_no_errors(&parser, "array destructuring");
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
fn destructuring_nested() {
    // `const { a: { b } } = obj;`
    let (parser, _) = parse_source("const { a: { b } } = obj;");
    assert_no_errors(&parser, "nested destructuring");
}
#[test]
fn destructuring_with_defaults() {
    // `const { a = 1, b = 2 } = obj;`
    let (parser, _) = parse_source("const { a = 1, b = 2 } = obj;");
    assert_no_errors(&parser, "destructuring with defaults");
}

// =============================================================================
// 10. Additional Type Tests
// =============================================================================
#[test]
fn type_import() {
    // `type T = import('module').Foo`
    let (parser, root) = parse_source("type T = import('module').Foo;");
    assert_no_errors(&parser, "import type");
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
    let (parser, root) = parse_source(source);
    assert_no_errors(&parser, "qualified type reference span");

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
    let (parser, root) = parse_source(source);
    assert_no_errors(&parser, "type query span");

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
    let (parser, root) = parse_source(source);
    assert_no_errors(&parser, "import type span");

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
    let (parser, root) = parse_source(source);
    assert_no_errors(&parser, "unique symbol");

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
    let (parser, root) = parse_source("type T = { readonly [K in keyof T]-?: T[K] };");
    assert_no_errors(&parser, "mapped type with modifiers");
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
    let (parser, root) = parse_source("type T = { x: string; y: number };");
    assert_no_errors(&parser, "type literal");
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
    let (parser, root) = parse_source("type T = A & B | C & D;");
    assert_no_errors(&parser, "union/intersection precedence");
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

// =============================================================================
// 11. Template Literal Tests
// =============================================================================
#[test]
fn template_no_substitution() {
    // `const x = \`hello\``
    let (parser, root) = parse_source("const x = `hello`;");
    assert_no_errors(&parser, "no-sub template");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
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
    let (parser, root) = parse_source("const x = `hello ${name} world`;");
    assert_no_errors(&parser, "template with substitution");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(
        node.kind,
        syntax_kind_ext::TEMPLATE_EXPRESSION,
        "should be template expression"
    );
}

// =============================================================================
// 12. Using / Await Using Declarations
// =============================================================================
#[test]
fn decl_using() {
    // `using x = getResource();`
    let (parser, root) = parse_source("using x = getResource();");
    assert_no_errors(&parser, "using declaration");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(
        stmt_node.kind,
        syntax_kind_ext::VARIABLE_STATEMENT,
        "should be variable statement"
    );
}
#[test]
fn decl_await_using() {
    // `await using x = getResource();`
    let (parser, _root) = parse_source("async function f() { await using x = getResource(); }");
    assert_no_errors(&parser, "await using declaration");
}

// =============================================================================
// 13. Edge Cases for Specific AST Verification
// =============================================================================
#[test]
fn class_expression() {
    // `const C = class extends Base { constructor() { super(); } };`
    let (parser, root) =
        parse_source("const C = class extends Base { constructor() { super(); } };");
    assert_no_errors(&parser, "class expression");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(
        node.kind,
        syntax_kind_ext::CLASS_EXPRESSION,
        "should be class expression"
    );
}
#[test]
fn function_expression() {
    // `const f = function foo(x: number) { return x; };`
    let (parser, root) = parse_source("const f = function foo(x: number) { return x; };");
    assert_no_errors(&parser, "function expression");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(
        node.kind,
        syntax_kind_ext::FUNCTION_EXPRESSION,
        "should be function expression"
    );
    let func = arena.get_function(node).expect("func data");
    assert!(func.name.is_some(), "should have name 'foo'");
}
#[test]
fn generator_function() {
    // `function* gen() { yield 1; }`
    let (parser, root) = parse_source("function* gen() { yield 1; }");
    assert_no_errors(&parser, "generator function");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let func = arena.get_function(stmt_node).expect("function");
    assert!(func.asterisk_token, "should have asterisk (generator)");
}
#[test]
fn async_generator_function() {
    // `async function* gen() { yield 1; }`
    let (parser, root) = parse_source("async function* gen() { yield 1; }");
    assert_no_errors(&parser, "async generator");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let func = arena.get_function(stmt_node).expect("function");
    assert!(func.is_async, "should be async");
    assert!(func.asterisk_token, "should be generator");
}
#[test]
fn multiple_variable_declarations() {
    // `const a = 1, b = 2, c = 3;`
    let (parser, root) = parse_source("const a = 1, b = 2, c = 3;");
    assert_no_errors(&parser, "multiple variable declarations");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let var = arena.get_variable(stmt_node).expect("var");
    // var.declarations contains the VARIABLE_DECLARATION_LIST
    let decl_list_node = arena.get(var.declarations.nodes[0]).expect("decl list");
    let decl_list = arena.get_variable(decl_list_node).expect("decl list data");
    assert_eq!(
        decl_list.declarations.nodes.len(),
        3,
        "should have 3 declarations"
    );
}
#[test]
fn interface_call_and_construct_signatures() {
    // `interface I { (): void; new (): I; }`
    let (parser, root) = parse_source("interface I { (): void; new (): I; }");
    assert_no_errors(&parser, "call and construct signatures");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let iface = arena.get_interface(stmt_node).expect("interface");
    assert_eq!(iface.members.nodes.len(), 2, "should have 2 members");
    let m0 = arena.get(iface.members.nodes[0]).expect("m0");
    assert_eq!(
        m0.kind,
        syntax_kind_ext::CALL_SIGNATURE,
        "first should be call signature"
    );
    let m1 = arena.get(iface.members.nodes[1]).expect("m1");
    assert_eq!(
        m1.kind,
        syntax_kind_ext::CONSTRUCT_SIGNATURE,
        "second should be construct signature"
    );
}
#[test]
fn type_predicate_in_function() {
    // `function isString(x: any): x is string { return typeof x === "string"; }`
    let (parser, root) =
        parse_source("function isString(x: any): x is string { return typeof x === 'string'; }");
    assert_no_errors(&parser, "type predicate");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let func = arena.get_function(stmt_node).expect("function");
    let ret_type = arena.get(func.type_annotation).expect("return type");
    assert_eq!(
        ret_type.kind,
        syntax_kind_ext::TYPE_PREDICATE,
        "should be type predicate"
    );
}
#[test]
fn import_with_attributes() {
    // `import data from './data.json' with { type: 'json' };`
    let (parser, root) = parse_source("import data from './data.json' with { type: 'json' };");
    assert_no_errors(&parser, "import with attributes");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let import = arena.get_import_decl(stmt_node).expect("import");
    assert!(import.attributes.is_some(), "should have import attributes");
}
#[test]
fn export_star_from() {
    // `export * from './module';`
    let (parser, root) = parse_source("export * from './module';");
    assert_no_errors(&parser, "export star from");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(stmt_node.kind, syntax_kind_ext::EXPORT_DECLARATION);
    let export = arena.get_export_decl(stmt_node).expect("export decl");
    assert!(
        export.module_specifier.is_some(),
        "should have module specifier"
    );
}
#[test]
fn export_star_as_namespace() {
    // `export * as ns from './module';`
    let (parser, root) = parse_source("export * as ns from './module';");
    assert_no_errors(&parser, "export * as ns");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(stmt_node.kind, syntax_kind_ext::EXPORT_DECLARATION);
}
