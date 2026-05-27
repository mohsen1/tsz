#[test]
fn precedence_multiplication_binds_tighter_than_addition() {
    // `1 + 2 * 3` should parse as `1 + (2 * 3)`
    let (parser, root) = parse_source("const x = 1 + 2 * 3;");
    assert_no_errors(&parser, "1 + 2 * 3");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let (left, op, right) = get_binary(arena, init);
    assert_eq!(op, SyntaxKind::PlusToken as u16, "top should be +");
    // left should be numeric literal 1
    let left_node = arena.get(left).expect("left node");
    assert_eq!(left_node.kind, SyntaxKind::NumericLiteral as u16);
    // right should be binary: 2 * 3
    let right_node = arena.get(right).expect("right node");
    assert_eq!(right_node.kind, syntax_kind_ext::BINARY_EXPRESSION);
    let (_, inner_op, _) = get_binary(arena, right);
    assert_eq!(
        inner_op,
        SyntaxKind::AsteriskToken as u16,
        "inner should be *"
    );
}

#[test]
fn precedence_nullish_coalescing_vs_logical_or() {
    // `a ?? b || c` — ?? and || mixing. The parser may or may not error here
    // (tsc treats it as a parse error). We verify it produces a valid AST.
    let (parser, root) = parse_source("const x = a ?? b || c;");
    let arena = parser.get_arena();
    let sf = arena.get_source_file_at(root).expect("source file");
    // Should produce at least one statement regardless of errors
    assert!(!sf.statements.nodes.is_empty(), "should parse something");
}

#[test]
fn precedence_logical_and_vs_logical_or() {
    // `a || b && c` should parse as `a || (b && c)` since && binds tighter
    let (parser, root) = parse_source("const x = a || b && c;");
    assert_no_errors(&parser, "a || b && c");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let (_, op, right) = get_binary(arena, init);
    assert_eq!(op, SyntaxKind::BarBarToken as u16, "top should be ||");
    let right_node = arena.get(right).expect("right node");
    assert_eq!(
        right_node.kind,
        syntax_kind_ext::BINARY_EXPRESSION,
        "RHS should be binary"
    );
    let (_, inner_op, _) = get_binary(arena, right);
    assert_eq!(
        inner_op,
        SyntaxKind::AmpersandAmpersandToken as u16,
        "inner should be &&"
    );
}

#[test]
fn precedence_ternary_nesting_right_associative() {
    // `a ? b : c ? d : e` should parse as `a ? b : (c ? d : e)`
    let (parser, root) = parse_source("const x = a ? b : c ? d : e;");
    assert_no_errors(&parser, "ternary nesting");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init node");
    assert_eq!(
        node.kind,
        syntax_kind_ext::CONDITIONAL_EXPRESSION,
        "top should be conditional"
    );
    let cond = arena.get_conditional_expr(node).expect("cond data");
    // when_false should itself be a conditional expression
    let false_node = arena.get(cond.when_false).expect("false branch");
    assert_eq!(
        false_node.kind,
        syntax_kind_ext::CONDITIONAL_EXPRESSION,
        "false branch should be nested conditional"
    );
}

#[test]
fn await_call_in_non_async_function_parses_as_identifier_call() {
    let (parser, root) = parse_source("function f() { const x = await(Promise.resolve(1)); }");
    assert_no_errors(&parser, "await call identifier parse");

    let arena = parser.get_arena();
    let init = get_first_function_var_initializer(arena, root);
    let call_node = arena.get(init).expect("initializer");

    assert_eq!(call_node.kind, syntax_kind_ext::CALL_EXPRESSION);
    let call = arena.get_call_expr(call_node).expect("call data");
    let callee = arena.get(call.expression).expect("callee");
    assert_eq!(callee.kind, SyntaxKind::Identifier as u16);
}

#[test]
fn await_operand_in_non_async_function_stays_await_expression() {
    let (parser, root) = parse_source("function f() { const x = await Promise.resolve(1); }");
    assert_no_errors(&parser, "await operand expression parse");

    let arena = parser.get_arena();
    let init = get_first_function_var_initializer(arena, root);
    let init_node = arena.get(init).expect("initializer");
    assert_eq!(init_node.kind, syntax_kind_ext::AWAIT_EXPRESSION);
}

#[test]
fn precedence_comma_operator_vs_argument_separator() {
    // In `f(a, b)`, comma separates arguments, not comma operator
    let (parser, root) = parse_source("f(a, b);");
    assert_no_errors(&parser, "f(a, b)");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt node");
    let expr_stmt = arena.get_expression_statement(stmt_node).expect("expr");
    let call_node = arena.get(expr_stmt.expression).expect("call node");
    let call = arena.get_call_expr(call_node).expect("call data");
    let args = call.arguments.as_ref().expect("arguments");
    assert_eq!(args.nodes.len(), 2, "should have 2 arguments, not comma op");
}

#[test]
fn precedence_comma_operator_in_expression() {
    // `const x = (1, 2, 3)` — the comma operator inside parens
    let (parser, root) = parse_source("const x = (1, 2, 3);");
    assert_no_errors(&parser, "comma operator");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    // Should be parenthesized
    let paren_node = arena.get(init).expect("node");
    assert_eq!(paren_node.kind, syntax_kind_ext::PARENTHESIZED_EXPRESSION);
    let paren = arena.get_parenthesized(paren_node).expect("paren data");
    // Inner should be comma binary expression
    let inner = arena.get(paren.expression).expect("inner");
    assert_eq!(inner.kind, syntax_kind_ext::BINARY_EXPRESSION);
    let (_, op, _) = get_binary(arena, paren.expression);
    assert_eq!(op, SyntaxKind::CommaToken as u16, "should be comma");
}

#[test]
fn comma_expression_with_missing_rhs_preserves_binary_shape() {
    let source = "(ANY, );";
    let (parser, root) = parse_source(source);
    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
        "expected TS1109 for missing comma RHS, got {codes:?}"
    );

    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let expr_stmt = arena
        .get_expression_statement(stmt_node)
        .expect("expr stmt");
    let paren_node = arena.get(expr_stmt.expression).expect("paren node");
    assert_eq!(paren_node.kind, syntax_kind_ext::PARENTHESIZED_EXPRESSION);
    let paren = arena.get_parenthesized(paren_node).expect("paren data");
    let inner = arena.get(paren.expression).expect("inner");
    assert_eq!(inner.kind, syntax_kind_ext::BINARY_EXPRESSION);
    let (_, op, right) = get_binary(arena, paren.expression);
    assert_eq!(op, SyntaxKind::CommaToken as u16, "should keep comma op");
    let right_node = arena.get(right).expect("missing rhs node");
    assert_eq!(right_node.kind, SyntaxKind::Identifier as u16);
    let right_ident = arena
        .get_identifier(right_node)
        .expect("missing rhs identifier");
    assert!(
        right_ident.escaped_text.is_empty(),
        "missing comma RHS should be an empty recovery identifier"
    );
}

