//! Tests for parser improvements to reduce TS1005 and TS2300 false positives — class member recovery.

use crate::parser::test_fixture::parse_source;
use tsz_common::diagnostics::diagnostic_codes;

#[test]
fn test_class_method_string_names_use_string_literal_nodes() {
    let source = r#"
class C {
    "foo"();
    "bar"() { }
}
"#;
    let (parser, root) = parse_source(source);
    let source_file = parser.get_arena().get_source_file_at(root).unwrap();
    let class_idx = source_file.statements.nodes[0];
    let class_node = parser.get_arena().get(class_idx).unwrap();
    let class_data = parser.get_arena().get_class(class_node).unwrap();
    let kinds: Vec<_> = class_data
        .members
        .nodes
        .iter()
        .filter_map(|&member_idx| {
            let member_node = parser.get_arena().get(member_idx)?;
            (member_node.kind == crate::parser::syntax_kind_ext::METHOD_DECLARATION).then_some({
                let method = parser.get_arena().get_method_decl(member_node)?;
                let name_node = parser.get_arena().get(method.name)?;
                (
                    method.name,
                    name_node.kind,
                    parser
                        .get_arena()
                        .get_literal(name_node)
                        .map(|lit| lit.text.clone()),
                )
            })
        })
        .collect();

    assert_eq!(kinds.len(), 2);
    for (_name_idx, kind, text) in kinds {
        assert_eq!(
            kind,
            tsz_scanner::SyntaxKind::StringLiteral as u16,
            "expected string literal name node"
        );
        assert!(text.is_some());
    }
}

#[test]
fn test_computed_property_signature_after_array_type_line_break_does_not_emit_ts1131() {
    let source = r"
const IGNORE_LIST = 'ignoreList';

interface SourceMap {
  sources: string[]
  [IGNORE_LIST]: number[]
}
";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::PROPERTY_OR_SIGNATURE_EXPECTED),
        "A line-broken computed property signature should not be parsed as indexed access: {diagnostics:?}"
    );
}

#[test]
fn test_class_computed_property_after_type_annotation_line_break_uses_asi() {
    let source = r"
class C {
    [e]: number
    [e2]: number
}
";
    let (parser, root) = parse_source(source);

    let source_file = parser.get_arena().get_source_file_at(root).unwrap();
    let class_idx = source_file.statements.nodes[0];
    let class_node = parser.get_arena().get(class_idx).unwrap();
    let class_data = parser.get_arena().get_class(class_node).unwrap();
    assert_eq!(
        class_data.members.nodes.len(),
        2,
        "line-broken class computed members should not become one indexed-access type"
    );
}

#[test]
fn test_class_computed_method_after_return_type_line_break_uses_asi() {
    let source = r#"
class C {
    ["foo"](): void
    ["bar"](): void;
    ["foo"]() {}
}
"#;
    let (parser, root) = parse_source(source);

    let codes: Vec<_> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED)
            && !codes.contains(&diagnostic_codes::OR_EXPECTED),
        "line-broken computed method signatures should remain separate members, got {codes:?}",
    );

    let source_file = parser.get_arena().get_source_file_at(root).unwrap();
    let class_idx = source_file.statements.nodes[0];
    let class_node = parser.get_arena().get(class_idx).unwrap();
    let class_data = parser.get_arena().get_class(class_node).unwrap();
    assert_eq!(
        class_data.members.nodes.len(),
        3,
        "computed method signatures should not become indexed-access return types"
    );
}

