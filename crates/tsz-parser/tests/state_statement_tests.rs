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

include!("state_statement_tests_parts/part_00.rs");
include!("state_statement_tests_parts/part_01.rs");
