use tsz_checker::diagnostics::diagnostic_codes;
use tsz_checker::test_utils::check_source_diagnostics;

fn diagnostic_codes_for(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

#[test]
fn mapped_type_member_in_interface_reports_ts7061() {
    let codes = diagnostic_codes_for(
        r#"
interface EventHandlers {
  [K in `on${string}`]: () => void;
}
"#,
    );

    assert!(
        codes.contains(&diagnostic_codes::A_MAPPED_TYPE_MAY_NOT_DECLARE_PROPERTIES_OR_METHODS),
        "expected TS7061 for mapped type member in interface, got {codes:?}",
    );
}

#[test]
fn mapped_type_alias_remains_valid() {
    let codes = diagnostic_codes_for(
        r#"
type EventHandlers = {
  [K in `on${string}`]: () => void;
};
"#,
    );

    assert!(
        !codes.contains(&diagnostic_codes::A_MAPPED_TYPE_MAY_NOT_DECLARE_PROPERTIES_OR_METHODS),
        "mapped type aliases should remain valid, got {codes:?}",
    );
}
