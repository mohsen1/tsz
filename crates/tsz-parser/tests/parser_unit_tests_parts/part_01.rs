#[test]
fn computed_field_method_like_continuation_reports_ts1005_before_outer_block() {
    let source = "class C {\n    [e] = 0\n    [e2]() { }\n}";
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|diag| diag.code).collect();
    let block_pos = source.find("{ }").expect("expected recovered method body") as u32;

    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == diagnostic_codes::EXPECTED && diag.start == block_pos),
        "expected TS1005 at the recovered method body brace, got {diagnostics:?}"
    );
    assert!(
        codes.contains(&diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED),
        "method-like continuation should still recover the body as an outer block, got {diagnostics:?}"
    );
    assert!(
        !codes.contains(
            &diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED
        ),
        "method-like continuation should not cascade into TS1068, got {diagnostics:?}"
    );
}

#[test]
fn computed_field_followed_by_bare_block_reports_ts1068() {
    let source = "class C {\n    ['a'] = 0\n    {}\n}";
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|diag| diag.code).collect();
    let block_pos = source.find("{}").expect("expected recovered block") as u32;

    assert!(
        diagnostics.iter().any(|diag| {
            diag.code
                == diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED
                && diag.start == block_pos
        }),
        "bare block after a computed field initializer should report TS1068, got {diagnostics:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::EXPECTED),
        "bare block recovery should not degrade to TS1005, got {diagnostics:?}"
    );
}

#[test]
fn class_getter_setter() {
    // `class Foo { get x() { return 1; } set x(v: number) {} }`
    let (parser, root) = parse_source("class Foo { get x() { return 1; } set x(v: number) {} }");
    assert_no_errors(&parser, "getter/setter");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    assert_eq!(class.members.nodes.len(), 2, "should have getter + setter");
    let getter = arena.get(class.members.nodes[0]).expect("getter");
    assert_eq!(
        getter.kind,
        syntax_kind_ext::GET_ACCESSOR,
        "first should be getter"
    );
    let setter = arena.get(class.members.nodes[1]).expect("setter");
    assert_eq!(
        setter.kind,
        syntax_kind_ext::SET_ACCESSOR,
        "second should be setter"
    );
}

#[test]
fn class_extends_implements() {
    // `class Foo extends Bar implements Baz {}`
    let (parser, root) = parse_source("class Foo extends Bar implements Baz {}");
    assert_no_errors(&parser, "extends implements");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    let heritage = class.heritage_clauses.as_ref().expect("heritage clauses");
    assert_eq!(
        heritage.nodes.len(),
        2,
        "should have extends + implements clauses"
    );
}

#[test]
fn class_duplicate_extends_recovery_keeps_duplicate_clause_types() {
    // tsc reports TS1172 ('extends' clause already seen) but still preserves
    // the duplicate clause in the AST so JS emit prints `extends A extends B`
    // verbatim (matching tsc baseline output for `extendsClauseAlreadySeen`
    // and `parserClassDeclaration2`).  Checker iterators that take the
    // .first() heritage clause continue to ignore the duplicate.
    let (parser, root) = parse_source("class C extends A extends B {}");
    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        codes.contains(&diagnostic_codes::EXTENDS_CLAUSE_ALREADY_SEEN),
        "expected TS1172 for duplicate extends clause, got {:?}",
        parser.get_diagnostics()
    );

    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    let heritage = class.heritage_clauses.as_ref().expect("heritage clauses");
    assert_eq!(
        heritage.nodes.len(),
        2,
        "duplicate extends recovery should keep both heritage clauses for emit parity"
    );

    let first_node = arena.get(heritage.nodes[0]).expect("first heritage node");
    let first = arena.get_heritage(first_node).expect("first heritage data");
    assert_eq!(
        first.types.nodes.len(),
        1,
        "first extends clause should keep its base type"
    );

    let second_node = arena.get(heritage.nodes[1]).expect("second heritage node");
    let second = arena
        .get_heritage(second_node)
        .expect("second heritage data");
    assert_eq!(
        second.types.nodes.len(),
        1,
        "duplicate extends clause should keep its base type so emit prints it"
    );
}

