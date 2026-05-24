use tsz_checker::test_utils::check_source_code_messages;

#[test]
fn protected_intersection_owner_display_is_structural_for_renamed_classes() {
    let diagnostics = check_source_code_messages(
        r#"
class Alpha {
    protected slot: string = "";
}
class Beta {
    protected slot: string = "";
}
declare var value: Alpha & Beta;
value.slot;
"#,
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2445
                && message.contains("Property 'slot' is protected")
                && message.contains("Alpha & Beta")
        }),
        "expected TS2445 against the renamed protected intersection owner, got: {diagnostics:?}"
    );
    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2339),
        "renamed protected intersection member should not fall back to missing property, got: {diagnostics:?}"
    );
}
