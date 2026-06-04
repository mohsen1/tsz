#[test]
fn class_abstract_method() {
    // `abstract class Foo { abstract bar(): void; }`
    let (parser, root) = parse_clean_source(
        "abstract class Foo { abstract bar(): void; }",
        "abstract method",
    );
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
    let (parser, root) = parse_clean_source(
        "class Foo { constructor(public x: number) {} }",
        "parameter property",
    );
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
    let (parser, root) =
        parse_clean_source("declare var dec: any; @dec class Foo {}", "class decorator");
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
    let (parser, root) = parse_clean_source(
        "declare var a: any; declare var b: any; @a @b class Foo {}",
        "multiple decorators",
    );
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
    let (parser, root) = parse_clean_source(
        "class Foo { [key: string]: number; }",
        "class index signature",
    );
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
    let (parser, root) = parse_clean_source(
        "class Foo { [Symbol.iterator]() {} }",
        "computed property name",
    );
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