#[test]
fn class_duplicate_implements_recovery_discards_duplicate_clause_types() {
    let (parser, root) = parse_source("class C implements A implements B {}");
    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        codes.contains(&diagnostic_codes::IMPLEMENTS_CLAUSE_ALREADY_SEEN),
        "expected TS1175 for duplicate implements clause, got {:?}",
        parser.get_diagnostics()
    );

    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    let heritage = class.heritage_clauses.as_ref().expect("heritage clauses");
    assert_eq!(
        heritage.nodes.len(),
        1,
        "duplicate implements recovery should keep only the first heritage clause"
    );

    let clause_node = arena.get(heritage.nodes[0]).expect("heritage node");
    let clause = arena.get_heritage(clause_node).expect("heritage data");
    assert_eq!(
        clause.types.nodes.len(),
        1,
        "duplicate implements recovery should keep only the first implemented type"
    );
}

#[test]
fn class_extends_comma_recovery_keeps_single_base_type() {
    let source = "class C extends A, B {}";
    let (parser, root) = parse_source(source);
    let diags = parser.get_diagnostics();
    let ts1174 = diags
        .iter()
        .find(|diag| diag.code == diagnostic_codes::CLASSES_CAN_ONLY_EXTEND_A_SINGLE_CLASS)
        .expect("expected TS1174 for comma-separated extends");
    let b_pos = source.find('B').expect("B position") as u32;
    assert_eq!(
        ts1174.start, b_pos,
        "TS1174 should point at the extra base type, got {diags:?}"
    );

    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    let heritage = class.heritage_clauses.as_ref().expect("heritage clauses");
    assert_eq!(
        heritage.nodes.len(),
        1,
        "comma extends recovery should keep a single heritage clause"
    );

    let clause_node = arena.get(heritage.nodes[0]).expect("heritage node");
    let clause = arena.get_heritage(clause_node).expect("heritage data");
    assert_eq!(
        clause.types.nodes.len(),
        2,
        "comma extends recovery should preserve all base types for emit (matching tsc)"
    );
}

#[test]
fn class_out_of_order_extends_recovery_keeps_trailing_clause() {
    let (parser, root) = parse_source("class C implements A extends B {}");
    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        codes.contains(&diagnostic_codes::EXTENDS_CLAUSE_MUST_PRECEDE_IMPLEMENTS_CLAUSE),
        "expected TS1173 for out-of-order extends clause, got {:?}",
        parser.get_diagnostics()
    );

    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    let heritage = class.heritage_clauses.as_ref().expect("heritage clauses");
    assert_eq!(
        heritage.nodes.len(),
        2,
        "out-of-order extends recovery should keep both heritage clauses"
    );
}

#[test]
fn class_extends_object_literal_recovery_keeps_body_and_uses_ts1005() {
    let source = "class C extends { foo: string; } { method() {} }";
    let (parser, root) = parse_source(source);
    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        codes.contains(&diagnostic_codes::EXPECTED),
        "expected TS1005 from the object-literal separator recovery, got {:?}",
        parser.get_diagnostics()
    );
    assert!(
        !codes.contains(&diagnostic_codes::LIST_CANNOT_BE_EMPTY),
        "should not treat the object literal as an empty extends list, got {:?}",
        parser.get_diagnostics()
    );
    assert!(
        !codes.contains(
            &diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED
        ),
        "should not spill the heritage literal into class-member parsing, got {:?}",
        parser.get_diagnostics()
    );
    assert!(
        !codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
        "should not emit TS1109 for object literal bases, got {:?}",
        parser.get_diagnostics()
    );

    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    let heritage = class.heritage_clauses.as_ref().expect("heritage clauses");
    assert_eq!(
        heritage.nodes.len(),
        1,
        "should keep a single extends clause"
    );
    let clause_node = arena.get(heritage.nodes[0]).expect("heritage node");
    let clause = arena.get_heritage(clause_node).expect("heritage data");
    assert_eq!(
        clause.types.nodes.len(),
        1,
        "should keep one base expression"
    );
    let base_node = arena.get(clause.types.nodes[0]).expect("base");
    assert_eq!(
        base_node.kind,
        syntax_kind_ext::OBJECT_LITERAL_EXPRESSION,
        "extends base should recover as an object literal expression"
    );
    assert_eq!(
        class.members.nodes.len(),
        1,
        "class body should still parse"
    );
}

