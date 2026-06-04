#[test]
fn while_missing_open_paren_before_colon_recovers_rest_tail() {
    let source = "public Overloads( while : string, ...rest: string[]) {  &\npublic DefaultValue(value?: string = \"Hello\") { }\n";
    let fingerprints = diagnostic_fingerprints(source);

    assert!(
        fingerprints.contains(&(
            diagnostic_codes::ARGUMENT_EXPRESSION_EXPECTED,
            1,
            19,
            "Argument expression expected.".to_string()
        )),
        "expected argument recovery at `while`, got {fingerprints:?}"
    );
    assert!(
        fingerprints.contains(&(
            diagnostic_codes::EXPECTED,
            1,
            25,
            "'(' expected.".to_string()
        )),
        "expected missing `(` after `while`, got {fingerprints:?}"
    );
    assert!(
        fingerprints.contains(&(
            diagnostic_codes::EXPRESSION_EXPECTED,
            1,
            35,
            "Expression expected.".to_string()
        )),
        "expected TS1109 at rest spread in while tail, got {fingerprints:?}"
    );
    assert!(
        fingerprints.iter().any(|(code, _, _, message)| {
            *code == diagnostic_codes::AN_ELEMENT_ACCESS_EXPRESSION_SHOULD_TAKE_AN_ARGUMENT
                && message == "An element access expression should take an argument."
        }),
        "expected element-access recovery for `string[]`, got {fingerprints:?}"
    );
    assert!(
        !fingerprints
            .iter()
            .any(|(_, _, _, message)| message == "')' expected."),
        "while colon recovery should not report a spurious missing `)`, got {fingerprints:?}"
    );

    let (parser, root) = parse_source(source);
    let arena = parser.get_arena();
    let sf = arena.get_source_file_at(root).unwrap();
    let while_node = arena
        .get(sf.statements.nodes[1])
        .expect("expected recovered while statement");
    assert_eq!(while_node.kind, syntax_kind_ext::WHILE_STATEMENT);
    let while_data = arena.get_loop(while_node).expect("expected loop data");
    assert_eq!(
        while_data.condition,
        NodeIndex::NONE,
        "`while :` recovery should keep the condition missing"
    );
    assert_eq!(
        arena.get(while_data.statement).unwrap().kind,
        syntax_kind_ext::EXPRESSION_STATEMENT,
        "the leading colon tail should become the while body"
    );
    assert!(
        sf.statements.nodes.iter().skip(2).any(|&idx| arena
            .get(idx)
            .is_some_and(|node| node.kind == syntax_kind_ext::LABELED_STATEMENT)),
        "`rest: string[]` should survive as a following labeled statement"
    );
}

#[test]
fn class_missing_body_at_dot_reports_stray_outer_closes_without_eof_close() {
    let source = "namespace N {\n  class A .\n    public method1() { }\n  }\n}\nenum E { A }\n";
    let fingerprints = diagnostic_fingerprints(source);

    let stray_close_count = fingerprints
        .iter()
        .filter(|(code, _, _, message)| {
            *code == diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED
                && message == "Declaration or statement expected."
        })
        .count();
    assert!(
        stray_close_count >= 2,
        "expected recovered stray close-brace diagnostics, got {fingerprints:?}"
    );
    assert!(
        !fingerprints
            .iter()
            .any(|(_, _, _, message)| message == "'}' expected."),
        "missing class body recovery should not cascade to EOF `}} expected`, got {fingerprints:?}"
    );
}

#[test]
fn class_missing_body_at_dot_does_not_suppress_later_eof_close_brace() {
    let source = "namespace N {\n  class A .\n    public method1() { }\n  }\n}\nfunction f() {\n";
    let fingerprints = diagnostic_fingerprints(source);

    assert!(
        fingerprints.iter().any(|(code, _, _, message)| {
            *code == diagnostic_codes::EXPECTED && message == "'}' expected."
        }),
        "missing class body recovery should not hide a later function EOF close-brace error, got {fingerprints:?}"
    );
}

