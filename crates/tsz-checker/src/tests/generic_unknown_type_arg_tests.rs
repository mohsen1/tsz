use crate::test_utils::check_source_diagnostics;

#[test]
fn unknown_type_arg_constraint_uses_keyword_syntax_with_trivia() {
    let diagnostics = check_source_diagnostics(
        r#"
type Need<T extends string> = T;
type Bad = Need</* preserved trivia */ unknown>;
"#,
    );

    let matches = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == 2344)
        .collect::<Vec<_>>();
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one TS2344 for unknown type argument, got: {diagnostics:?}"
    );
    assert!(
        matches[0]
            .message_text
            .contains("Type 'unknown' does not satisfy the constraint 'string'."),
        "expected TS2344 to report the unknown keyword and string constraint, got: {:?}",
        matches[0]
    );
}