#[test]
fn precedence_assignment_right_associativity() {
    // `a = b = c` should parse as `a = (b = c)`
    let (parser, root) = parse_source("a = b = c;");
    assert_no_errors(&parser, "a = b = c");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let expr_stmt = arena
        .get_expression_statement(stmt_node)
        .expect("expr stmt");
    let (_, op, right) = get_binary(arena, expr_stmt.expression);
    assert_eq!(op, SyntaxKind::EqualsToken as u16, "top = assignment");
    let right_node = arena.get(right).expect("right");
    assert_eq!(
        right_node.kind,
        syntax_kind_ext::BINARY_EXPRESSION,
        "RHS should be nested assignment"
    );
    let (_, inner_op, _) = get_binary(arena, right);
    assert_eq!(
        inner_op,
        SyntaxKind::EqualsToken as u16,
        "inner = assignment"
    );
}

#[test]
fn precedence_exponentiation_right_associative() {
    // `2 ** 3 ** 4` should parse as `2 ** (3 ** 4)`
    let (parser, root) = parse_source("const x = 2 ** 3 ** 4;");
    assert_no_errors(&parser, "2 ** 3 ** 4");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let (left, op, right) = get_binary(arena, init);
    assert_eq!(
        op,
        SyntaxKind::AsteriskAsteriskToken as u16,
        "top should be **"
    );
    // left should be numeric literal 2
    let left_node = arena.get(left).expect("left");
    assert_eq!(left_node.kind, SyntaxKind::NumericLiteral as u16);
    // right should be binary: 3 ** 4
    let right_node = arena.get(right).expect("right");
    assert_eq!(right_node.kind, syntax_kind_ext::BINARY_EXPRESSION);
    let (_, inner_op, _) = get_binary(arena, right);
    assert_eq!(
        inner_op,
        SyntaxKind::AsteriskAsteriskToken as u16,
        "inner should be **"
    );
}

#[test]
fn precedence_optional_chaining_with_call() {
    // `a?.b()` should parse as call on optional property access
    let (parser, root) = parse_source("a?.b();");
    assert_no_errors(&parser, "a?.b()");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let expr_stmt = arena.get_expression_statement(stmt_node).expect("expr");
    let call_node = arena.get(expr_stmt.expression).expect("call node");
    assert_eq!(
        call_node.kind,
        syntax_kind_ext::CALL_EXPRESSION,
        "should be call expr"
    );
    let call = arena.get_call_expr(call_node).expect("call data");
    // The callee should be a property access with question dot
    let access_node = arena.get(call.expression).expect("access");
    assert_eq!(
        access_node.kind,
        syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
    );
    let access = arena.get_access_expr(access_node).expect("access data");
    assert!(access.question_dot_token, "should have ?. token");
}

#[test]
fn precedence_comparison_operators() {
    // `a < b === c > d` should parse as `(a < b) === (c > d)`
    let (parser, root) = parse_source("const x = a < b === c > d;");
    assert_no_errors(&parser, "comparison operators");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let (_, op, _) = get_binary(arena, init);
    assert_eq!(
        op,
        SyntaxKind::EqualsEqualsEqualsToken as u16,
        "top should be ==="
    );
}

#[test]
fn precedence_bitwise_and_vs_equality() {
    // `a === b & c`: & has lower precedence than ===, so it's `(a === b) & c`
    let (parser, root) = parse_source("const x = a === b & c;");
    assert_no_errors(&parser, "=== vs &");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let (_, op, _) = get_binary(arena, init);
    assert_eq!(
        op,
        SyntaxKind::AmpersandToken as u16,
        "top should be & (lower precedence)"
    );
}

#[test]
fn precedence_as_expression() {
    // `a as T` produces an AsExpression
    let (parser, root) = parse_source("const x = a as number;");
    assert_no_errors(&parser, "as expression");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(
        node.kind,
        syntax_kind_ext::AS_EXPRESSION,
        "should be as expression"
    );
}

#[test]
fn precedence_satisfies_expression() {
    // `a satisfies T` produces a SatisfiesExpression
    let (parser, root) = parse_source("const x = a satisfies number;");
    assert_no_errors(&parser, "satisfies expression");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(
        node.kind,
        syntax_kind_ext::SATISFIES_EXPRESSION,
        "should be satisfies expression"
    );
}

#[test]
fn precedence_as_const_after_satisfies_wraps_satisfies_expression() {
    for source in [
        "const x = { a: 1 } satisfies Record<string, number> as const;",
        "const x = value satisfies Foo | Bar as const;",
        "const x = ((value) satisfies Alias) as const;",
    ] {
        let (parser, root) = parse_source(source);
        assert_no_errors(&parser, source);

        let arena = parser.get_arena();
        let init = get_var_initializer(arena, root);
        let outer = arena.get(init).expect("initializer");
        assert_eq!(
            outer.kind,
            syntax_kind_ext::AS_EXPRESSION,
            "`as const` should wrap the satisfies expression for {source}"
        );

        let outer_assertion = arena
            .get_type_assertion(outer)
            .expect("outer as expression data");
        let outer_type = arena
            .get(outer_assertion.type_node)
            .expect("outer as expression type");
        assert_eq!(
            outer_type.kind,
            SyntaxKind::ConstKeyword as u16,
            "`as const` should keep `const` as the outer assertion type for {source}"
        );

        let mut inner_idx = outer_assertion.expression;
        loop {
            let inner = arena.get(inner_idx).expect("inner expression");
            if inner.kind != syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                break;
            }
            inner_idx = arena
                .get_parenthesized(inner)
                .expect("parenthesized expression data")
                .expression;
        }

        let inner = arena.get(inner_idx).expect("unwrapped inner expression");
        assert_eq!(
            inner.kind,
            syntax_kind_ext::SATISFIES_EXPRESSION,
            "outer `as const` should contain a satisfies expression for {source}"
        );
    }
}

#[test]
fn precedence_non_null_assertion() {
    // `a!` produces a NonNullExpression
    let (parser, root) = parse_source("const x = a!;");
    assert_no_errors(&parser, "non-null assertion");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(
        node.kind,
        syntax_kind_ext::NON_NULL_EXPRESSION,
        "should be non-null expr"
    );
}

#[test]
fn precedence_type_assertion_angle_bracket() {
    // `<number>a` produces a TypeAssertion
    let (parser, root) = parse_source("const x = <number>a;");
    assert_no_errors(&parser, "type assertion");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(
        node.kind,
        syntax_kind_ext::TYPE_ASSERTION,
        "should be type assertion"
    );
}

