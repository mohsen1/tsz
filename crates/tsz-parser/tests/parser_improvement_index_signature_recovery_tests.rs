//! Tests for parser improvements to reduce TS1005 and TS2300 false positives — index signature recovery.

use crate::parser::test_fixture::parse_source;
use tsz_common::diagnostics::diagnostic_codes;
use tsz_common::position::LineMap;

#[test]
fn test_index_signature_with_modifier_emits_ts1071() {
    // Index signature with public modifier should emit TS1071, not TS1184
    // TS1071: '{0}' modifier cannot appear on an index signature.
    // TS1184: Modifiers cannot appear here. (too generic)
    let source = r"
interface I {
  public [a: string]: number;
}
";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();

    // Should emit TS1071 for modifier on index signature
    let ts1071_count = diagnostics.iter().filter(|d| d.code == 1071).count();
    assert_eq!(
        ts1071_count, 1,
        "Expected 1 TS1071 error for modifier on index signature, got {ts1071_count}",
    );

    // Should NOT emit the generic TS1184
    let ts1184_count = diagnostics.iter().filter(|d| d.code == 1184).count();
    assert_eq!(
        ts1184_count, 0,
        "Expected no TS1184 errors (should be TS1071 instead), got {ts1184_count}",
    );
}

#[test]
fn index_signature_type_predicate_tail_defers_close_brace() {
    let source = "interface I2 {\n    [index: number]: p1 is C;\n}\n";
    let (parser, root) = parse_source(source);
    let line_map = LineMap::build(source);

    let fingerprints: Vec<(u32, u32, u32, String)> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| {
            let pos = line_map.offset_to_position(diag.start, source);
            (
                diag.code,
                pos.line + 1,
                pos.character + 1,
                diag.message.clone(),
            )
        })
        .collect();

    assert!(
        fingerprints.contains(&(
            diagnostic_codes::EXPECTED,
            2,
            25,
            "';' expected.".to_string()
        )),
        "expected TS1005 at the invalid `is` tail, got {fingerprints:?}"
    );
    assert!(
        fingerprints.contains(&(
            diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
            3,
            1,
            "Declaration or statement expected.".to_string()
        )),
        "expected TS1128 at the deferred interface close brace, got {fingerprints:?}"
    );

    let arena = parser.get_arena();
    let source_file = arena.get_source_file_at(root).unwrap();
    let statement_kinds: Vec<u16> = source_file
        .statements
        .nodes
        .iter()
        .map(|&stmt_idx| arena.get(stmt_idx).unwrap().kind)
        .collect();
    assert_eq!(
        statement_kinds,
        vec![
            crate::parser::syntax_kind_ext::INTERFACE_DECLARATION,
            crate::parser::syntax_kind_ext::EXPRESSION_STATEMENT,
            crate::parser::syntax_kind_ext::EXPRESSION_STATEMENT,
        ],
        "invalid index-signature type-predicate tails should recover as top-level statements"
    );
}

#[test]
fn test_empty_index_signature_after_type_member_annotation_line_break_uses_asi() {
    let source = r"
var v: {
   a: B
   [];
};
";
    let (parser, _root) = parse_source(source);

    let codes: Vec<_> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&diagnostic_codes::AN_INDEX_SIGNATURE_MUST_HAVE_EXACTLY_ONE_PARAMETER),
        "empty bracket member should recover as an index signature, got {codes:?}",
    );
}
