use crate::diagnostics::diagnostic_codes;
use crate::test_utils::check_js_source_diagnostics;

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