/// Regression: `<number> yield 0` inside a generator must NOT consume `yield`
/// into the type assertion's expression. tsc's `parseSimpleUnaryExpression`
/// does not handle `YieldKeyword`, so the type assertion ends with a missing
/// expression, TS1109 is reported at `yield`, and `yield 0` becomes a
/// separate yield expression statement.
///
/// Conformance test `castOfYield` (`tests/cases/compiler/castOfYield.ts`)
/// pinned this shape: tsc emits `;` (empty stmt) followed by `yield 0;`.
#[test]
fn type_assertion_does_not_consume_yield_in_generator() {
    let (parser, root) = parse_source("function* f() { <number> (yield 0); <number> yield 0; }");
    let arena = parser.get_arena();

    // Locate the generator function body.
    let func_idx = get_first_statement(arena, root);
    let func_node = arena.get(func_idx).expect("function decl");
    let func = arena
        .get_function(func_node)
        .expect("function declaration data");
    let body_node = arena.get(func.body).expect("function body");
    let block = arena.get_block(body_node).expect("block data");
    let body_stmts = &block.statements.nodes;

    // After the fix, the body has 3 statements:
    //   1) ExpressionStatement: `<number> (yield 0)` — well-formed type assertion
    //   2) ExpressionStatement: `<number>` with a missing expression (recovery)
    //   3) ExpressionStatement: `yield 0` as a separate yield expression
    assert_eq!(
        body_stmts.len(),
        3,
        "expected 3 statements, got {}: {:?}",
        body_stmts.len(),
        body_stmts
            .iter()
            .map(|&i| arena.get(i).map(|n| n.kind))
            .collect::<Vec<_>>()
    );

    // First statement: type assertion wrapping parenthesized yield (valid).
    let s1_node = arena.get(body_stmts[0]).expect("stmt 1");
    let s1_expr_stmt = arena.get_expression_statement(s1_node).expect("expr stmt");
    let s1_inner = arena
        .get(s1_expr_stmt.expression)
        .expect("type assertion node");
    assert_eq!(
        s1_inner.kind,
        syntax_kind_ext::TYPE_ASSERTION,
        "first stmt should be a TypeAssertion"
    );

    // Second statement: type assertion with NO inner expression (recovery).
    let s2_node = arena.get(body_stmts[1]).expect("stmt 2");
    let s2_expr_stmt = arena.get_expression_statement(s2_node).expect("expr stmt");
    let s2_inner = arena
        .get(s2_expr_stmt.expression)
        .expect("type assertion node");
    assert_eq!(
        s2_inner.kind,
        syntax_kind_ext::TYPE_ASSERTION,
        "second stmt should be a TypeAssertion"
    );
    let s2_assert = arena
        .get_type_assertion(s2_inner)
        .expect("type assertion data");
    assert!(
        s2_assert.expression.is_none(),
        "type assertion before bare `yield` must have no inner expression"
    );

    // Third statement: yield expression statement.
    let s3_node = arena.get(body_stmts[2]).expect("stmt 3");
    let s3_expr_stmt = arena.get_expression_statement(s3_node).expect("expr stmt");
    let s3_inner = arena.get(s3_expr_stmt.expression).expect("yield expr node");
    assert_eq!(
        s3_inner.kind,
        syntax_kind_ext::YIELD_EXPRESSION,
        "third stmt should be a YieldExpression"
    );

    // The recovery must report exactly one TS1109 (Expression expected) at `yield`,
    // not two diagnostics at the same site.
    let ts1109_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == diagnostic_codes::EXPRESSION_EXPECTED)
        .count();
    assert_eq!(
        ts1109_count, 1,
        "expected exactly one TS1109 for `<number> yield 0`, got {ts1109_count}"
    );
}

#[test]
fn precedence_instanceof_and_in() {
    // `a instanceof B` and `a in b` should parse without errors
    let (parser, _) = parse_source("const x = a instanceof B; const y = 'a' in b;");
    assert_no_errors(&parser, "instanceof and in");
}

#[test]
fn precedence_ternary_with_assignment() {
    // `a ? b = 1 : c = 2` should parse correctly
    let (parser, root) = parse_source("a ? b = 1 : c = 2;");
    assert_no_errors(&parser, "ternary with assignment");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let expr_stmt = arena.get_expression_statement(stmt_node).expect("expr");
    let cond_node = arena.get(expr_stmt.expression).expect("cond");
    assert_eq!(
        cond_node.kind,
        syntax_kind_ext::CONDITIONAL_EXPRESSION,
        "should be conditional"
    );
}

#[test]
fn precedence_addition_left_associative() {
    // `1 + 2 + 3` should parse as `(1 + 2) + 3`
    let (parser, root) = parse_source("const x = 1 + 2 + 3;");
    assert_no_errors(&parser, "1 + 2 + 3");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let (left, op, right) = get_binary(arena, init);
    assert_eq!(op, SyntaxKind::PlusToken as u16, "top should be +");
    // right should be a numeric literal (3), left should be binary
    let right_node = arena.get(right).expect("right");
    assert_eq!(
        right_node.kind,
        SyntaxKind::NumericLiteral as u16,
        "RHS should be literal"
    );
    let left_node = arena.get(left).expect("left");
    assert_eq!(
        left_node.kind,
        syntax_kind_ext::BINARY_EXPRESSION,
        "LHS should be binary (left-assoc)"
    );
}

// =============================================================================
// 2. Arrow Function Edge Cases (10+ tests)
// =============================================================================

#[test]
fn arrow_single_param_no_parens() {
    // `x => x`
    let (parser, root) = parse_source("const f = x => x;");
    assert_no_errors(&parser, "x => x");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(
        node.kind,
        syntax_kind_ext::ARROW_FUNCTION,
        "should be arrow"
    );
    let func = arena.get_function(node).expect("function data");
    assert_eq!(func.parameters.nodes.len(), 1, "should have 1 param");
    assert!(!func.is_async, "should not be async");
}

#[test]
fn arrow_multi_param() {
    // `(x, y) => x + y`
    let (parser, root) = parse_source("const f = (x, y) => x + y;");
    assert_no_errors(&parser, "(x, y) => x + y");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(node.kind, syntax_kind_ext::ARROW_FUNCTION);
    let func = arena.get_function(node).expect("function data");
    assert_eq!(func.parameters.nodes.len(), 2, "should have 2 params");
}

#[test]
fn arrow_no_params() {
    // `() => 42`
    let (parser, root) = parse_source("const f = () => 42;");
    assert_no_errors(&parser, "() => 42");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(node.kind, syntax_kind_ext::ARROW_FUNCTION);
    let func = arena.get_function(node).expect("function data");
    assert_eq!(func.parameters.nodes.len(), 0, "should have 0 params");
}

#[test]
fn arrow_object_literal_body_needs_parens() {
    // `() => ({})` — object literal body must be parenthesized
    let (parser, root) = parse_source("const f = () => ({});");
    assert_no_errors(&parser, "() => ({})");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(node.kind, syntax_kind_ext::ARROW_FUNCTION);
    let func = arena.get_function(node).expect("function data");
    // body should be a parenthesized expression
    let body = arena.get(func.body).expect("body");
    assert_eq!(
        body.kind,
        syntax_kind_ext::PARENTHESIZED_EXPRESSION,
        "body should be parenthesized"
    );
}

#[test]
fn arrow_async() {
    // `async x => await x`
    let (parser, root) = parse_source("const f = async x => await x;");
    assert_no_errors(&parser, "async arrow");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(node.kind, syntax_kind_ext::ARROW_FUNCTION);
    let func = arena.get_function(node).expect("function data");
    assert!(func.is_async, "should be async");
}

#[test]
fn arrow_async_multi_param() {
    // `async (a, b) => a + b`
    let (parser, root) = parse_source("const f = async (a, b) => a + b;");
    assert_no_errors(&parser, "async multi param arrow");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(node.kind, syntax_kind_ext::ARROW_FUNCTION);
    let func = arena.get_function(node).expect("function data");
    assert!(func.is_async, "should be async");
    assert_eq!(func.parameters.nodes.len(), 2);
}

