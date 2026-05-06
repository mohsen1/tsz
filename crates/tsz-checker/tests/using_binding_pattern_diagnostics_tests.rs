use crate::test_utils::check_source_codes;

#[test]
fn using_binding_pattern_suppresses_disposal_followup_diagnostic() {
    let codes = check_source_codes(
        r#"
using { x } = { x: {} };
"#,
    );

    assert!(
        !codes.contains(&2850),
        "checker should not add TS2850 after parser rejects a using binding pattern, got: {codes:?}"
    );
}
