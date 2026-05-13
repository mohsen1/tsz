use tsz_checker::test_utils::check_source_code_messages;

#[test]
fn conflicting_generic_argument_reports_ts2345_without_type_display_panic() {
    let diagnostics = check_source_code_messages(
        r#"
function combine<T>(a: T, b: T): T[] {
  return [a, b];
}
const c: (string | number)[] = combine("a", 1);
"#,
    );

    assert!(
        diagnostics
            .iter()
            .any(|(code, message)| *code == 2345 && message.contains("Argument of type")),
        "expected TS2345 for the conflicting generic call without panicking, got: {diagnostics:#?}"
    );
}