#[test]
fn arrow_with_block_body() {
    // `(x) => { return x; }`
    let (parser, root) = parse_source("const f = (x) => { return x; };");
    assert_no_errors(&parser, "arrow with block body");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(node.kind, syntax_kind_ext::ARROW_FUNCTION);
    let func = arena.get_function(node).expect("function data");
    let body = arena.get(func.body).expect("body");
    assert_eq!(body.kind, syntax_kind_ext::BLOCK, "body should be block");
}

#[test]
fn arrow_with_type_annotation() {
    // `(x: number): string => x.toString()`
    let (parser, root) = parse_source("const f = (x: number): string => x.toString();");
    assert_no_errors(&parser, "arrow with type annotation");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(node.kind, syntax_kind_ext::ARROW_FUNCTION);
    let func = arena.get_function(node).expect("function data");
    assert!(func.type_annotation.is_some(), "should have return type");
}

#[test]
fn arrow_in_ternary() {
    // `cond ? x => x : y => y` — arrows in ternary branches
    let (parser, root) = parse_source("const f = cond ? x => x : y => y;");
    assert_no_errors(&parser, "arrow in ternary");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(
        node.kind,
        syntax_kind_ext::CONDITIONAL_EXPRESSION,
        "top should be conditional"
    );
    let cond = arena.get_conditional_expr(node).expect("cond data");
    let true_branch = arena.get(cond.when_true).expect("true");
    assert_eq!(
        true_branch.kind,
        syntax_kind_ext::ARROW_FUNCTION,
        "true branch should be arrow"
    );
    let false_branch = arena.get(cond.when_false).expect("false");
    assert_eq!(
        false_branch.kind,
        syntax_kind_ext::ARROW_FUNCTION,
        "false branch should be arrow"
    );
}

#[test]
fn arrow_generic_in_ts_file() {
    // `<T>(x: T) => x` — generic arrow in .ts file (not TSX)
    let (parser, root) = parse_source("const f = <T>(x: T) => x;");
    assert_no_errors(&parser, "generic arrow .ts");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(
        node.kind,
        syntax_kind_ext::ARROW_FUNCTION,
        "should be arrow function"
    );
    let func = arena.get_function(node).expect("function data");
    assert!(
        func.type_parameters.is_some(),
        "should have type parameters"
    );
}

#[test]
fn arrow_generic_with_constraint() {
    // `<T extends string>(x: T) => x`
    let (parser, root) = parse_source("const f = <T extends string>(x: T) => x;");
    assert_no_errors(&parser, "generic arrow with constraint");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(node.kind, syntax_kind_ext::ARROW_FUNCTION);
}

#[test]
fn arrow_nested() {
    // `(a) => (b) => a + b` — curried arrow
    let (parser, root) = parse_source("const f = (a) => (b) => a + b;");
    assert_no_errors(&parser, "nested arrow");
    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(node.kind, syntax_kind_ext::ARROW_FUNCTION);
    let func = arena.get_function(node).expect("function data");
    let body = arena.get(func.body).expect("body");
    assert_eq!(
        body.kind,
        syntax_kind_ext::ARROW_FUNCTION,
        "body should be nested arrow"
    );
}

#[test]
fn js_optional_parameter_span_starts_at_question_token() {
    let source = "const f = (b, c?: string) => c;";
    let (parser, root) = parse_source_named("fileJs.js", source);
    let arena = parser.get_arena();

    let init = get_var_initializer(arena, root);
    let arrow_node = arena.get(init).expect("arrow node");
    let arrow = arena.get_function(arrow_node).expect("arrow data");
    let param_idx = arrow.parameters.nodes[1];
    let param_node = arena.get(param_idx).expect("param node");

    assert_eq!(
        param_node.pos,
        source.find('?').expect("question token position") as u32,
        "JS optional parameter spans should anchor at '?' for JS-only diagnostics"
    );
}

// =============================================================================
// 3. Type Syntax Parsing (15+ tests)
// =============================================================================

#[test]
fn type_union() {
    // `type T = A | B | C`
    let (parser, root) = parse_source("type T = A | B | C;");
    assert_no_errors(&parser, "union type");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("type alias");
    let type_node = arena.get(alias.type_node).expect("type node");
    assert_eq!(
        type_node.kind,
        syntax_kind_ext::UNION_TYPE,
        "should be union type"
    );
    let composite = arena.get_composite_type(type_node).expect("composite");
    assert_eq!(composite.types.nodes.len(), 3, "should have 3 members");
}

#[test]
fn type_intersection() {
    // `type T = A & B & C`
    let (parser, root) = parse_source("type T = A & B & C;");
    assert_no_errors(&parser, "intersection type");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("type alias");
    let type_node = arena.get(alias.type_node).expect("type node");
    assert_eq!(
        type_node.kind,
        syntax_kind_ext::INTERSECTION_TYPE,
        "should be intersection type"
    );
    let composite = arena.get_composite_type(type_node).expect("composite");
    assert_eq!(composite.types.nodes.len(), 3, "should have 3 members");
}

#[test]
fn type_conditional() {
    // `type T = X extends Y ? A : B`
    let (parser, root) = parse_source("type T = X extends Y ? A : B;");
    assert_no_errors(&parser, "conditional type");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("type alias");
    let type_node = arena.get(alias.type_node).expect("type node");
    assert_eq!(
        type_node.kind,
        syntax_kind_ext::CONDITIONAL_TYPE,
        "should be conditional type"
    );
    let cond = arena.get_conditional_type(type_node).expect("cond type");
    assert!(cond.check_type.is_some(), "should have check type");
    assert!(cond.extends_type.is_some(), "should have extends type");
    assert!(cond.true_type.is_some(), "should have true type");
    assert!(cond.false_type.is_some(), "should have false type");
}

#[test]
fn type_conditional_nested() {
    // `type T = X extends A ? B extends C ? D : E : F`
    let (parser, root) = parse_source("type T = X extends A ? B extends C ? D : E : F;");
    assert_no_errors(&parser, "nested conditional type");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("type alias");
    let type_node = arena.get(alias.type_node).expect("type node");
    assert_eq!(type_node.kind, syntax_kind_ext::CONDITIONAL_TYPE);
    let outer = arena.get_conditional_type(type_node).expect("outer cond");
    // true branch should be a nested conditional type
    let true_node = arena.get(outer.true_type).expect("true branch");
    assert_eq!(
        true_node.kind,
        syntax_kind_ext::CONDITIONAL_TYPE,
        "true branch should be nested conditional"
    );
}

#[test]
fn type_mapped() {
    // `type T = { [K in keyof O]: O[K] }`
    let (parser, root) = parse_source("type T = { [K in keyof O]: O[K] };");
    assert_no_errors(&parser, "mapped type");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("type alias");
    let type_node = arena.get(alias.type_node).expect("type node");
    assert_eq!(
        type_node.kind,
        syntax_kind_ext::MAPPED_TYPE,
        "should be mapped type"
    );
    let mapped = arena.get_mapped_type(type_node).expect("mapped data");
    assert!(mapped.type_parameter.is_some(), "should have type param");
    assert!(mapped.type_node.is_some(), "should have type node");
}

