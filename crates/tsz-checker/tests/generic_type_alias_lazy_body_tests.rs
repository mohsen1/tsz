use tsz_checker::test_utils::check_source_diagnostics;

fn diagnostic_codes(source: &str) -> Vec<u32> {
    let mut codes: Vec<u32> = check_source_diagnostics(source)
        .into_iter()
        .map(|diag| diag.code)
        .collect();
    codes.sort_unstable();
    codes
}

#[test]
fn generic_alias_body_validation_still_reports_missing_reference() {
    for (alias, param) in [("Alias", "T"), ("Wrap", "Element")] {
        let source = format!("type {alias}<{param}> = Missing<{param}>;");
        let codes = diagnostic_codes(&source);
        assert!(
            codes.contains(&2304),
            "generic alias body validation must still report missing names for {alias}<{param}>: {codes:?}"
        );
    }
}

#[test]
fn generic_alias_body_validation_still_checks_explicit_type_arguments() {
    let codes = diagnostic_codes(
        r#"
type Box<T> = T;
type Alias<U> = Box<U, U>;
"#,
    );

    assert!(
        codes.contains(&2314) || codes.contains(&2315),
        "generic alias body validation must still check explicit type arguments: {codes:?}"
    );
}

#[test]
fn generic_alias_use_site_still_resolves_body_after_lazy_declaration_check() {
    for (alias, param) in [("Identity", "T"), ("Project", "Value")] {
        let source = format!(
            r#"
type {alias}<{param}> = {param};
let value: {alias}<string> = 1;
"#
        );
        let codes = diagnostic_codes(&source);
        assert!(
            codes.contains(&2322),
            "generic alias use sites must still resolve the alias body for {alias}<{param}>: {codes:?}"
        );
    }
}
