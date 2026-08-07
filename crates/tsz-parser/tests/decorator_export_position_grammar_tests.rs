//! `@dec export @dec class` / `@dec export default @dec class`: TS8038
//! (`Decorators may not appear after 'export' or 'export default' if they
//! also appear before 'export'.`) with its TS1486 (`Decorator used before
//! 'export' here.`) related-info pointer.
//!
//! `tsc` reports TS8038 exactly once per declaration, anchored at the
//! *first* trailing decorator, regardless of how many leading or trailing
//! decorators are present — extra decorators on either side do not multiply
//! the report. The related-info pointer always names the *first* leading
//! decorator. Oracle-verified against `typescript@7.0.2`.

use crate::parser::test_fixture::parse_source;

fn parse_code(code: &str) -> Vec<crate::parser::ParseDiagnostic> {
    let (parser, _root) = parse_source(code);
    parser.get_diagnostics().to_vec()
}

fn ts8038_diagnostics(code: &str) -> Vec<crate::parser::ParseDiagnostic> {
    parse_code(code)
        .into_iter()
        .filter(|d| d.code == 8038)
        .collect()
}

/// A single leading + trailing decorator: one TS8038 at the trailing
/// decorator, with a TS1486 related pointer at the leading decorator.
#[test]
fn single_leading_and_trailing_decorator_reports_once_with_related_info() {
    let src = "@dec export @dec class M {}\n";
    let diags = ts8038_diagnostics(src);
    assert_eq!(diags.len(), 1, "expected exactly one TS8038, got {diags:?}");
    let diag = &diags[0];
    // Anchored at the trailing decorator's `@dec` (after `export `).
    let trailing_pos = src.find("export @dec").unwrap() as u32 + "export ".len() as u32;
    assert_eq!(diag.start, trailing_pos);
    assert_eq!(diag.length, 4); // "@dec"
    let related = diag
        .related
        .as_ref()
        .expect("TS8038 must carry a TS1486 related-info pointer");
    assert_eq!(related.code, 1486);
    assert_eq!(related.message, "Decorator used before 'export' here.");
    // Anchored at the leading decorator, position 0.
    assert_eq!(related.start, 0);
    assert_eq!(related.length, 4); // "@dec"
}

/// `export default` form: same single-report, related-pointer semantics.
#[test]
fn export_default_single_leading_and_trailing_decorator_reports_once() {
    let src = "@dec export default @dec class M {}\n";
    let diags = ts8038_diagnostics(src);
    assert_eq!(diags.len(), 1, "expected exactly one TS8038, got {diags:?}");
    let related = diags[0]
        .related
        .as_ref()
        .expect("TS8038 must carry a TS1486 related-info pointer");
    assert_eq!(related.code, 1486);
    assert_eq!(related.start, 0);
}

/// Multiple *trailing* decorators must not multiply the report: `tsc` still
/// emits exactly one TS8038, at the first trailing decorator.
#[test]
fn multiple_trailing_decorators_report_once() {
    let src = "@dec export @dec @dec class M {}\n";
    let diags = ts8038_diagnostics(src);
    assert_eq!(
        diags.len(),
        1,
        "multiple trailing decorators must not multiply TS8038, got {diags:?}"
    );
    let trailing_pos = src.find("export @dec").unwrap() as u32 + "export ".len() as u32;
    assert_eq!(diags[0].start, trailing_pos);
}

/// Multiple *leading* decorators: still one TS8038, related info at the
/// first leading decorator (position 0), not the second.
#[test]
fn multiple_leading_decorators_point_related_info_at_first() {
    let src = "@dec @dec export @dec class M {}\n";
    let diags = ts8038_diagnostics(src);
    assert_eq!(diags.len(), 1, "expected exactly one TS8038, got {diags:?}");
    let related = diags[0]
        .related
        .as_ref()
        .expect("TS8038 must carry a TS1486 related-info pointer");
    assert_eq!(related.start, 0);
}

/// `export default`, multiple trailing decorators: still one TS8038.
#[test]
fn export_default_multiple_trailing_decorators_report_once() {
    let src = "@dec export default @dec @dec class M {}\n";
    let diags = ts8038_diagnostics(src);
    assert_eq!(
        diags.len(),
        1,
        "multiple trailing decorators must not multiply TS8038, got {diags:?}"
    );
}

/// Negative control: a lone leading decorator with no trailing decorator
/// after `export` is legal and must not report TS8038 or TS1486.
#[test]
fn leading_decorator_only_is_clean() {
    let src = "@dec export class M {}\n";
    let diags = ts8038_diagnostics(src);
    assert!(diags.is_empty(), "expected no TS8038, got {diags:?}");
}

/// Negative control: a lone trailing decorator (`export @dec class`, no
/// leading decorator) is the ordinary native-decorator form and must not
/// report TS8038 or TS1486.
#[test]
fn trailing_decorator_only_is_clean() {
    let src = "export @dec class M {}\n";
    let diags = ts8038_diagnostics(src);
    assert!(diags.is_empty(), "expected no TS8038, got {diags:?}");
}

/// Renamed-binder adjacent case: the decorator's identifier text must not be
/// load-bearing.
#[test]
fn renamed_decorator_identifier_still_reports() {
    let src = "@myCustomDecorator export @myCustomDecorator class Widget {}\n";
    let diags = ts8038_diagnostics(src);
    assert_eq!(diags.len(), 1, "expected exactly one TS8038, got {diags:?}");
}
