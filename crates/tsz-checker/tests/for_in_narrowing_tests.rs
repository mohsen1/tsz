use crate::test_utils::check_source_strict_codes as check_strict;

#[test]
fn for_in_loop_body_preserves_outer_truthy_narrowing_through_initializer_assignment() {
    let source = r#"
const o: { [key: string]: string } | undefined = {};
if (o) {
    for (const key in o) {
        const value = o[key];
        value;
    }
}
"#;

    let codes = check_strict(source);
    assert!(
        !codes.contains(&18048),
        "Expected no TS18048 inside for-in body after outer truthy narrowing, got codes: {codes:?}"
    );
}