#[test]
fn type_template_literal() {
    // type T = `${string}px`
    let (parser, root) = parse_source("type T = `${string}px`;");
    assert_no_errors(&parser, "template literal type");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("type alias");
    let type_node = arena.get(alias.type_node).expect("type node");
    assert_eq!(
        type_node.kind,
        syntax_kind_ext::TEMPLATE_LITERAL_TYPE,
        "should be template literal type"
    );
}

#[test]
fn type_tuple_with_labels() {
    // `type T = [name: string, age: number]`
    let (parser, root) = parse_source("type T = [name: string, age: number];");
    assert_no_errors(&parser, "tuple with labels");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("type alias");
    let type_node = arena.get(alias.type_node).expect("type node");
    assert_eq!(
        type_node.kind,
        syntax_kind_ext::TUPLE_TYPE,
        "should be tuple type"
    );
    let tuple = arena.get_tuple_type(type_node).expect("tuple data");
    assert_eq!(tuple.elements.nodes.len(), 2, "should have 2 elements");
    // Each element should be a NamedTupleMember
    let elem = arena.get(tuple.elements.nodes[0]).expect("elem0");
    assert_eq!(
        elem.kind,
        syntax_kind_ext::NAMED_TUPLE_MEMBER,
        "should be named tuple member"
    );
}

#[test]
fn type_tuple_optional_element() {
    // `type T = [string, number?]`
    let (parser, root) = parse_source("type T = [string, number?];");
    assert_no_errors(&parser, "tuple optional element");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("type alias");
    let type_node = arena.get(alias.type_node).expect("type node");
    assert_eq!(type_node.kind, syntax_kind_ext::TUPLE_TYPE);
    let tuple = arena.get_tuple_type(type_node).expect("tuple");
    assert_eq!(tuple.elements.nodes.len(), 2);
    // Second element should be an OptionalType
    let elem1 = arena.get(tuple.elements.nodes[1]).expect("elem1");
    assert_eq!(
        elem1.kind,
        syntax_kind_ext::OPTIONAL_TYPE,
        "should be optional type"
    );
}

#[test]
fn type_tuple_rest_element() {
    // `type T = [string, ...number[]]`
    let (parser, root) = parse_source("type T = [string, ...number[]];");
    assert_no_errors(&parser, "tuple rest element");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("type alias");
    let type_node = arena.get(alias.type_node).expect("type node");
    assert_eq!(type_node.kind, syntax_kind_ext::TUPLE_TYPE);
    let tuple = arena.get_tuple_type(type_node).expect("tuple");
    assert_eq!(tuple.elements.nodes.len(), 2);
    // Second element should be a RestType
    let elem1 = arena.get(tuple.elements.nodes[1]).expect("elem1");
    assert_eq!(
        elem1.kind,
        syntax_kind_ext::REST_TYPE,
        "should be rest type"
    );
}

#[test]
fn type_infer_in_conditional() {
    // `type T = X extends Array<infer U> ? U : never`
    let (parser, root) = parse_source("type T = X extends Array<infer U> ? U : never;");
    assert_no_errors(&parser, "infer in conditional");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("type alias");
    let type_node = arena.get(alias.type_node).expect("type node");
    assert_eq!(type_node.kind, syntax_kind_ext::CONDITIONAL_TYPE);
    // The extends type should be a TypeReference with type arguments containing infer
    let cond = arena.get_conditional_type(type_node).expect("cond");
    let extends_node = arena.get(cond.extends_type).expect("extends");
    assert_eq!(extends_node.kind, syntax_kind_ext::TYPE_REFERENCE);
    let type_ref = arena.get_type_ref(extends_node).expect("type ref");
    let args = type_ref.type_arguments.as_ref().expect("type args");
    assert_eq!(args.nodes.len(), 1);
    let infer_node = arena.get(args.nodes[0]).expect("infer");
    assert_eq!(
        infer_node.kind,
        syntax_kind_ext::INFER_TYPE,
        "should be infer type"
    );
}

#[test]
fn type_index_access() {
    // `type T = Foo["key"]`
    let (parser, root) = parse_source("type T = Foo[\"key\"];");
    assert_no_errors(&parser, "index access type");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("type alias");
    let type_node = arena.get(alias.type_node).expect("type node");
    assert_eq!(
        type_node.kind,
        syntax_kind_ext::INDEXED_ACCESS_TYPE,
        "should be indexed access type"
    );
}

#[test]
fn type_index_access_allows_line_break_before_bracket() {
    let source = "type T = Foo\n[\"key\"];";
    let (parser, root) = parse_source(source);
    assert_no_errors(&parser, "line-broken index access type alias");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("type alias");
    let type_node = arena.get(alias.type_node).expect("type node");
    assert_eq!(
        type_node.kind,
        syntax_kind_ext::INDEXED_ACCESS_TYPE,
        "line-broken type alias should still parse as indexed access"
    );
}

#[test]
fn type_annotation_index_access_allows_line_break_before_bracket() {
    let source = "let value: Foo\n[\"key\"];";
    let (parser, root) = parse_source(source);
    assert_no_errors(&parser, "line-broken index access type annotation");
    let arena = parser.get_arena();
    let type_annotation = get_var_type_annotation(arena, root);
    let type_node = arena.get(type_annotation).expect("type node");
    assert_eq!(
        type_node.kind,
        syntax_kind_ext::INDEXED_ACCESS_TYPE,
        "line-broken type annotation should still parse as indexed access"
    );
}

#[test]
fn type_index_access_number() {
    // `type T = Arr[number]`
    let (parser, root) = parse_source("type T = Arr[number];");
    assert_no_errors(&parser, "index access type number");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("type alias");
    let type_node = arena.get(alias.type_node).expect("type node");
    assert_eq!(type_node.kind, syntax_kind_ext::INDEXED_ACCESS_TYPE);
}

#[test]
fn type_typeof() {
    // `type T = typeof x`
    let (parser, root) = parse_source("type T = typeof x;");
    assert_no_errors(&parser, "typeof type");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("type alias");
    let type_node = arena.get(alias.type_node).expect("type node");
    assert_eq!(
        type_node.kind,
        syntax_kind_ext::TYPE_QUERY,
        "should be type query"
    );
}

#[test]
fn type_keyof() {
    // `type T = keyof X`
    let (parser, root) = parse_source("type T = keyof X;");
    assert_no_errors(&parser, "keyof type");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("type alias");
    let type_node = arena.get(alias.type_node).expect("type node");
    assert_eq!(
        type_node.kind,
        syntax_kind_ext::TYPE_OPERATOR,
        "should be type operator"
    );
    let op = arena.get_type_operator(type_node).expect("type operator");
    assert_eq!(
        op.operator,
        SyntaxKind::KeyOfKeyword as u16,
        "should be keyof"
    );
}

