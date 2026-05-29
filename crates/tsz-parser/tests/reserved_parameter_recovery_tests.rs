use crate::parser::syntax_kind_ext;
use crate::parser::test_fixture::parse_source;
use tsz_common::diagnostics::diagnostic_codes;

#[test]
fn hard_reserved_parameter_names_yield_statement_tail_recovery() {
    let source = "function f1(enum) {}\nfunction f2(class) {}\nfunction f3(function) {}\nfunction f4(while) {}\nfunction f5(for) {}";
    let (parser, root) = parse_source(source);
    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();

    assert!(
        codes.contains(&diagnostic_codes::IS_NOT_ALLOWED_AS_A_PARAMETER_NAME),
        "expected TS1390 for hard reserved parameter names, got {codes:?}"
    );

    let arena = parser.get_arena();
    let source_file = arena.get_source_file_at(root).unwrap();
    let kinds: Vec<u16> = source_file
        .statements
        .nodes
        .iter()
        .map(|&stmt| arena.get(stmt).unwrap().kind)
        .collect();
    assert_eq!(
        kinds,
        vec![
            syntax_kind_ext::FUNCTION_DECLARATION,
            syntax_kind_ext::ENUM_DECLARATION,
            syntax_kind_ext::BLOCK,
            syntax_kind_ext::FUNCTION_DECLARATION,
            syntax_kind_ext::CLASS_DECLARATION,
            syntax_kind_ext::BLOCK,
            syntax_kind_ext::FUNCTION_DECLARATION,
            syntax_kind_ext::FUNCTION_DECLARATION,
            syntax_kind_ext::BLOCK,
            syntax_kind_ext::FUNCTION_DECLARATION,
            syntax_kind_ext::WHILE_STATEMENT,
            syntax_kind_ext::FUNCTION_DECLARATION,
            syntax_kind_ext::FOR_STATEMENT,
        ],
        "reserved parameter keyword tails should recover as statements"
    );
}
