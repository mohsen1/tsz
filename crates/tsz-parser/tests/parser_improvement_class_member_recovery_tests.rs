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

// =====================================================================
// Misplaced `case`/`default` switch-clause keyword in a class body.
//
// When a class member begins with `case`/`default` followed by a property
// name on the same line (a misplaced switch clause), tsc reports a single
// TS1068 ("A constructor, method, accessor, or property was expected."),
// consumes the keyword, and parses the rest as a normal class member that it
// KEEPS (it still emits — `case d = () => {...}` -> `this.d = () => {...}`).
// It does not, however, run the post-parse grammar checks on that recovered
// member, so the yield-outside-generator check (TS1163) does not fire. tsz
// emits TS1163 eagerly in the parser, so it suppresses it for the recovered
// member while still parsing (and thus emitting) the member.
// =====================================================================

fn diagnostic_codes_of(source: &str) -> Vec<u32> {
    let (parser, _root) = parse_source(source);
    let mut codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    codes.sort_unstable();
    codes
}

/// Number of class members in the first top-level class declaration.
fn first_class_member_count(source: &str) -> usize {
    let (parser, root) = parse_source(source);
    let arena = parser.get_arena();
    let source_file = arena.get_source_file_at(root).unwrap();
    let class_idx = source_file.statements.nodes[0];
    let class_node = arena.get(class_idx).unwrap();
    arena.get_class(class_node).unwrap().members.nodes.len()
}

#[test]
fn misplaced_case_clause_with_yield_arrow_reports_only_ts1068() {
    // The witness from `constructorWithIncompleteTypeAnnotation.ts`: a
    // misplaced `case` clause whose initializer is a non-generator arrow with a
    // `yield` expression. tsc reports only TS1068 — the recovered member is
    // kept but its yield is not grammar-checked (no TS1163).
    let source = "class Widget {\n     case  handler = () => {  yield  0; };\n    render() { return 0; }\n}\n";
    let codes = diagnostic_codes_of(source);
    assert_eq!(
        codes,
        vec![diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED],
        "misplaced `case` clause must report only TS1068, never the recovered \
         member's TS1163; got {:?}",
        diagnostic_codes_of(source)
    );
}

#[test]
fn misplaced_case_clause_keeps_recovered_member_for_emit() {
    // tsc keeps the recovered `handler` member (it emits `this.handler = ...`),
    // so tsz must not drop it. The class retains both the recovered member and
    // the following `render` method.
    let source = "class Widget {\n     case  handler = () => {  yield  0; };\n    render() { return 0; }\n}\n";
    assert_eq!(
        first_class_member_count(source),
        2,
        "the recovered `case` member must be kept alongside the following method"
    );
}

#[test]
fn misplaced_case_clause_with_object_literal_reports_only_ts1068() {
    // A balanced object/array literal initializer parses as part of the kept
    // member; only the leading TS1068 is reported.
    let source = "class Registry {\n  case  entry = { a: 1, b: [2, 3] };\n  size() {}\n}\n";
    assert_eq!(
        diagnostic_codes_of(source),
        vec![diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED],
    );
}

#[test]
fn misplaced_case_clause_before_class_close_reports_only_ts1068() {
    // The recovered member ends via ASI at the class close `}`; no cascading
    // errors leak past the class body.
    let source = "class Node_ {\n  case  next = 1 }\n";
    assert_eq!(
        diagnostic_codes_of(source),
        vec![diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED],
    );
}

#[test]
fn misplaced_default_clause_with_yield_arrow_reports_only_ts1068() {
    // `default` shares the misplaced-switch-clause recovery with `case`.
    let source = "class Surface {\n  default  paint = () => { yield 1; };\n  area() {}\n}\n";
    assert_eq!(
        diagnostic_codes_of(source),
        vec![diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED],
    );
}

#[test]
fn case_as_property_name_followed_by_semicolon_is_still_valid() {
    // Negative control: `case` is a legal property name by itself (followed by
    // `;`/`(`), so it must NOT trigger the misplaced-clause recovery. A bare
    // `case;` property parses without any TS1068.
    let source = "class Bag {\n  case;\n  value() {}\n}\n";
    let codes = diagnostic_codes_of(source);
    assert!(
        !codes.contains(
            &diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED
        ),
        "`case;` as a property name must not emit TS1068; got {codes:?}"
    );
}