#[test]
fn type_function_type() {
    // `type T = (x: number) => string`
    let (parser, root) = parse_source("type T = (x: number) => string;");
    assert_no_errors(&parser, "function type");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("type alias");
    let type_node = arena.get(alias.type_node).expect("type node");
    assert_eq!(
        type_node.kind,
        syntax_kind_ext::FUNCTION_TYPE,
        "should be function type"
    );
    let func_type = arena.get_function_type(type_node).expect("func type data");
    assert_eq!(func_type.parameters.nodes.len(), 1);
    assert!(
        func_type.type_annotation.is_some(),
        "should have return type"
    );
}

#[test]
fn type_constructor_type() {
    // `type T = new (x: number) => Foo`
    let (parser, root) = parse_source("type T = new (x: number) => Foo;");
    assert_no_errors(&parser, "constructor type");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("type alias");
    let type_node = arena.get(alias.type_node).expect("type node");
    assert_eq!(
        type_node.kind,
        syntax_kind_ext::CONSTRUCTOR_TYPE,
        "should be constructor type"
    );
}

#[test]
fn type_array() {
    // `type T = number[]`
    let (parser, root) = parse_source("type T = number[];");
    assert_no_errors(&parser, "array type");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("type alias");
    let type_node = arena.get(alias.type_node).expect("type node");
    assert_eq!(
        type_node.kind,
        syntax_kind_ext::ARRAY_TYPE,
        "should be array type"
    );
}

#[test]
fn type_parenthesized() {
    // `type T = (A | B) & C`
    let (parser, root) = parse_source("type T = (A | B) & C;");
    assert_no_errors(&parser, "parenthesized type");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("type alias");
    let type_node = arena.get(alias.type_node).expect("type node");
    assert_eq!(
        type_node.kind,
        syntax_kind_ext::INTERSECTION_TYPE,
        "top should be intersection"
    );
}

#[test]
fn type_readonly_array() {
    // `type T = readonly number[]`
    let (parser, root) = parse_source("type T = readonly number[];");
    assert_no_errors(&parser, "readonly array");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("type alias");
    let type_node = arena.get(alias.type_node).expect("type node");
    assert_eq!(
        type_node.kind,
        syntax_kind_ext::TYPE_OPERATOR,
        "should be type operator (readonly)"
    );
    let op = arena.get_type_operator(type_node).expect("type op");
    assert_eq!(
        op.operator,
        SyntaxKind::ReadonlyKeyword as u16,
        "should be readonly"
    );
}

#[test]
fn type_this() {
    // `interface I { get(): this }`
    let (parser, root) = parse_source("interface I { get(): this; }");
    assert_no_errors(&parser, "this type");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let iface = arena.get_interface(stmt_node).expect("interface");
    let member_node = arena.get(iface.members.nodes[0]).expect("member");
    let sig = arena.get_signature(member_node).expect("signature");
    let ret_node = arena.get(sig.type_annotation).expect("return type");
    assert_eq!(
        ret_node.kind,
        syntax_kind_ext::THIS_TYPE,
        "should be this type"
    );
}

#[test]
fn type_literal_string() {
    // `type T = "hello"`
    let (parser, root) = parse_source("type T = \"hello\";");
    assert_no_errors(&parser, "literal type string");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("type alias");
    let type_node = arena.get(alias.type_node).expect("type node");
    assert_eq!(
        type_node.kind,
        syntax_kind_ext::LITERAL_TYPE,
        "should be literal type"
    );
}

// =============================================================================
// 4. Declaration Edge Cases (10+ tests)
// =============================================================================

#[test]
fn decl_export_default_function_anonymous() {
    // `export default function() {}` — wraps function in export declaration
    let (parser, root) = parse_source("export default function() {}");
    assert_no_errors(&parser, "export default function anonymous");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(
        stmt_node.kind,
        syntax_kind_ext::EXPORT_DECLARATION,
        "should be export declaration wrapping the function"
    );
    let export = arena.get_export_decl(stmt_node).expect("export decl");
    assert!(export.is_default_export, "should be default export");
}

#[test]
fn decl_export_as_default() {
    // `export { x as default }`
    let (parser, root) = parse_source("const x = 1; export { x as default };");
    assert_no_errors(&parser, "export { x as default }");
    let arena = parser.get_arena();
    let stmts = get_statements(arena, root);
    assert_eq!(stmts.len(), 2);
    let export_node = arena.get(stmts[1]).expect("export");
    assert_eq!(export_node.kind, syntax_kind_ext::EXPORT_DECLARATION);
}

#[test]
fn decl_import_type() {
    // `import type { Foo } from 'bar'`
    let (parser, root) = parse_source("import type { Foo } from 'bar';");
    assert_no_errors(&parser, "import type");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(stmt_node.kind, syntax_kind_ext::IMPORT_DECLARATION);
    let import = arena.get_import_decl(stmt_node).expect("import decl");
    let clause_node = arena.get(import.import_clause).expect("clause");
    let clause = arena.get_import_clause(clause_node).expect("import clause");
    assert!(clause.is_type_only, "should be type-only import");
}

#[test]
fn decl_declare_module_string_literal() {
    // `declare module 'foo' {}`
    let (parser, root) = parse_source("declare module 'foo' {}");
    assert_no_errors(&parser, "declare module");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(
        stmt_node.kind,
        syntax_kind_ext::MODULE_DECLARATION,
        "should be module declaration"
    );
}

#[test]
fn decl_ambient_function() {
    // `declare function f(): void`
    let (parser, root) = parse_source("declare function f(): void;");
    assert_no_errors(&parser, "ambient function");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(stmt_node.kind, syntax_kind_ext::FUNCTION_DECLARATION);
    let func = arena.get_function(stmt_node).expect("function");
    assert!(func.body.is_none(), "ambient function should have no body");
}

#[test]
fn decl_enum_basic() {
    // `enum Color { Red, Green, Blue }`
    let (parser, root) = parse_source("enum Color { Red, Green, Blue }");
    assert_no_errors(&parser, "enum");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(stmt_node.kind, syntax_kind_ext::ENUM_DECLARATION);
    let enum_data = arena.get_enum(stmt_node).expect("enum");
    assert_eq!(enum_data.members.nodes.len(), 3, "should have 3 members");
}

#[test]
fn decl_enum_with_initializers() {
    // `enum Dir { Up = 1, Down = 2 }`
    let (parser, root) = parse_source("enum Dir { Up = 1, Down = 2 }");
    assert_no_errors(&parser, "enum with initializers");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let enum_data = arena.get_enum(stmt_node).expect("enum");
    let member_node = arena.get(enum_data.members.nodes[0]).expect("member");
    let member = arena.get_enum_member(member_node).expect("member data");
    assert!(member.initializer.is_some(), "should have initializer");
}

