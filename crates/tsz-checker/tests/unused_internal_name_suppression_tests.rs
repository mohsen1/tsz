use tsz_checker::test_utils::check_source_no_unused_locals;

#[test]
fn user_export_helper_name_reports_ts6133() {
    let diagnostics = check_source_no_unused_locals(
        r#"
export {};

const __export = 1;
"#,
    );

    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.code == 6133 && diagnostic.message_text.contains("__export")
        }),
        "Expected TS6133 for unused user binding named `__export`, got: {diagnostics:?}"
    );
}
