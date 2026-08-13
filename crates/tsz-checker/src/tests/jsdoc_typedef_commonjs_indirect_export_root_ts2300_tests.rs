//! A JSDoc `@typedef` collides with a same-file value declaration (TS2300)
//! only when the value side is a genuine scope-level declaration. A
//! CommonJS export-property write is never that, regardless of whether the
//! export target is `exports`/`module.exports` directly or a local object
//! literal later assigned wholesale to `module.exports`
//! (`const ns = {}; ns.Foo = class {}; module.exports = ns;`).
//!
//! `check_jsdoc_typedef_name_conflicts`'s CommonJS branch tracked exactly
//! that indirect pattern (`collect_commonjs_export_object_roots`) and
//! treated the property write as if it declared `Foo` in file scope,
//! producing a spurious `TS2300` against a same-named `@typedef`
//! (`TypeScript/tests/cases/conformance/jsdoc/typedefCrossModule3.ts`,
//! oracle `typescript@7.0.2`: clean). The direct forms never reached that
//! branch to begin with (`export_object_roots` was only ever populated by
//! `module.exports = <identifier>`), so removing the branch entirely closes
//! the gap without touching the direct-form (already-correct) behavior.

use tsz_common::options::checker::CheckerOptions;

fn js_diags(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    let opts = CheckerOptions {
        check_js: true,
        allow_js: true,
        ..CheckerOptions::default()
    };
    crate::test_utils::check_source(source, "test.js", opts)
}

fn assert_no_ts2300(source: &str) {
    let diags = js_diags(source);
    assert!(
        !diags.iter().any(|d| d.code == 2300),
        "expected no TS2300, got: {diags:?}"
    );
}

#[test]
fn indirect_export_root_property_class_does_not_conflict_with_same_name_typedef() {
    assert_no_ts2300(
        r#"
/** @typedef {number} Foo */
const ns = {};
ns.Foo = class {}
module.exports = ns;
"#,
    );
}

#[test]
fn indirect_export_root_renamed_binders_still_do_not_conflict() {
    // Same shape, different names throughout: not keyed off `ns`/`Foo`.
    assert_no_ts2300(
        r#"
/** @typedef {string} Widget */
const surface = {};
surface.Widget = class {}
module.exports = surface;
"#,
    );
}

#[test]
fn indirect_export_root_property_function_does_not_conflict_with_same_name_typedef() {
    // Control: a non-constructor-like RHS (plain function) never registered
    // as a type-capable export either way, but must stay clean post-fix.
    assert_no_ts2300(
        r#"
/** @typedef {number} Foo */
const ns = {};
ns.Foo = function () {};
module.exports = ns;
"#,
    );
}

#[test]
fn direct_exports_property_class_does_not_conflict_with_same_name_typedef() {
    // Control: the direct `exports.X = class {}` form, unaffected by this
    // fix (it never reached the removed branch).
    assert_no_ts2300(
        r#"
/** @typedef {number} Foo */
exports.Foo = class {}
"#,
    );
}

#[test]
fn direct_module_exports_property_class_does_not_conflict_with_same_name_typedef() {
    // Control: the direct `module.exports.X = class {}` form.
    assert_no_ts2300(
        r#"
/** @typedef {number} Foo */
module.exports.Foo = class {}
"#,
    );
}

#[test]
fn genuine_top_level_class_declaration_still_conflicts_with_same_name_typedef() {
    // Positive control: a real scope-level declaration must still collide.
    let diags = js_diags(
        r#"
class Foo {}
/** @typedef {number} Foo */
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2300),
        "expected TS2300 for a genuine same-file class/typedef name collision, got: {diags:?}"
    );
}
