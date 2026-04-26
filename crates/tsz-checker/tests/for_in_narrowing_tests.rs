use crate::context::CheckerOptions;

fn check_strict(source: &str) -> Vec<u32> {
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        no_implicit_any: true,
        ..Default::default()
    };
    crate::test_utils::check_source(source, "test.ts", options)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

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