#[test]
fn test_module_like_class_member_recovers_as_outer_statement() {
    let source = r"
class C {
    global x
}
";
    let (parser, root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics.iter().any(|d| {
            d.code
                == diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED
        }),
        "Expected TS1068 for module-like class member, got diagnostics: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::EXPECTED && d.message == "';' expected."),
        "Expected TS1005 semicolon recovery for module-like class member, got diagnostics: {diagnostics:?}"
    );

    let arena = parser.get_arena();
    let source_file = arena.get_source_file_at(root).unwrap();
    let statements = &source_file.statements.nodes;
    assert!(
        statements.len() >= 3,
        "Recovered global declaration and following expression should survive outside the class: {statements:?}"
    );

    let class_node = arena.get(statements[0]).unwrap();
    assert_eq!(
        class_node.kind,
        crate::parser::syntax_kind_ext::CLASS_DECLARATION
    );
    let class_data = arena.get_class(class_node).unwrap();
    assert!(
        class_data.members.nodes.is_empty(),
        "Invalid module-like class member should terminate the class body"
    );

    let module_node = arena.get(statements[1]).unwrap();
    assert_eq!(
        module_node.kind,
        crate::parser::syntax_kind_ext::MODULE_DECLARATION
    );
    assert!(
        module_node.is_global_augmentation(),
        "Recovered `global` declaration should keep the global augmentation flag"
    );

    let expr_node = arena.get(statements[2]).unwrap();
    assert_eq!(
        expr_node.kind,
        crate::parser::syntax_kind_ext::EXPRESSION_STATEMENT
    );
}

#[test]
fn test_bare_hash_at_top_level_emits_ts1127() {
    // Bare `#` at top level should emit TS1127, not cascading errors
    let source = "# foo";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts1127_count = diagnostics.iter().filter(|d| d.code == 1127).count();
    assert!(
        ts1127_count >= 1,
        "Expected TS1127 for bare '#', got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_bare_hash_in_class_emits_ts1127() {
    // Bare `#` in class body should emit TS1127, not cascading errors
    let source = r"
class C {
    # name;
}
";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts1127_count = diagnostics.iter().filter(|d| d.code == 1127).count();
    assert!(
        ts1127_count >= 1,
        "Expected TS1127 for bare '#' in class body, got diagnostics: {diagnostics:?}"
    );
    // Should NOT cascade into TS1003/TS1005/TS1068/TS1128
    let cascade_count = diagnostics
        .iter()
        .filter(|d| matches!(d.code, 1003 | 1005 | 1068 | 1128))
        .count();
    assert_eq!(
        cascade_count, 0,
        "Bare '#' should not cascade into other errors, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_standalone_bare_hash_in_class_recovers_private_field_name() {
    let source = r"
class C {
    #

    m() {
        this.#
    }
}
";
    let (parser, root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics.iter().any(|d| d.code == 1127),
        "Expected TS1127 for recovered bare '#', got diagnostics: {diagnostics:?}"
    );

    let arena = parser.get_arena();
    let source_file = arena.get_source_file_at(root).unwrap();
    let class_idx = source_file.statements.nodes[0];
    let class_node = arena.get(class_idx).unwrap();
    let class_data = arena.get_class(class_node).unwrap();
    let member_node = arena.get(class_data.members.nodes[0]).unwrap();
    assert_eq!(
        member_node.kind,
        crate::parser::syntax_kind_ext::PROPERTY_DECLARATION,
        "standalone class-body '#' should recover as a property declaration"
    );
    let prop = arena.get_property_decl(member_node).unwrap();
    let name_node = arena.get(prop.name).unwrap();
    assert_eq!(
        name_node.kind,
        tsz_scanner::SyntaxKind::PrivateIdentifier as u16
    );
    let ident = arena.get_identifier(name_node).unwrap();
    assert_eq!(ident.escaped_text, "#");
}

#[test]
fn test_valid_private_name_no_ts1127() {
    // Valid private names should not emit TS1127
    let source = r"
class C {
    #name = 42;
    get #value() { return this.#name; }
}
";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts1127_count = diagnostics.iter().filter(|d| d.code == 1127).count();
    assert_eq!(
        ts1127_count, 0,
        "Valid private names should not emit TS1127, got diagnostics: {diagnostics:?}"
    );
}
