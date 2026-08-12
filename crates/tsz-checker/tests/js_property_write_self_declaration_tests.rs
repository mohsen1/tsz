use crate::context::CheckerOptions;
use crate::diagnostics::diagnostic_codes;
use crate::test_utils::{check_js_source_diagnostics, check_multi_file};

fn ts2339_count(source: &str) -> usize {
    check_js_source_diagnostics(source)
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE)
        .count()
}

#[test]
fn checked_js_alias_property_write_does_not_declare_property() {
    let source = r#"
// @ts-check
const obj = {};
const alias = obj;

alias.fake = 1;
alias.fake;
"#;

    assert_eq!(
        ts2339_count(source),
        2,
        "alias property write and read should both report TS2339"
    );
}

#[test]
fn checked_js_typed_object_property_write_does_not_declare_property() {
    let source = r#"
// @ts-check
/** @type {{known: number}} */
const obj = { known: 1 };

obj.fake = 1;
obj.fake;
"#;

    assert_eq!(
        ts2339_count(source),
        2,
        "typed object property write and read should both report TS2339"
    );
}

#[test]
fn checked_js_class_instance_property_write_does_not_declare_property() {
    let source = r#"
// @ts-check
class Box {
  constructor() {
    this.known = 1;
  }
}

const box = new Box();
box.fake = 1;
box.fake;
"#;

    assert_eq!(
        ts2339_count(source),
        2,
        "class instance property write and read should both report TS2339"
    );
}

#[test]
fn checked_js_direct_empty_object_expando_still_allowed() {
    let source = r#"
// @ts-check
const obj = {};

obj.fake = 1;
obj.fake;
"#;

    assert_eq!(
        ts2339_count(source),
        0,
        "direct empty-object expando write should remain accepted"
    );
}

#[test]
fn checked_js_class_expression_host_expando_write_still_allowed() {
    let source = r#"
// @ts-check
var Widget = class {};
Widget.count = 2;
Widget.count;
"#;

    assert_eq!(
        ts2339_count(source),
        0,
        "a class-expression initializer hosts expando members regardless of the object-literal emptiness rule"
    );
}

fn cross_file_ts2339_count(host_source: &str) -> usize {
    check_multi_file(
        &[
            ("host.js", host_source),
            ("writer.js", "shared.extra = {};\nshared.extra;\n"),
        ],
        "writer.js",
        CheckerOptions {
            check_js: true,
            ..CheckerOptions::default()
        },
    )
    .iter()
    .filter(|diag| diag.code == diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE)
    .count()
}

#[test]
fn checked_js_cross_file_nonempty_object_literal_write_reports_ts2339() {
    // The cross-file suppression runs through the checker's
    // `root_symbol_supports_js_direct_expando_write` / `..._read` predicates
    // (the writing file's binder cannot resolve the root), so the emptiness
    // rule must hold there too: a non-empty-literal host in another file is a
    // closed shape and both the write and the read report TS2339.
    assert_eq!(
        cross_file_ts2339_count("var shared = { seeded: 1 };\n"),
        2,
        "a cross-file root declared with a non-empty object literal is not an expando host"
    );
}

#[test]
fn checked_js_cross_file_empty_object_literal_write_still_allowed() {
    assert_eq!(
        cross_file_ts2339_count("var shared = {};\n"),
        0,
        "a cross-file root declared with an empty object literal stays an expando host"
    );
}
