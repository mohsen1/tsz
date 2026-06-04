//! Tests for statement parsing in the parser.
use crate::parser::NodeIndex;
use crate::parser::node::NodeArena;
use crate::parser::node_view::NodeAccess;
use crate::parser::syntax_kind_ext;
use crate::parser::test_fixture::{parse_source, parse_source_with_language_version};
use tsz_common::ScriptTarget;
use tsz_common::diagnostics::diagnostic_codes;
use tsz_common::position::LineMap;

fn assert_function_body_recovery_uses_statement_errors(source: &str) {
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&diagnostic_codes::EXPECTED),
        "expected TS1005 for the missing `(`, got {diags:?}"
    );
    assert!(
        codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
        "expected downstream TS1109 from the malformed body statement, got {diags:?}"
    );
    assert!(
        codes.contains(&diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED),
        "expected TS1128 from `static` statement recovery, got {diags:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::PROPERTY_ASSIGNMENT_EXPECTED),
        "should not parse the function body as an object/parameter list, got {diags:?}"
    );
}

#[test]
fn catch_missing_block_dangling_question_is_not_a_following_statement() {
    let source = "for (var x in { x: 0 }) {\n    !\n    try { throw null; }\n    catch (Exception) ?\n}\nfinally { }\n";
    let (parser, root) = parse_source(source);
    let arena = parser.get_arena();
    let sf = arena.get_source_file_at(root).unwrap();
    let for_node = arena
        .get(sf.statements.nodes[0])
        .expect("expected recovered for statement");
    let for_data = arena.get_for_in_of(for_node).expect("expected for-in data");
    let body_node = arena.get(for_data.statement).expect("expected for body");
    let body = arena.get_block(body_node).expect("expected block body");

    assert_eq!(
        body.statements.nodes.len(),
        2,
        "dangling `?` after a missing catch block should be consumed by catch recovery"
    );
    assert_eq!(
        arena.get(body.statements.nodes[1]).unwrap().kind,
        syntax_kind_ext::TRY_STATEMENT
    );
}

include!("state_statement_tests_parts/part_00.rs");
include!("state_statement_tests_parts/part_01.rs");
