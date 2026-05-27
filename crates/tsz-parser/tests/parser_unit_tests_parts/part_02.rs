#[test]
fn template_expression_parts_record_raw_token_text() {
    // Two-span template with invalid `\u` in head and invalid `\x` in tail.
    let source = "`\\u${0}mid${1}\\x`;";
    let (parser, root) = parse_source(source);
    let arena = parser.get_arena();
    let init = get_first_expression_statement_expr(arena, root);
    let node = arena.get(init).expect("init");
    assert_eq!(
        node.kind,
        syntax_kind_ext::TEMPLATE_EXPRESSION,
        "should parse as template expression",
    );
    let tpl = arena.get_template_expr(node).expect("template expr");

    let head = arena.get(tpl.head).expect("head node");
    let head_lit = arena.get_literal(head).expect("head literal");
    assert_eq!(head_lit.raw_text.as_deref(), Some("`\\u${"));

    let span_nodes = &tpl.template_spans.nodes;
    assert_eq!(span_nodes.len(), 2, "two substitution spans expected");

    let middle_span = arena
        .get_template_span(arena.get(span_nodes[0]).expect("span0"))
        .expect("span0 data");
    let middle = arena.get(middle_span.literal).expect("middle node");
    let middle_lit = arena.get_literal(middle).expect("middle literal");
    assert_eq!(middle.kind, SyntaxKind::TemplateMiddle as u16);
    assert_eq!(middle_lit.raw_text.as_deref(), Some("}mid${"));

    let tail_span = arena
        .get_template_span(arena.get(span_nodes[1]).expect("span1"))
        .expect("span1 data");
    let tail = arena.get(tail_span.literal).expect("tail node");
    let tail_lit = arena.get_literal(tail).expect("tail literal");
    assert_eq!(tail.kind, SyntaxKind::TemplateTail as u16);
    assert_eq!(tail_lit.raw_text.as_deref(), Some("}\\x`"));
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

#[test]
fn decl_using_and_await_using_in_blocks_are_variable_statements() {
    for (source, expected_flags, context) in [
        (
            "function f() { using x = getResource(); }",
            node_flags::USING,
            "using declaration in function block",
        ),
        (
            "async function f() { await using x = getResource(); }",
            node_flags::AWAIT_USING,
            "await using declaration in async function block",
        ),
    ] {
        let (parser, root) = parse_source(source);
        assert_no_errors(&parser, context);

        let arena = parser.get_arena();
        let function_node = arena
            .get(get_first_statement(arena, root))
            .expect("function declaration");
        let function = arena.get_function(function_node).expect("function data");
        let body_node = arena.get(function.body).expect("function body");
        let body = arena.get_block(body_node).expect("block data");
        let stmt = arena.get(body.statements.nodes[0]).expect("body statement");

        assert_eq!(
            stmt.kind,
            syntax_kind_ext::VARIABLE_STATEMENT,
            "{context} should parse as a variable statement"
        );
        let variable = arena.get_variable(stmt).expect("variable statement data");
        let declaration_list = arena
            .get(variable.declarations.nodes[0])
            .expect("declaration list");
        assert_eq!(
            declaration_list.flags as u32 & node_flags::AWAIT_USING,
            expected_flags,
            "{context} should preserve declaration flags"
        );
    }
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
fn asserts_type_predicate_in_setter_parameter_type() {
    // tsc parses `asserts <ident-on-same-line>` predicates in any type position
    // and emits TS1228 later when the predicate is in an invalid context.
    // Previously the parser only accepted them in return-type position, so this
    // setter parameter type produced a stray TS1005 (',' expected) instead.
    //
    // `declare class Wat { set p2(x: asserts this is string); }`
    let (parser, root) = parse_source("declare class Wat { set p2(x: asserts this is string); }");
    assert_no_errors(&parser, "asserts predicate in setter parameter type");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let class = arena.get_class(stmt_node).expect("class");
    let setter_idx = class.members.nodes[0];
    let setter = arena.get(setter_idx).expect("setter");
    assert_eq!(setter.kind, syntax_kind_ext::SET_ACCESSOR);
    let acc = arena.get_accessor(setter).expect("accessor data");
    let param_idx = acc.parameters.nodes[0];
    let param_node = arena.get(param_idx).expect("parameter");
    let param = arena.get_parameter(param_node).expect("parameter data");
    let type_node = arena.get(param.type_annotation).expect("type annotation");
    assert_eq!(
        type_node.kind,
        syntax_kind_ext::TYPE_PREDICATE,
        "setter parameter type should parse as TYPE_PREDICATE, not a stray identifier"
    );
    let predicate = arena
        .get_type_predicate(type_node)
        .expect("type predicate data");
    assert!(
        predicate.asserts_modifier,
        "predicate should carry the asserts modifier"
    );
}

#[test]
fn asserts_type_predicate_in_type_alias() {
    // `type T = asserts x is string;` — same family of bug. Even though this is
    // semantically invalid (the checker reports TS1228), the parser must not
    // emit a TS1005 here. Matches tsc's behaviour where asserts predicates are
    // recognised by `parseNonArrayType` regardless of context.
    let (parser, _root) = parse_source("type T = asserts x is string;");
    let parser_diags = parser.get_diagnostics();
    assert!(
        !parser_diags
            .iter()
            .any(|d| d.code == diagnostic_codes::EXPECTED),
        "asserts predicate in type alias must not produce TS1005, got: {:?}",
        parser_diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn asserts_identifier_with_line_break_is_type_reference() {
    // `asserts` followed by a line break is just an identifier in type position
    // (ASI). tsc's `nextTokenIsIdentifierOrKeywordOnSameLine` enforces this; the
    // tsz lookahead used to ignore the line break, which would have parsed
    // `asserts\n  bar` as an ill-formed predicate had we entered the branch.
    let (parser, root) = parse_source("type T = asserts\n;");
    assert_no_errors(&parser, "asserts as plain type reference");
    let arena = parser.get_arena();
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let alias = arena.get_type_alias(stmt_node).expect("type alias");
    let alias_type = arena.get(alias.type_node).expect("alias type");
    assert_eq!(
        alias_type.kind,
        syntax_kind_ext::TYPE_REFERENCE,
        "trailing newline should keep `asserts` as a TypeReference"
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

#[test]
fn private_identifier_optional_chain_continuations_report_ts18030() {
    let source = r"
class A {
    a?: A
    #b?: A;
    getA(): A {
        return new A();
    }
    constructor() {
        this?.#b;
        this?.a.#b;
        this?.getA().#b;
    }
}
";
    let (parser, _root) = parse_source(source);
    let diagnostics: Vec<_> = parser
        .get_diagnostics()
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::AN_OPTIONAL_CHAIN_CANNOT_CONTAIN_PRIVATE_IDENTIFIERS
        })
        .collect();

    let expected_starts = [
        source.find("this?.#b").expect("first optional access") + "this?.".len(),
        source.find("this?.a.#b").expect("property continuation") + "this?.a.".len(),
        source.find("this?.getA().#b").expect("call continuation") + "this?.getA().".len(),
    ]
    .map(|pos| pos as u32);
    let actual_starts: Vec<u32> = diagnostics.iter().map(|d| d.start).collect();

    assert_eq!(
        actual_starts,
        expected_starts,
        "optional chains containing private identifiers should report TS18030 at every private name; all diagnostics: {:?}",
        parser.get_diagnostics()
    );
}
