//! Which JS expando receivers have *ordered* property declarations.
//!
//! `tsc` reports TS2565 "used before being assigned" only when the receiver is a
//! function or class declaration — there the expando property is a declaration
//! on that object, so a use preceding the assignment is an error. A plain object
//! (`var o = {}`) or a CommonJS `exports` object is not ordered: `tsc` types
//! those from every assignment in the program regardless of position, so a use
//! written before the assignment is fine.
//!
//! Verified against the pinned tsc 7.0.2 (`--allowJs --checkJs --strict`):
//!
//! ```text
//! function C() {} C.f(); C.f = a;   -> TS2565   (ordered)
//! var o = {};     o.f(); o.f = a;   -> nothing  (not ordered)
//! exports.f();           exports.f = a; -> nothing (not ordered)
//! ```

use crate::context::CheckerOptions;
use crate::test_utils::check_source;

fn js_codes(source: &str) -> Vec<u32> {
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        strict: true,
        ..CheckerOptions::default()
    };
    check_source(source, "test.js", options)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

const USED_BEFORE_ASSIGNED: u32 = 2565;

// --- Ordered receivers keep reporting. ---

#[test]
fn function_declaration_expando_is_ordered() {
    let source = "function C() { }\nC.f()\nfunction a() { }\nC.f = a;\n";
    assert!(js_codes(source).contains(&USED_BEFORE_ASSIGNED));
}

/// A different binder name and property, so the rule is structural.
#[test]
fn function_declaration_expando_is_ordered_renamed() {
    let source = "function Widget() { }\nWidget.render()\nfunction r() { }\nWidget.render = r;\n";
    assert!(js_codes(source).contains(&USED_BEFORE_ASSIGNED));
}

#[test]
fn class_declaration_expando_is_ordered() {
    let source = "class K { }\nK.f()\nfunction a() { }\nK.f = a;\n";
    assert!(js_codes(source).contains(&USED_BEFORE_ASSIGNED));
}

/// A `const` initialised with a function expression is a function declaration in
/// all but syntax, and stays ordered.
#[test]
fn function_expression_binding_is_ordered() {
    let source = "const C = function () { };\nC.f()\nfunction a() { }\nC.f = a;\n";
    assert!(js_codes(source).contains(&USED_BEFORE_ASSIGNED));
}

// --- Unordered receivers must not report. ---

#[test]
fn plain_object_expando_is_not_ordered() {
    let source = "var o = {}\no.f()\nfunction a() { }\no.f = a;\n";
    assert!(!js_codes(source).contains(&USED_BEFORE_ASSIGNED));
}

#[test]
fn commonjs_exports_is_not_ordered() {
    let source = "exports.f()\nfunction a() { }\nexports.f = a;\n";
    assert!(!js_codes(source).contains(&USED_BEFORE_ASSIGNED));
}

/// Witness `typeFromPropertyAssignment17`: a property assigned later in the file
/// and used earlier through the module object.
#[test]
fn exports_property_assigned_after_use_is_not_ordered() {
    let source = concat!(
        "exports.helper = undefined;\n",
        "exports.helper()\n",
        "function h() { }\n",
        "exports.helper = h;\n",
    );
    assert!(!js_codes(source).contains(&USED_BEFORE_ASSIGNED));
}

// --- `exports`/`module.exports` are ordered per-property when that
// property's own RHS is not an "aliasable expression" (an identifier,
// dotted name, or class expression) — tsc's binder gives an aliasable-RHS
// export assignment `SymbolFlags.Alias` (never ordered, see above); any
// other RHS shape is a real `Property` declaration, ordered like the
// function/class declarations at the top of this file. Oracle-verified
// (`tsc` 6.0.2, `--allowJs --checkJs --strict`).

#[test]
fn commonjs_exports_function_expression_assignment_is_ordered() {
    let source = "exports.jj = exports.j;\nexports.j = function j() { };\n";
    assert!(js_codes(source).contains(&USED_BEFORE_ASSIGNED));
}

/// `module.exports.NAME` behaves identically to bare `exports.NAME`.
#[test]
fn commonjs_module_exports_function_expression_assignment_is_ordered() {
    let source = "module.exports.jj = module.exports.j;\nmodule.exports.j = function j() { };\n";
    assert!(js_codes(source).contains(&USED_BEFORE_ASSIGNED));
}

/// A different binder/property name, so the rule is structural.
#[test]
fn commonjs_exports_function_expression_assignment_is_ordered_renamed() {
    let source = "exports.pending = exports.widget;\nexports.widget = function widget() { };\n";
    assert!(js_codes(source).contains(&USED_BEFORE_ASSIGNED));
}

#[test]
fn commonjs_exports_arrow_function_assignment_is_ordered() {
    let source = "exports.jj = exports.j;\nexports.j = () => { };\n";
    assert!(js_codes(source).contains(&USED_BEFORE_ASSIGNED));
}

#[test]
fn commonjs_exports_object_literal_assignment_is_ordered() {
    let source = "exports.jj = exports.j;\nexports.j = { };\n";
    assert!(js_codes(source).contains(&USED_BEFORE_ASSIGNED));
}

/// A class expression is still an "aliasable expression" in tsc's binder
/// (`isAliasableExpression` covers entity names *and* class expressions), so
/// unlike the function-expression/arrow/object-literal RHS above it stays
/// unordered.
#[test]
fn commonjs_exports_class_expression_assignment_is_not_ordered() {
    let source = "exports.jj = exports.j;\nexports.j = class { };\n";
    assert!(!js_codes(source).contains(&USED_BEFORE_ASSIGNED));
}

/// An identifier-reference RHS keeps the property unordered even when the
/// referenced declaration is itself a hoisted function.
#[test]
fn commonjs_exports_identifier_assignment_is_still_not_ordered() {
    let source = "exports.jj = exports.j;\nfunction j() { }\nexports.j = j;\n";
    assert!(!js_codes(source).contains(&USED_BEFORE_ASSIGNED));
}

/// In-order assignment stays silent for the non-aliasable RHS shape too.
#[test]
fn commonjs_exports_function_expression_assignment_before_use_is_silent() {
    let source = "exports.j = function j() { };\nexports.jj = exports.j;\n";
    assert!(!js_codes(source).contains(&USED_BEFORE_ASSIGNED));
}

/// The ordinary in-order case stays silent everywhere.
#[test]
fn assignment_before_use_is_silent_for_every_receiver() {
    for source in [
        "function C() { }\nfunction a() { }\nC.f = a;\nC.f()\n",
        "var o = {}\nfunction a() { }\no.f = a;\no.f()\n",
        "function a() { }\nexports.f = a;\nexports.f()\n",
    ] {
        assert!(!js_codes(source).contains(&USED_BEFORE_ASSIGNED));
    }
}