#[test]
fn class_extends_array_literal_expression_keeps_body() {
    let source = "class C extends [] { method() {} }";
    let (parser, root) = parse_source(source);
    assert_no_errors(&parser, "class extends array literal");

    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    let heritage = class.heritage_clauses.as_ref().expect("heritage clauses");
    assert_eq!(
        heritage.nodes.len(),
        1,
        "should keep a single extends clause"
    );
    let clause_node = arena.get(heritage.nodes[0]).expect("heritage node");
    let clause = arena.get_heritage(clause_node).expect("heritage data");
    assert_eq!(
        clause.types.nodes.len(),
        1,
        "should keep one base expression"
    );
    let base_node = arena.get(clause.types.nodes[0]).expect("base");
    assert_eq!(
        base_node.kind,
        syntax_kind_ext::ARRAY_LITERAL_EXPRESSION,
        "extends base should recover as an array literal expression"
    );
    assert_eq!(
        class.members.nodes.len(),
        1,
        "class body should still parse"
    );
}

#[test]
fn class_extends_void_emits_ts1109_and_preserves_body() {
    let (parser, root) = parse_source("class C extends void { method() {} }");
    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
        "expected TS1109 for `extends void`, got {:?}",
        parser.get_diagnostics()
    );
    assert!(
        !codes.contains(&diagnostic_codes::LIST_CANNOT_BE_EMPTY),
        "should not treat `void` as an empty extends list, got {:?}",
        parser.get_diagnostics()
    );
    assert!(
        !codes.contains(
            &diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED
        ),
        "should not spill `void` into class-member parsing, got {:?}",
        parser.get_diagnostics()
    );

    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    assert_eq!(
        class.members.nodes.len(),
        1,
        "class body should still parse"
    );
}

#[test]
fn class_empty_extends_list_still_reports_ts1097() {
    let (parser, _root) = parse_source("class C extends { }");
    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        codes.contains(&diagnostic_codes::LIST_CANNOT_BE_EMPTY),
        "expected TS1097 for an empty extends list, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn interface_extends_array_literal_reports_interface_heritage_error() {
    let (parser, _root) = parse_source("interface I extends [] {}");
    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        codes.contains(
            &diagnostic_codes::AN_INTERFACE_CAN_ONLY_EXTEND_AN_IDENTIFIER_QUALIFIED_NAME_WITH_OPTIONAL_TYPE_ARG
        ),
        "expected the interface-specific heritage diagnostic, got {:?}",
        parser.get_diagnostics()
    );
}

// =============================================================================
// 6. Statement Edge Cases (8+ tests)
// =============================================================================

#[test]
fn stmt_labeled() {
    // `label: for (;;) {}`
    let (parser, root) = parse_source("label: for (;;) {}");
    assert_no_errors(&parser, "labeled statement");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(
        stmt_node.kind,
        syntax_kind_ext::LABELED_STATEMENT,
        "should be labeled"
    );
    let labeled = arena.get_labeled_statement(stmt_node).expect("labeled");
    let inner = arena.get(labeled.statement).expect("inner");
    assert_eq!(
        inner.kind,
        syntax_kind_ext::FOR_STATEMENT,
        "body should be for"
    );
}

