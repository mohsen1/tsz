//! Regression tests for #6888.
//!
//! Structural rule: when a `satisfies` source is an object literal and a
//! concrete source property value is incompatible with the matching target
//! property type, tsc elaborates the failure as TS2322 at the source property
//! instead of TS1360 at the `satisfies` expression.

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_source;

fn diagnostics(source: &str) -> Vec<Diagnostic> {
    check_source(source, "test.ts", CheckerOptions::default())
}

fn diagnostics_with_code(source: &str, code: u32) -> Vec<Diagnostic> {
    diagnostics(source)
        .into_iter()
        .filter(|diag| diag.code == code)
        .collect()
}

fn line_col(source: &str, start: u32) -> (usize, usize) {
    let start = start as usize;
    let line = source[..start]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1;
    let line_start = source[..start].rfind('\n').map_or(0, |idx| idx + 1);
    (line, start - line_start + 1)
}

#[test]
fn named_property_mismatch_reports_ts2322_at_property_name() {
    let source = r#"interface Config {
  host: string;
  port: number;
}

const wrongType = {
  host: "localhost",
  port: "3000"
} satisfies Config;
"#;

    let ts2322 = diagnostics_with_code(source, 2322);
    assert_eq!(ts2322.len(), 1, "expected one TS2322, got: {ts2322:?}");
    assert_eq!(
        ts2322[0].start as usize,
        source.rfind("port:").expect("fixture has port property"),
        "TS2322 must anchor at the mismatching property name"
    );
    assert_eq!(line_col(source, ts2322[0].start), (8, 3));
    assert!(
        ts2322[0].message_text.contains("'string'") && ts2322[0].message_text.contains("'number'"),
        "expected string-to-number message, got: {}",
        ts2322[0].message_text
    );
    assert!(
        diagnostics_with_code(source, 1360).is_empty(),
        "named property elaboration should suppress generic TS1360"
    );
}

#[test]
fn renamed_named_property_mismatch_uses_same_structural_path() {
    let source = r#"interface Flags {
  enabled: boolean;
  retries: number;
}

const wrong = {
  enabled: "yes",
  retries: 3
} satisfies Flags;
"#;

    let ts2322 = diagnostics_with_code(source, 2322);
    assert_eq!(ts2322.len(), 1, "expected one TS2322, got: {ts2322:?}");
    assert_eq!(
        ts2322[0].start as usize,
        source
            .rfind("enabled:")
            .expect("fixture has enabled property"),
        "renaming properties must not change the diagnostic path"
    );
    assert!(
        diagnostics_with_code(source, 1360).is_empty(),
        "renamed named property elaboration should suppress generic TS1360"
    );
}

#[test]
fn type_alias_target_named_property_mismatch_elaborates_at_property() {
    let source = r#"type Limits = {
  count: number;
};

const wrong = {
  count: "many"
} satisfies Limits;
"#;

    let ts2322 = diagnostics_with_code(source, 2322);
    assert_eq!(ts2322.len(), 1, "expected one TS2322, got: {ts2322:?}");
    assert_eq!(
        ts2322[0].start as usize,
        source.rfind("count:").expect("fixture has count property"),
        "alias targets with named properties should use the same elaboration"
    );
    assert!(
        diagnostics_with_code(source, 1360).is_empty(),
        "alias target elaboration should suppress generic TS1360"
    );
}

#[test]
fn bad_property_diagnostic_is_on_line_after_ts_expect_error_directive() {
    let source = r#"interface Config {
  host: string;
  port: number;
}

const wrongType = {
  host: "localhost",
  // @ts-expect-error
  port: "3000"
} satisfies Config;
"#;

    let ts2322 = diagnostics_with_code(source, 2322);
    assert_eq!(
        ts2322.len(),
        1,
        "expected raw checker TS2322, got: {ts2322:?}"
    );
    assert_eq!(
        line_col(source, ts2322[0].start),
        (9, 3),
        "raw checker diagnostic must land on the line covered by the directive"
    );
}

#[test]
fn compatible_named_properties_do_not_report_satisfies_diagnostics() {
    let source = r#"interface Config {
  host: string;
  port: number;
}

const ok = {
  host: "localhost",
  port: 3000
} satisfies Config;
"#;

    let codes: Vec<u32> = diagnostics(source)
        .into_iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        !codes.contains(&2322) && !codes.contains(&1360),
        "compatible object literal should not report TS2322/TS1360, got: {codes:?}"
    );
}

#[test]
fn primitive_satisfies_failure_still_reports_ts1360() {
    let source = "const wrong = 1 satisfies boolean;\n";
    let ts1360 = diagnostics_with_code(source, 1360);
    assert_eq!(
        ts1360.len(),
        1,
        "non-object satisfies failures should keep the generic TS1360 path"
    );
    assert!(
        diagnostics_with_code(source, 2322).is_empty(),
        "primitive satisfies failure should not be rewritten to TS2322"
    );
}