#[test]
fn nested_class_recovery_anchors_real_close_before_comments() {
    let source = "class C {\n  m() {}\n  /* comment } */\n  class D {}\n}\n";
    let member_close_pos = source.find("m() {}").expect("method") as u32 + "m() {".len() as u32;
    let comment_close_pos =
        source.find("comment }").expect("comment") as u32 + "comment ".len() as u32;
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();

    assert!(
        diags.iter().any(|diag| {
            diag.code == diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED
                && diag.start == member_close_pos
        }),
        "nested class recovery should anchor TS1128 to the previous real close brace, got {diags:?}"
    );
    assert!(
        !diags.iter().any(|diag| {
            diag.code == diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED
                && diag.start == comment_close_pos
        }),
        "nested class recovery should ignore close-brace text inside comments, got {diags:?}"
    );
}

#[test]
fn unicode_escape_unknown_variable_name_reports_only_invalid_character() {
    let source = "function f() {\n  var  _\\uD4A5\\u7204\\uC316\\uE59F  = local;\n}\n";
    let fingerprints = diagnostic_fingerprints(source);

    assert!(
        fingerprints
            .iter()
            .any(|(code, _, _, _)| *code == diagnostic_codes::INVALID_CHARACTER),
        "expected TS1127 for invalid escaped identifier, got {fingerprints:?}"
    );
    assert!(
        !fingerprints.iter().any(|(code, _, _, _)| *code == 1134),
        "invalid escaped identifier should not cascade to TS1134, got {fingerprints:?}"
    );
}

/// `interface <Name>.<Rest> { }` is a malformed dotted interface name. tsc
/// parses `interface <Name>` with an empty body, reports TS1005 `'{' expected.`
/// at the dot, and resumes statement parsing at `<Rest>` — recovering it as an
/// expression statement (TS1434 "Unexpected keyword or identifier") followed by
/// the trailing `{ }` block. The recovered statements must survive into the AST
/// so they are emitted, instead of being silently swallowed by the interface.
///
/// This check varies the chosen identifier names (`Foo.I1`, `Bar.Baz`) so the
/// rule is keyed on the dotted-name grammar shape, not on a particular spelling.
fn assert_dotted_interface_recovery(source: &str, dot_offset: u32, rest_offset: u32) {
    use syntax_kind_ext::{BLOCK, EXPRESSION_STATEMENT, INTERFACE_DECLARATION};

    let (parser, root) = parse_source(source);
    let diags = parser.get_diagnostics();

    const TS1005: u32 = diagnostic_codes::EXPECTED;
    const TS1434: u32 = diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER;

    // TS1005 `'{' expected.` reported at the dot that follows the interface name.
    assert!(
        diags
            .iter()
            .any(|d| d.code == TS1005 && d.start == dot_offset && d.message.contains("'{'")),
        "expected TS1005 `'{{' expected.` at the dot (offset {dot_offset}) for {source:?}, got {diags:?}"
    );
    // TS1434 reported at the segment after the dot, which re-enters statement
    // recovery as an expression statement.
    assert!(
        diags
            .iter()
            .any(|d| d.code == TS1434 && d.start == rest_offset),
        "expected TS1434 at the post-dot identifier (offset {rest_offset}) for {source:?}, got {diags:?}"
    );

    // The recovered statement list must contain an empty interface, the
    // expression statement for the trailing identifier, and the block.
    let sf = parser.get_arena().get_source_file_at(root).unwrap();
    let kinds: Vec<u16> = sf
        .statements
        .nodes
        .iter()
        .filter_map(|idx| parser.get_arena().get(*idx).map(|node| node.kind))
        .collect();
    assert!(
        kinds.contains(&INTERFACE_DECLARATION)
            && kinds.contains(&EXPRESSION_STATEMENT)
            && kinds.contains(&BLOCK),
        "dotted interface name should recover as interface + expression statement + block, got {kinds:?} for {source:?}"
    );
}

#[test]
fn parse_dotted_interface_name_recovers_trailing_statements() {
    // `interface Foo.I1 { }`: dot at offset 13, `I1` at offset 14.
    assert_dotted_interface_recovery("interface Foo.I1 { }\n", 13, 14);
    // Renamed shape proves the rule is structural, not spelling-specific:
    // `interface Bar.Baz { }`: dot at offset 13, `Baz` at offset 14.
    assert_dotted_interface_recovery("interface Bar.Baz { }\n", 13, 14);
}