#[test]
fn stmt_for_await_of() {
    // `for await (const x of iter) {}`
    let (parser, root) = parse_source("async function f() { for await (const x of iter) {} }");
    assert_no_errors(&parser, "for await of");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let func_node = arena.get(stmt_idx).expect("func");
    let func = arena.get_function(func_node).expect("func data");
    let body_node = arena.get(func.body).expect("body");
    let block = arena.get_block(body_node).expect("block");
    let for_node = arena.get(block.statements.nodes[0]).expect("for");
    assert_eq!(
        for_node.kind,
        syntax_kind_ext::FOR_OF_STATEMENT,
        "should be for-of"
    );
    let for_data = arena.get_for_in_of(for_node).expect("for data");
    assert!(for_data.await_modifier, "should have await modifier");
}

#[test]
fn stmt_switch_with_fallthrough() {
    // Switch with fallthrough
    let (parser, root) = parse_source("switch (x) { case 1: case 2: break; default: break; }");
    assert_no_errors(&parser, "switch with fallthrough");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(stmt_node.kind, syntax_kind_ext::SWITCH_STATEMENT);
}

#[test]
fn stmt_try_catch_finally() {
    // `try {} catch (e) {} finally {}`
    let (parser, root) = parse_source("try {} catch (e) {} finally {}");
    assert_no_errors(&parser, "try/catch/finally");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(stmt_node.kind, syntax_kind_ext::TRY_STATEMENT);
    let try_data = arena.get_try(stmt_node).expect("try data");
    assert!(try_data.try_block.is_some(), "should have try block");
    assert!(try_data.catch_clause.is_some(), "should have catch clause");
    assert!(
        try_data.finally_block.is_some(),
        "should have finally block"
    );
}

#[test]
fn stmt_try_finally_no_catch() {
    // `try {} finally {}`
    let (parser, root) = parse_source("try {} finally {}");
    assert_no_errors(&parser, "try/finally no catch");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let try_data = arena.get_try(stmt_node).expect("try data");
    assert!(try_data.catch_clause.is_none(), "should have no catch");
    assert!(try_data.finally_block.is_some(), "should have finally");
}

#[test]
fn stmt_catch_without_binding() {
    // `try {} catch {}`  (ES2019 optional catch binding)
    let (parser, root) = parse_source("try {} catch {}");
    assert_no_errors(&parser, "catch without binding");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let try_data = arena.get_try(stmt_node).expect("try data");
    let catch_node = arena.get(try_data.catch_clause).expect("catch");
    let catch = arena.get_catch_clause(catch_node).expect("catch data");
    assert!(
        catch.variable_declaration.is_none(),
        "should have no binding"
    );
}

#[test]
fn stmt_with() {
    // `with (obj) { x; }` (legacy)
    let (parser, root) = parse_source("with (obj) { x; }");
    assert_no_errors(&parser, "with statement");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(
        stmt_node.kind,
        syntax_kind_ext::WITH_STATEMENT,
        "should be with statement"
    );
}

#[test]
fn stmt_empty() {
    // `;` (empty statement)
    let (parser, root) = parse_source(";");
    assert_no_errors(&parser, "empty statement");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(
        stmt_node.kind,
        syntax_kind_ext::EMPTY_STATEMENT,
        "should be empty statement"
    );
}

#[test]
fn stmt_debugger() {
    // `debugger;`
    let (parser, root) = parse_source("debugger;");
    assert_no_errors(&parser, "debugger statement");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(
        stmt_node.kind,
        syntax_kind_ext::DEBUGGER_STATEMENT,
        "should be debugger"
    );
}

#[test]
fn stmt_for_in() {
    // `for (const k in obj) {}`
    let (parser, root) = parse_source("for (const k in obj) {}");
    assert_no_errors(&parser, "for-in");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(
        stmt_node.kind,
        syntax_kind_ext::FOR_IN_STATEMENT,
        "should be for-in"
    );
}

#[test]
fn stmt_for_of() {
    // `for (const x of arr) {}`
    let (parser, root) = parse_source("for (const x of arr) {}");
    assert_no_errors(&parser, "for-of");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(
        stmt_node.kind,
        syntax_kind_ext::FOR_OF_STATEMENT,
        "should be for-of"
    );
}

