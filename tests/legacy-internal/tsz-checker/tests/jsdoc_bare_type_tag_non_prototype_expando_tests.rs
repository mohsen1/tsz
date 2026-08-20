//! A bare JSDoc `@type`-commented property-access *statement* declares a
//! member only for a function-as-constructor's expando prototype
//! (`function C(){} /** @type {T} */ C.prototype.x;` — see
//! `ts2565_jsdoc_prototype_type_decl_tests.rs`). For every other receiver —
//! a plain namespace/value object, a function's own static property, or an
//! ES class's closed prototype — tsc still reports `TS2339` for the read.
//!
//! `resolve_jsdoc_assigned_value_type_in_arena`'s bare-statement scan used to
//! gate only on excluding an ES class prototype (`expando_receiver_is_class`
//! applied one level too shallow, on the statement's own receiver rather than
//! on a `.prototype` link), so it never actually required the access be a
//! `.prototype` chain at all. Any `/** @type {T} */ ns.prop;` was treated as
//! a declaration, suppressing `TS2339` on the following real read too.
//! Verified against the pinned `typescript@7.0.2` oracle for every row below.

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

/// The original repro: a bare `@type` tag before a plain object's property
/// read must not suppress `TS2339`. Oracle: `tsc` 7.0.2 reports `TS2339` on
/// both the declaring statement and the later read.
#[test]
fn bare_type_tag_on_plain_namespace_property_still_reports_ts2339() {
    let codes = diag_codes_js(
        r#"
var exports = {};
/** @type {string} */
exports.SomeName;
"#,
    );
    assert!(
        codes.contains(&2339),
        "a bare @type tag on a plain object's property must not manufacture a \
         declaration; tsc still reports TS2339 here. Got: {codes:?}"
    );
}

/// Same shape, renamed binders, to keep the rule structural rather than
/// keyed on the `exports`/`SomeName` identifiers from the original repro.
#[test]
fn bare_type_tag_on_plain_namespace_property_works_with_renamed_binders() {
    let codes = diag_codes_js(
        r#"
var Registry = {};
/** @type {number} */
Registry.Count;
"#,
    );
    assert!(
        codes.contains(&2339),
        "the rule must hold under renamed binders, not just the original repro's \
         names. Got: {codes:?}"
    );
}

/// A function's own *static* property (no `.prototype` in the chain) is not
/// a constructor's expando prototype either — tsc reports TS2339.
#[test]
fn bare_type_tag_on_function_static_property_still_reports_ts2339() {
    let codes = diag_codes_js(
        r#"
function F() {}
/** @type {number} */
F.x;
"#,
    );
    assert!(
        codes.contains(&2339),
        "a bare @type tag on a function's own static property (not \
         `.prototype`) must not manufacture a declaration. Got: {codes:?}"
    );
}

/// Negative control keeping the real feature alive: a function-as-constructor
/// `.prototype` member declared via a bare `@type` statement must still be
/// silent, matching `ts2565_jsdoc_prototype_type_decl_tests`.
#[test]
fn bare_type_tag_on_function_constructor_prototype_stays_silent() {
    let codes = diag_codes_js(
        r#"
function C() {}
/** @type {number} */
C.prototype.x;
var y = C.prototype.x;
"#,
    );
    assert!(
        !codes.contains(&2339),
        "the real function-as-constructor prototype declaration must still be \
         recognized. Got: {codes:?}"
    );
}

/// Negative control: the same plain-namespace shape with no JSDoc comment at
/// all must already report TS2339 — this pins the baseline behavior the bug
/// only broke once a `@type` tag was added.
#[test]
fn plain_namespace_property_without_jsdoc_reports_ts2339() {
    let codes = diag_codes_js(
        r#"
var exports = {};
exports.SomeName;
"#,
    );
    assert!(
        codes.contains(&2339),
        "the un-annotated read is the baseline the bug regressed against; it \
         must stay TS2339. Got: {codes:?}"
    );
}

/// An unrelated member on the same root, immediately after the bare
/// `@type`-tagged statement, must independently keep reporting TS2339 — the
/// declaration mechanism must not leak namespace-wide.
#[test]
fn unrelated_member_after_bare_type_tag_still_reports_ts2339() {
    let codes = diag_codes_js(
        r#"
var exports = {};
/** @type {string} */
exports.SomeName;
exports.Other;
"#,
    );
    let ts2339_count = codes.iter().filter(|&&c| c == 2339).count();
    assert_eq!(
        ts2339_count, 2,
        "both `exports.SomeName` and `exports.Other` must independently report \
         TS2339. Got: {codes:?}"
    );
}