#[test]
fn decl_enum_invalid_separator_recovery_keeps_members() {
    let (parser, root) = parse_source(
        "enum E13 { postSemicolon; postColonValueComma: 2, postColonValueSemicolon: 3; }\n\
enum E14 { a, b: any \"hello\" += 1, c, d }",
    );
    assert_has_errors(&parser, "invalid enum separators");

    let arena = parser.get_arena();
    let statements = get_statements(arena, root);
    let enum_member_names = |stmt_idx| {
        let stmt_node = arena.get(stmt_idx).expect("enum statement");
        let enum_data = arena.get_enum(stmt_node).expect("enum data");
        enum_data
            .members
            .nodes
            .iter()
            .map(|&member_idx| {
                let member_node = arena.get(member_idx).expect("enum member");
                let member = arena.get_enum_member(member_node).expect("member data");
                arena
                    .get_identifier_text(member.name)
                    .or_else(|| arena.get_literal_text(member.name))
                    .expect("member name text")
                    .to_string()
            })
            .collect::<Vec<_>>()
    };

    assert_eq!(
        enum_member_names(statements[0]),
        [
            "postSemicolon",
            "postColonValueComma",
            "2",
            "postColonValueSemicolon",
            "3"
        ]
    );
    assert_eq!(
        enum_member_names(statements[1]),
        ["a", "b", "any", "hello", "1", "c", "d"]
    );
}

#[test]
fn decl_enum_reserved_name_recovery_keeps_reserved_statement() {
    for source in ["enum void {}", "enum typeof {}", "enum delete {}"] {
        let (parser, root) = parse_source(source);
        assert_has_errors(&parser, "reserved enum name");

        let arena = parser.get_arena();
        let statements = get_statements(arena, root);
        assert_eq!(
            statements.len(),
            2,
            "{source}: should recover anonymous enum plus reserved-word statement"
        );

        let enum_node = arena.get(statements[0]).expect("enum statement");
        assert_eq!(enum_node.kind, syntax_kind_ext::ENUM_DECLARATION);
        assert_eq!(enum_node.pos, 0, "{source}: enum start");
        assert_eq!(enum_node.end, 4, "{source}: enum should end at keyword");
        let enum_data = arena.get_enum(enum_node).expect("enum data");
        assert_eq!(
            arena.get_identifier_text(enum_data.name),
            Some(""),
            "{source}: recovered enum name should be missing"
        );
        assert!(enum_data.members.nodes.is_empty(), "{source}: no members");

        let expr_node = arena
            .get(statements[1])
            .expect("reserved-word expression statement");
        assert_eq!(expr_node.kind, syntax_kind_ext::EXPRESSION_STATEMENT);
        assert_eq!(node_text(arena, source, statements[1]), &source[5..]);
    }

    let source = "enum class {}";
    let (parser, root) = parse_source(source);
    assert_has_errors(&parser, "reserved enum class name");

    let arena = parser.get_arena();
    let statements = get_statements(arena, root);
    assert_eq!(
        statements.len(),
        2,
        "{source}: should recover anonymous enum plus class declaration"
    );
    let enum_node = arena.get(statements[0]).expect("enum statement");
    assert_eq!(enum_node.kind, syntax_kind_ext::ENUM_DECLARATION);
    assert_eq!(enum_node.end, 4, "{source}: enum should end at keyword");
    let enum_data = arena.get_enum(enum_node).expect("enum data");
    assert_eq!(arena.get_identifier_text(enum_data.name), Some(""));
    let class_node = arena.get(statements[1]).expect("class declaration");
    assert_eq!(class_node.kind, syntax_kind_ext::CLASS_DECLARATION);
    assert_eq!(node_text(arena, source, statements[1]), "class {}");
}

#[test]
fn decl_const_enum() {
    // `const enum Flags { A, B }`
    let (parser, root) = parse_source("const enum Flags { A, B }");
    assert_no_errors(&parser, "const enum");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(stmt_node.kind, syntax_kind_ext::ENUM_DECLARATION);
}

#[test]
fn decl_namespace() {
    // `namespace Foo { export const x = 1; }`
    let (parser, root) = parse_source("namespace Foo { export const x = 1; }");
    assert_no_errors(&parser, "namespace");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(stmt_node.kind, syntax_kind_ext::MODULE_DECLARATION);
}

#[test]
fn decl_export_equals() {
    // `export = x`
    let (parser, root) = parse_source("export = x;");
    assert_no_errors(&parser, "export equals");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(
        stmt_node.kind,
        syntax_kind_ext::EXPORT_ASSIGNMENT,
        "should be export assignment"
    );
    let export = arena.get_export_assignment(stmt_node).expect("export");
    assert!(export.is_export_equals, "should be export =");
}

#[test]
fn decl_export_default_expression() {
    // `export default 42` — parsed as EXPORT_DECLARATION with default flag
    let (parser, root) = parse_source("export default 42;");
    assert_no_errors(&parser, "export default expression");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(
        stmt_node.kind,
        syntax_kind_ext::EXPORT_DECLARATION,
        "should be export declaration"
    );
    let export = arena.get_export_decl(stmt_node).expect("export decl");
    assert!(export.is_default_export, "should be default export");
}

#[test]
fn decl_interface_with_extends() {
    // `interface Foo extends Bar, Baz { x: number; }`
    let (parser, root) = parse_source("interface Foo extends Bar, Baz { x: number; }");
    assert_no_errors(&parser, "interface extends");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(stmt_node.kind, syntax_kind_ext::INTERFACE_DECLARATION);
    let iface = arena.get_interface(stmt_node).expect("interface");
    assert!(iface.heritage_clauses.is_some(), "should have extends");
    assert_eq!(iface.members.nodes.len(), 1, "should have 1 member");
}

#[test]
fn decl_type_alias_generic() {
    // `type Box<T> = { value: T }`
    let (parser, root) = parse_source("type Box<T> = { value: T };");
    assert_no_errors(&parser, "generic type alias");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(stmt_node.kind, syntax_kind_ext::TYPE_ALIAS_DECLARATION);
    let alias = arena.get_type_alias(stmt_node).expect("type alias");
    assert!(
        alias.type_parameters.is_some(),
        "should have type parameters"
    );
}

// =============================================================================
// 5. Class Syntax (10+ tests)
// =============================================================================

#[test]
fn class_basic() {
    // `class Foo { x: number; }`
    let (parser, root) = parse_source("class Foo { x: number; }");
    assert_no_errors(&parser, "basic class");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(stmt_node.kind, syntax_kind_ext::CLASS_DECLARATION);
    let class = arena.get_class(stmt_node).expect("class");
    assert_eq!(class.members.nodes.len(), 1, "should have 1 member");
}

#[test]
fn class_private_field() {
    // `class Foo { #x: number; }`
    let (parser, root) = parse_source("class Foo { #x: number; }");
    assert_no_errors(&parser, "private field");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    let member_node = arena.get(class.members.nodes[0]).expect("member");
    assert_eq!(
        member_node.kind,
        syntax_kind_ext::PROPERTY_DECLARATION,
        "should be property declaration"
    );
    let prop = arena.get_property_decl(member_node).expect("prop");
    let name_node = arena.get(prop.name).expect("name");
    assert_eq!(
        name_node.kind,
        SyntaxKind::PrivateIdentifier as u16,
        "should be private identifier"
    );
}

#[test]
fn class_static_block() {
    // `class Foo { static { console.log("init"); } }`
    let (parser, root) = parse_source("class Foo { static { console.log(\"init\"); } }");
    assert_no_errors(&parser, "static block");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    assert!(!class.members.nodes.is_empty(), "should have members");
    let member_node = arena.get(class.members.nodes[0]).expect("member");
    assert_eq!(
        member_node.kind,
        syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION,
        "should be static block"
    );
}