#[test]
fn stmt_do_while() {
    // `do { x++; } while (x < 10);`
    let (parser, root) = parse_source("do { x++; } while (x < 10);");
    assert_no_errors(&parser, "do-while");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    assert_eq!(
        stmt_node.kind,
        syntax_kind_ext::DO_STATEMENT,
        "should be do-while"
    );
}

#[test]
fn stmt_break_continue_with_label() {
    // `outer: for (;;) { inner: for (;;) { break outer; continue inner; } }`
    let (parser, root) =
        parse_source("outer: for (;;) { inner: for (;;) { break outer; continue inner; } }");
    assert_no_errors(&parser, "break/continue with label");
    // The existence of no errors proves the parser handles labeled break/continue
    let arena = parser.get_arena();
    let sf = arena.get_source_file_at(root).expect("source file");
    assert!(!sf.statements.nodes.is_empty());
}

// =============================================================================
// 7. Error Recovery (5+ tests)
// =============================================================================

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
fn expr_tagged_template_with_type_arguments() {
    for (source, context) in [
        (
            "const value = tag<number>`hello`;",
            "no-substitution tagged template type arguments",
        ),
        (
            "const value = tag<number>`hello ${name}`;",
            "substituted tagged template type arguments",
        ),
    ] {
        let (parser, root) = parse_source(source);
        assert_no_errors(&parser, context);

        let arena = parser.get_arena();
        let init = get_var_initializer(arena, root);
        let node = arena.get(init).expect("initializer");
        assert_eq!(
            node.kind,
            syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION,
            "{context}: should parse as a tagged template expression"
        );

        let tagged = arena
            .get_tagged_template(node)
            .expect("tagged template data");
        let type_arguments = tagged
            .type_arguments
            .as_ref()
            .expect("tagged template should retain type arguments");
        assert_eq!(
            type_arguments.nodes.len(),
            1,
            "{context}: expected exactly one type argument"
        );
    }
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
fn expr_prefix_update_delete_recovery_keeps_outer_update_with_missing_operand() {
    // `++ delete foo.bar` parses as two statements in tsc:
    //   1) PrefixUnaryExpression `++<missing>` (TS1109 at `delete`)
    //   2) PrefixUnaryExpression `delete foo.bar`
    // The JS emitter prints them as `++;\ndelete foo.bar;`.
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
    let stmts = get_statements(arena, root);
    assert_eq!(
        stmts.len(),
        2,
        "expected outer ++ and inner delete to be two statements: {stmts:?}"
    );

    let outer = arena.get(stmts[0]).expect("outer stmt");
    let outer_expr_stmt = arena
        .get_expression_statement(outer)
        .expect("outer expression statement");
    let outer_expr = arena.get(outer_expr_stmt.expression).expect("outer expr");
    assert_eq!(
        outer_expr.kind,
        syntax_kind_ext::PREFIX_UNARY_EXPRESSION,
        "outer ++ should be a prefix unary expression"
    );
    let outer_unary = arena.get_unary_expr(outer_expr).expect("outer unary data");
    assert_eq!(
        outer_unary.operator,
        SyntaxKind::PlusPlusToken as u16,
        "outer operator should be `++`"
    );
    assert!(
        outer_unary.operand.is_none(),
        "outer ++ should have a missing operand"
    );

    let inner = arena.get(stmts[1]).expect("inner stmt");
    let inner_expr_stmt = arena
        .get_expression_statement(inner)
        .expect("inner expression statement");
    let inner_expr = arena.get(inner_expr_stmt.expression).expect("inner expr");
    assert_eq!(
        inner_expr.kind,
        syntax_kind_ext::PREFIX_UNARY_EXPRESSION,
        "inner delete should be a prefix unary expression"
    );
    let inner_unary = arena.get_unary_expr(inner_expr).expect("inner unary data");
    assert_eq!(
        inner_unary.operator,
        SyntaxKind::DeleteKeyword as u16,
        "inner operator should be `delete`"
    );
}

#[test]
fn expr_prefix_update_repeated_operator_keeps_outer_update_with_missing_operand() {
    // `++\n++y;` parses as two statements in tsc:
    //   1) PrefixUnaryExpression `++<missing>` (TS1109 at second `++`)
    //   2) PrefixUnaryExpression `++y`
    // The JS emitter prints them as `++;\n++y;`.
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
    let stmts = get_statements(arena, root);
    assert_eq!(
        stmts.len(),
        2,
        "expected outer ++ and inner ++y to be two statements: {stmts:?}"
    );

    let outer = arena.get(stmts[0]).expect("outer stmt");
    let outer_expr_stmt = arena
        .get_expression_statement(outer)
        .expect("outer expression statement");
    let outer_expr = arena.get(outer_expr_stmt.expression).expect("outer expr");
    assert_eq!(outer_expr.kind, syntax_kind_ext::PREFIX_UNARY_EXPRESSION);
    let outer_unary = arena.get_unary_expr(outer_expr).expect("outer unary data");
    assert_eq!(outer_unary.operator, SyntaxKind::PlusPlusToken as u16);
    assert!(
        outer_unary.operand.is_none(),
        "outer ++ should have a missing operand"
    );

    let inner = arena.get(stmts[1]).expect("inner stmt");
    let inner_expr_stmt = arena
        .get_expression_statement(inner)
        .expect("inner expression statement");
    let inner_expr = arena.get(inner_expr_stmt.expression).expect("inner expr");
    assert_eq!(inner_expr.kind, syntax_kind_ext::PREFIX_UNARY_EXPRESSION);
    let inner_unary = arena.get_unary_expr(inner_expr).expect("inner unary data");
    assert_eq!(inner_unary.operator, SyntaxKind::PlusPlusToken as u16);
    let inner_operand = arena
        .get(inner_unary.operand)
        .expect("inner operand should exist");
    assert_eq!(
        inner_operand.kind,
        SyntaxKind::Identifier as u16,
        "inner ++ should target the identifier"
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
    // `let o8 = { ...*o };` — `*` at the start of the spread operand is a
    // binary operator with a missing LHS. tsc's recovery produces a
    // BinaryExpression (missing * o), and we match that tree shape.
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
    // Match tsc: the `*` is consumed as a binary operator with a missing LHS.
    // The right-hand operand is the identifier `o`.
    assert_eq!(
        operand.kind,
        syntax_kind_ext::BINARY_EXPRESSION,
        "expected BinaryExpression for `*o` recovery, got kind={}",
        operand.kind
    );
    let binary = arena
        .get_binary_expr(operand)
        .expect("binary expression data");
    assert_eq!(
        binary.operator_token,
        SyntaxKind::AsteriskToken as u16,
        "operator should be `*`"
    );
    let right = arena.get(binary.right).expect("binary right");
    assert_eq!(right.kind, SyntaxKind::Identifier as u16);
    assert_eq!(node_text(arena, source, binary.right), "o");
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
fn expr_prefix_update_repeated_operator_after_line_break_matches_sputnik_shape() {
    // Sputnik S7.9_A5.7_T1: tsc emits two top-level statements after the var
    // declarations: `++<missing>;` and `++y;`. Track the AST shape so the JS
    // emitter prints the `++;\n++y;` baseline.
    let source = "var x=0, y=0;\nvar z=\nx\n++\n++\ny\n";
    let (parser, root) = parse_source(source);

    let arena = parser.get_arena();
    let stmts = get_statements(arena, root);
    assert_eq!(
        stmts.len(),
        4,
        "expected 4 top-level statements (two `var`s, outer `++`, inner `++y`): {stmts:?}"
    );

    let outer = arena.get(stmts[2]).expect("outer ++");
    let outer_expr_stmt = arena
        .get_expression_statement(outer)
        .expect("outer expression statement");
    let outer_expr = arena.get(outer_expr_stmt.expression).expect("outer expr");
    assert_eq!(outer_expr.kind, syntax_kind_ext::PREFIX_UNARY_EXPRESSION);
    let outer_unary = arena.get_unary_expr(outer_expr).expect("outer unary data");
    assert_eq!(outer_unary.operator, SyntaxKind::PlusPlusToken as u16);
    assert!(
        outer_unary.operand.is_none(),
        "outer ++ should have a missing operand"
    );

    let inner = arena.get(stmts[3]).expect("inner ++y");
    let inner_expr_stmt = arena
        .get_expression_statement(inner)
        .expect("inner expression statement");
    let inner_expr = arena.get(inner_expr_stmt.expression).expect("inner expr");
    assert_eq!(inner_expr.kind, syntax_kind_ext::PREFIX_UNARY_EXPRESSION);
    let inner_unary = arena.get_unary_expr(inner_expr).expect("inner unary data");
    assert_eq!(inner_unary.operator, SyntaxKind::PlusPlusToken as u16);
    let inner_operand = arena
        .get(inner_unary.operand)
        .expect("inner operand should exist");
    assert_eq!(
        inner_operand.kind,
        SyntaxKind::Identifier as u16,
        "inner ++ should target the identifier `y`"
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

#[test]
fn expr_call_type_arguments_allow_trailing_comma() {
    // `id<number,>("x")` should parse as a call expression with one type
    // argument and a trailing comma marker, not as a relational expression.
    let (parser, root) = parse_source("const x = id<number,>(\"x\");");
    assert_no_errors(&parser, "call type arguments with trailing comma");

    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(node.kind, syntax_kind_ext::CALL_EXPRESSION);

    let call = arena.get_call_expr(node).expect("call data");
    let type_args = call.type_arguments.as_ref().expect("type arguments");
    assert_eq!(type_args.nodes.len(), 1, "expected one type argument");
    assert!(type_args.has_trailing_comma, "expected trailing comma");
}

#[test]
fn expr_call_type_arguments_recover_missing_leading_argument() {
    // `id<,>("x")` is malformed, but recovery should keep the call expression
    // intact so later stages can continue from the argument list.
    let source = "const x = id<,>(\"x\");";
    let (parser, root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let comma_pos = source.find(',').expect("comma position") as u32;
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == diagnostic_codes::TYPE_EXPECTED && diag.start == comma_pos),
        "expected TS1110 at the missing type argument comma, got {diagnostics:?}"
    );

    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(node.kind, syntax_kind_ext::CALL_EXPRESSION);
}

#[test]
fn expr_new_type_arguments_allow_trailing_comma() {
    let (parser, root) = parse_source("const x = new Box<string,>();");
    assert_no_errors(&parser, "new expression type arguments with trailing comma");

    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(node.kind, syntax_kind_ext::NEW_EXPRESSION);

    let new_expr = arena.get_call_expr(node).expect("new expression data");
    let type_args = new_expr.type_arguments.as_ref().expect("type arguments");
    assert_eq!(type_args.nodes.len(), 1, "expected one type argument");
    assert!(type_args.has_trailing_comma, "expected trailing comma");
}

#[test]
fn expr_tagged_template_type_arguments_recover_missing_leading_argument() {
    let source = "const x = tag<,>`value`;";
    let (parser, root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let comma_pos = source.find(',').expect("comma position") as u32;
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == diagnostic_codes::TYPE_EXPECTED && diag.start == comma_pos),
        "expected TS1110 at the missing type argument comma, got {diagnostics:?}"
    );

    let arena = parser.get_arena();
    let init = get_var_initializer(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(node.kind, syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION);
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
fn accessor_children_include_body_once() {
    let source = "class C { get x() { return 1; } }";
    let (parser, root) = parse_source(source);
    assert_no_errors(&parser, "class accessor");

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