#[test]
fn non_generator_arrow_yield_still_reports_ts1163_when_attached() {
    // Negative control: when the arrow is a normal (non-recovered) member
    // initializer, the yield-outside-generator grammar check still fires. The
    // suppression flag must be scoped to the recovered member only, not leak to
    // sibling members.
    let source = "class Live {\n  handler = () => {  yield  0; };\n  run() {}\n}\n";
    let codes = diagnostic_codes_of(source);
    assert!(
        codes.contains(&diagnostic_codes::A_YIELD_EXPRESSION_IS_ONLY_ALLOWED_IN_A_GENERATOR_BODY),
        "a genuine non-generator arrow `yield` must still report TS1163; got {codes:?}"
    );
}

#[test]
fn yield_suppression_does_not_leak_to_sibling_member_after_case() {
    // The recovered `case` member suppresses its own TS1163, but the *following*
    // genuine member's non-generator `yield` must still report TS1163 — the flag
    // is reset per member.
    let source = "class Mixed {\n  case  a = () => { yield 0; };\n  b = () => { yield 1; };\n}\n";
    let codes = diagnostic_codes_of(source);
    assert!(
        codes.contains(&diagnostic_codes::A_YIELD_EXPRESSION_IS_ONLY_ALLOWED_IN_A_GENERATOR_BODY),
        "the sibling member's yield must still report TS1163; got {codes:?}"
    );
    // Exactly one TS1163 (from the sibling `b`, not the recovered `a`).
    let ts1163 = codes
        .iter()
        .filter(|&&c| c == diagnostic_codes::A_YIELD_EXPRESSION_IS_ONLY_ALLOWED_IN_A_GENERATOR_BODY)
        .count();
    assert_eq!(
        ts1163, 1,
        "only the sibling member should report TS1163; got {codes:?}"
    );
}

/// Recovery-record name for the first class declaration in `source`, as the
/// emitters would query it: by the class node's span.
fn class_var_fn_recovery_name(source: &str) -> Option<String> {
    let (parser, root) = parse_source(source);
    let source_file = parser.get_arena().get_source_file_at(root).unwrap();
    let class_idx = source_file.statements.nodes[0];
    let class_node = parser.get_arena().get(class_idx).unwrap();
    parser
        .get_arena()
        .class_body_var_fn_recovery_name_in_span(class_node.pos, class_node.end)
        .map(str::to_string)
}

#[test]
fn class_body_var_fn_recovery_records_dropped_member() {
    let source = "class C {\n    var constructor() { }\n}";
    let (parser, root) = parse_source(source);

    let source_file = parser.get_arena().get_source_file_at(root).unwrap();
    let class_idx = source_file.statements.nodes[0];
    let class_node = parser.get_arena().get(class_idx).unwrap();
    assert_eq!(
        parser
            .get_arena()
            .class_body_var_fn_recovery_name_in_span(class_node.pos, class_node.end),
        Some("constructor"),
        "dropped `var constructor() {{ }}` member should be recorded for emit"
    );

    // The recovery record must not change the diagnostic cascade.
    let diags = parser.get_diagnostics();
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert_eq!(
        codes,
        vec![
            diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED,
            diagnostic_codes::EXPECTED,
            diagnostic_codes::EXPECTED,
            diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
        ],
        "diagnostics must be unchanged by recovery recording, got {diags:?}"
    );
}

#[test]
fn class_body_var_fn_recovery_records_renamed_binder() {
    assert_eq!(
        class_var_fn_recovery_name("class Box {\n    var boxName_7() { }\n}"),
        Some("boxName_7".to_string())
    );
}

#[test]
fn class_body_var_fn_recovery_requires_var_keyword() {
    assert_eq!(
        class_var_fn_recovery_name("class C {\n    let constructor() { }\n}"),
        None,
        "`let` member recovery must not record the var-function shape"
    );
}

#[test]
fn class_body_var_fn_recovery_requires_empty_parameter_list() {
    assert_eq!(
        class_var_fn_recovery_name("class C {\n    var fn(a) { }\n}"),
        None
    );
}

#[test]
fn class_body_var_fn_recovery_requires_empty_body() {
    assert_eq!(
        class_var_fn_recovery_name("class C {\n    var fn() { run(); }\n}"),
        None
    );
}

#[test]
fn class_body_var_fn_recovery_rejects_return_type_annotation() {
    assert_eq!(
        class_var_fn_recovery_name("class C {\n    var fn(): void { }\n}"),
        None
    );
}

#[test]
fn class_body_var_fn_recovery_rejects_type_parameters() {
    assert_eq!(
        class_var_fn_recovery_name("class C {\n    var fn<T>() { }\n}"),
        None
    );
}

#[test]
fn class_body_var_fn_recovery_ignores_var_initializer_member() {
    assert_eq!(
        class_var_fn_recovery_name("class C {\n    var x = 1;\n}"),
        None,
        "`var x = 1` recovers as a property, not the var-function shape"
    );
}