#[test]
fn class_abstract_method() {
    // `abstract class Foo { abstract bar(): void; }`
    let (parser, root) = parse_source("abstract class Foo { abstract bar(): void; }");
    assert_no_errors(&parser, "abstract method");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    let member_node = arena.get(class.members.nodes[0]).expect("member");
    assert_eq!(
        member_node.kind,
        syntax_kind_ext::METHOD_DECLARATION,
        "should be method"
    );
    let method = arena.get_method_decl(member_node).expect("method");
    assert!(
        arena.has_modifier(&method.modifiers, SyntaxKind::AbstractKeyword),
        "should have abstract modifier"
    );
}

#[test]
fn class_parameter_property() {
    // `class Foo { constructor(public x: number) {} }`
    let (parser, root) = parse_source("class Foo { constructor(public x: number) {} }");
    assert_no_errors(&parser, "parameter property");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    let ctor_node = arena.get(class.members.nodes[0]).expect("ctor");
    assert_eq!(
        ctor_node.kind,
        syntax_kind_ext::CONSTRUCTOR,
        "should be constructor"
    );
    let ctor = arena.get_constructor(ctor_node).expect("ctor data");
    let param_node = arena.get(ctor.parameters.nodes[0]).expect("param");
    let param = arena.get_parameter(param_node).expect("param data");
    assert!(
        arena.has_modifier(&param.modifiers, SyntaxKind::PublicKeyword),
        "should have public modifier"
    );
}

fn assert_incomplete_constructor_return_colon_recovers_class_members(
    source: &str,
    expected_member_kinds: &[u16],
) {
    let (parser, root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&diagnostic_codes::TYPE_EXPECTED),
        "expected TS1110 for the missing constructor return type, got {diagnostics:?}"
    );
    assert!(
        !codes.contains(
            &diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED
        ),
        "constructor return-type recovery should not cascade into TS1068, got {diagnostics:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED),
        "constructor return-type recovery should keep following members in the class, got {diagnostics:?}"
    );

    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    let actual_member_kinds: Vec<u16> = class
        .members
        .nodes
        .iter()
        .map(|&member| arena.get(member).expect("member").kind)
        .collect();
    assert_eq!(
        actual_member_kinds, expected_member_kinds,
        "expected recovered class member boundaries"
    );
}

#[test]
fn constructor_return_colon_no_params_recovers_following_member() {
    assert_incomplete_constructor_return_colon_recovers_class_members(
        "class C {\n  constructor():\n  m() {}\n}",
        &[
            syntax_kind_ext::CONSTRUCTOR,
            syntax_kind_ext::METHOD_DECLARATION,
        ],
    );
}

#[test]
fn constructor_return_colon_normal_params_recovers_following_member() {
    assert_incomplete_constructor_return_colon_recovers_class_members(
        "class C {\n  constructor(value: string):\n  m() {}\n}",
        &[
            syntax_kind_ext::CONSTRUCTOR,
            syntax_kind_ext::METHOD_DECLARATION,
        ],
    );
}

#[test]
fn constructor_return_colon_parameter_properties_recovers_following_member() {
    assert_incomplete_constructor_return_colon_recovers_class_members(
        "class C {\n  constructor(public value: string):\n  m() {}\n}",
        &[
            syntax_kind_ext::CONSTRUCTOR,
            syntax_kind_ext::METHOD_DECLARATION,
        ],
    );
}

#[test]
fn constructor_return_colon_recovers_following_overload_pair() {
    assert_incomplete_constructor_return_colon_recovers_class_members(
        "class C {\n  constructor(private value: string):\n  overload(value: string);\n  overload(value: string) {}\n}",
        &[
            syntax_kind_ext::CONSTRUCTOR,
            syntax_kind_ext::METHOD_DECLARATION,
            syntax_kind_ext::METHOD_DECLARATION,
        ],
    );
}

#[test]
fn class_decorator() {
    // `@dec class Foo {}`
    let (parser, root) = parse_source("declare var dec: any; @dec class Foo {}");
    assert_no_errors(&parser, "class decorator");
    let arena = parser.get_arena();
    let stmts = get_statements(arena, root);
    let class_node = arena.get(stmts[1]).expect("class node");
    assert_eq!(class_node.kind, syntax_kind_ext::CLASS_DECLARATION);
    let class = arena.get_class(class_node).expect("class");
    // Modifiers should include a decorator
    let mods = class.modifiers.as_ref().expect("modifiers");
    let has_decorator = mods.nodes.iter().any(|&idx| {
        arena
            .get(idx)
            .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
    });
    assert!(has_decorator, "should have decorator modifier");
}

#[test]
fn class_multiple_decorators() {
    // `@a @b class Foo {}`
    let (parser, root) = parse_source("declare var a: any; declare var b: any; @a @b class Foo {}");
    assert_no_errors(&parser, "multiple decorators");
    let arena = parser.get_arena();
    let stmts = get_statements(arena, root);
    let class_node = arena.get(stmts[2]).expect("class node");
    let class = arena.get_class(class_node).expect("class");
    let mods = class.modifiers.as_ref().expect("modifiers");
    let decorator_count = mods
        .nodes
        .iter()
        .filter(|&&idx| {
            arena
                .get(idx)
                .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
        })
        .count();
    assert_eq!(decorator_count, 2, "should have 2 decorators");
}

#[test]
fn class_index_signature() {
    // `class Foo { [key: string]: number; }`
    let (parser, root) = parse_source("class Foo { [key: string]: number; }");
    assert_no_errors(&parser, "class index signature");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    let member_node = arena.get(class.members.nodes[0]).expect("member");
    assert_eq!(
        member_node.kind,
        syntax_kind_ext::INDEX_SIGNATURE,
        "should be index signature"
    );
}

#[test]
fn class_computed_property() {
    // `class Foo { [Symbol.iterator]() {} }`
    let (parser, root) = parse_source("class Foo { [Symbol.iterator]() {} }");
    assert_no_errors(&parser, "computed property name");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    let member_node = arena.get(class.members.nodes[0]).expect("member");
    assert_eq!(
        member_node.kind,
        syntax_kind_ext::METHOD_DECLARATION,
        "should be method"
    );
    let method = arena.get_method_decl(member_node).expect("method");
    let name_node = arena.get(method.name).expect("name");
    assert_eq!(
        name_node.kind,
        syntax_kind_ext::COMPUTED_PROPERTY_NAME,
        "name should be computed property"
    );
}

#[test]
fn computed_field_typed_initializer_continuation_reports_ts1005() {
    let source = "class C {\n    [e]: number = 0\n    [e2]: number\n}";
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|diag| diag.code).collect();
    let colon_pos = source
        .rfind(": number")
        .expect("expected second type annotation") as u32;

    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == diagnostic_codes::EXPECTED && diag.start == colon_pos),
        "expected TS1005 at the continuation type annotation colon, got {diagnostics:?}"
    );
    assert!(
        !codes.contains(
            &diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED
        ),
        "continuation type annotation should not cascade into TS1068, got {diagnostics:?}"
    );
}

