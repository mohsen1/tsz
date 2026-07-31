//! Regression tests for TS2565 ('used before being assigned') suppression on
//! JSDoc-typed prototype property declarations.
//!
//! `function C() {}; /** @type {T} */ C.prototype.x;` is a function-as-
//! constructor pattern where the bare prototype reference declares the
//! property's type via JSDoc. tsc treats this as a declaration, not an
//! "used before assigned" read.
//!
//! For ES `class C {}` declarations the same prototype attachment does NOT
//! declare anything: `C.prototype` is the class instance type, which is closed,
//! so the member simply does not exist. tsc reports **TS2339**, not TS2565.
//!
//! That second rule was previously asserted here as TS2565, which describes a
//! diagnostic `tsc` never emits for this shape. See #16049 for the oracle
//! matrix (`tsc` 7.0.2, `--strict --allowJs --checkJs`) and for the underlying
//! tsz defect: the TS path already reports TS2339 for `K.prototype.late`, and
//! only the checked-JS path stays silent.

use crate::context::CheckerOptions;
use crate::test_utils::check_source;

fn diag_codes_js(source: &str) -> Vec<u32> {
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        ..CheckerOptions::default()
    };
    check_source(source, "test.js", options)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn diagnostics_js(source: &str) -> Vec<tsz_common::diagnostics::Diagnostic> {
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        ..CheckerOptions::default()
    };
    check_source(source, "test.js", options)
}

#[test]
fn ts2565_suppressed_for_jsdoc_typed_prototype_on_function_constructor() {
    let codes = diag_codes_js(
        r#"
function C() { this.x = false; };
/** @type {number} */
C.prototype.x;
new C().x;
"#,
    );
    assert!(
        !codes.contains(&2565),
        "TS2565 must NOT fire for JSDoc-typed prototype on function-as-constructor. Got: {codes:?}"
    );
}

/// A JSDoc `@type` statement does not open an ES `class`'s prototype: the
/// instance type is closed, so the member does not exist and the read is a
/// plain TS2339. Oracle (`tsc` 7.0.2 `--strict --allowJs --checkJs`):
/// `error TS2339: Property 'late' does not exist on type 'K'.`
///
/// The suppression that makes this silent is checked-JS-only — the identical
/// program in a `.ts` file already reports TS2339 today. See #16049.
#[test]
fn ts2565_still_fires_for_jsdoc_typed_prototype_on_class() {
    let codes = diag_codes_js(
        r#"
class K {
    method() {}
}
/** @type {(x: number) => void} */
K.prototype.late;
"#,
    );
    assert!(
        codes.contains(&2339),
        "an ES `class` prototype is closed, so a JSDoc-typed late attachment must report TS2339 (tsc 7.0.2 does), not be silently accepted. Got: {codes:?}"
    );
    assert!(
        !codes.contains(&2565),
        "tsc reports TS2339 here, never TS2565 — the prototype member is absent, not read before assignment. Got: {codes:?}"
    );
}

/// The suppression is keyed on the `.prototype` receiver being a class, not on
/// the presence of a JSDoc comment: the identical read with no annotation at
/// all must also report TS2339. Oracle: `tsc` 7.0.2 agrees.
#[test]
fn class_prototype_read_with_no_jsdoc_reports_ts2339() {
    let codes = diag_codes_js(
        r#"
class K {
    method() {}
}
K.prototype.late;
"#,
    );
    assert!(
        codes.contains(&2339),
        "a class-prototype read of an absent member must report TS2339 with or without a JSDoc comment. Got: {codes:?}"
    );
}

/// Contrast that localises the family: `new K().late` and `K.prototype.late`
/// share the same receiver type `K`, so both must report TS2339 identically —
/// the defect was confined to the `.prototype` path, not the receiver type.
#[test]
fn new_expression_read_of_absent_member_still_reports_ts2339() {
    let codes = diag_codes_js(
        r#"
class K {
    method() {}
}
new K().late;
"#,
    );
    assert!(
        codes.contains(&2339),
        "new K().late must keep reporting TS2339 exactly like K.prototype.late. Got: {codes:?}"
    );
}

/// The write form (`K.prototype.late = 1`) is silenced by a different
/// mechanism than the read form: the binder's expando-property tracking
/// unconditionally treated any `ClassName.prototype.member` write as
/// expando-capable, the same way it legitimately does for a class's own
/// static side (`Base.newProp = 2`). Oracle: `tsc` 7.0.2 reports TS2339 here
/// too.
#[test]
fn class_prototype_write_of_absent_member_reports_ts2339() {
    let codes = diag_codes_js(
        r#"
class K {
    method() {}
}
K.prototype.late = 1;
"#,
    );
    assert!(
        codes.contains(&2339),
        "writing an absent member through a class prototype must report TS2339, not be accepted as an expando. Got: {codes:?}"
    );
}

/// Anti-hardcoding cover for the class-prototype rule: structural (class vs
/// function), not keyed on identifier names.
#[test]
fn class_prototype_rule_works_with_renamed_binders() {
    let codes = diag_codes_js(
        r#"
class Widget {
    render() {}
}
/** @type {() => void} */
Widget.prototype.unrelated;
"#,
    );
    assert!(
        codes.contains(&2339),
        "the class-prototype TS2339 rule must hold for any class/member name pair. Got: {codes:?}"
    );
}

/// Positive control, oracle-confirmed clean: reading a member the class really
/// declares through `.prototype` must stay silent. This is the case the TS2339
/// rule above must not over-fire on.
#[test]
fn class_prototype_read_of_declared_member_is_clean() {
    let codes = diag_codes_js(
        r#"
class K {
    method() {}
}
K.prototype.method;
"#,
    );
    assert!(
        !codes.contains(&2339),
        "a member the class declares is present on its prototype. Got: {codes:?}"
    );
}

/// Anti-hardcoding cover: the rule is structural (function vs class), not
/// based on identifier names — works with arbitrary names.
#[test]
fn ts2565_suppression_works_with_renamed_constructor() {
    let codes = diag_codes_js(
        r#"
function MyThing() { this.value = 0; };
/** @type {string} */
MyThing.prototype.label;
"#,
    );
    assert!(
        !codes.contains(&2565),
        "TS2565 suppression must work for any function-constructor name. Got: {codes:?}"
    );
}

#[test]
fn jsdoc_typed_prototype_checks_constructor_this_assignment() {
    let source =
        "function C() { this.x = false; };\n/** @type {number} */\nC.prototype.x;\nnew C().x;\n";
    let diagnostics = diagnostics_js(source);
    let ts2322: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected one TS2322 for constructor `this.x` conflicting with prototype JSDoc; got: {diagnostics:?}"
    );
    assert_eq!(
        ts2322[0].message_text,
        "Type 'boolean' is not assignable to type 'number'."
    );
    assert_eq!(
        ts2322[0].start as usize,
        source
            .find("this.x")
            .expect("test source should contain this.x"),
        "TS2322 should be anchored on the constructor assignment target"
    );
    assert!(
        !diagnostics.iter().any(|d| d.code == 2565),
        "the JSDoc prototype declaration should still suppress TS2565; got: {diagnostics:?}"
    );
}

#[test]
fn matching_jsdoc_typed_prototype_constructor_assignment_has_no_ts2322() {
    let codes = diag_codes_js(
        r#"
function C() { this.x = 1; };
/** @type {number} */
C.prototype.x;
"#,
    );
    assert!(
        !codes.contains(&2322),
        "matching constructor assignment and prototype JSDoc type must not emit TS2322; got: {codes:?}"
    );
}
