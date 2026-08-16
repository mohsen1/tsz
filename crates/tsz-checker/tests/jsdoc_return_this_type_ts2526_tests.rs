//! Regression coverage for a JSDoc `@return {this}` tag in a position that
//! supplies no `this` type.
//!
//! Structural rule: a `@return`/`@returns` tag types the host function's own
//! return, so its `this` resolves against that function's `this` container.
//! When the container is not a non-static class or interface member, `tsc`
//! emits `TS2526` ("A 'this' type is available only in a non-static member
//! of a class or interface.") — oracle-verified (typescript@7.0.2) against
//! `TypeScript/tests/cases/conformance/salsa/thisTypeOfConstructorFunctions.ts`.
//!
//! The TS-syntax `this` type node already routed through
//! `TypeNodeChecker`'s `THIS_TYPE` branch, which asks `is_this_type_allowed`.
//! JSDoc type expressions are resolved from comment text instead, where
//! `"this"` mapped straight to the solver's `this` type with no positional
//! gate (`crates/tsz-checker/src/jsdoc/resolution/name_resolution.rs`), so
//! the same invalid position went undiagnosed in checked JS.

use tsz_checker::test_utils::check_js_source_diagnostics;

fn ts2526_count(source: &str) -> usize {
    check_js_source_diagnostics(source)
        .iter()
        .filter(|d| d.code == 2526)
        .count()
}

/// A function expression assigned onto a constructor function's prototype is
/// not a class member — the `thisTypeOfConstructorFunctions.ts` shape.
#[test]
fn return_this_on_prototype_function_expression_reports_ts2526() {
    let source = "/**\n * @class\n */\nfunction Cpp() {\n}\n/** @return {this} */\nCpp.prototype.m2 = function () { return this }\n";
    assert_eq!(ts2526_count(source), 1, "expected one TS2526");
}

/// Renamed binder: the rule keys on the host's syntactic position, not on
/// any identifier spelling.
#[test]
fn return_this_on_prototype_function_expression_renamed_binder_reports_ts2526() {
    let source = "/**\n * @class\n */\nfunction Widget() {\n}\n/** @return {this} */\nWidget.prototype.render = function () { return this }\n";
    assert_eq!(ts2526_count(source), 1, "expected one TS2526");
}

/// An object-literal method assigned onto a constructor's `prototype` is a
/// third distinct host shape (`METHOD_DECLARATION`, not `FUNCTION_EXPRESSION`
/// and not a class member) that goes through neither
/// `check_function_declaration_callback`'s statement-position dispatch nor
/// `check_class_member_with_request`'s class-member dispatch.
#[test]
fn return_this_on_prototype_object_literal_method_reports_ts2526() {
    let source = "function Cp() {\n}\nCp.prototype = {\n    /** @return {this} */\n    m4() { return this }\n};\n";
    assert_eq!(ts2526_count(source), 1, "expected one TS2526");
}

/// An arrow assigned to a `this` property inside a `@class` constructor
/// function: the arrow passes through to the enclosing plain function, which
/// supplies no `this` type.
#[test]
fn return_this_on_this_property_arrow_reports_ts2526() {
    let source = "/**\n * @class\n */\nfunction Cp() {\n    /** @return {this} */\n    this.m3 = () => this\n}\n";
    assert_eq!(ts2526_count(source), 1, "expected one TS2526");
}

/// The `@returns` spelling is the same tag.
#[test]
fn returns_spelling_reports_ts2526() {
    let source = "/**\n * @class\n */\nfunction Cpp() {\n}\n/** @returns {this} */\nCpp.prototype.m2 = function () { return this }\n";
    assert_eq!(ts2526_count(source), 1, "expected one TS2526");
}

/// Negative control: a real non-static class method does supply a `this`
/// type, so the same tag must stay silent.
#[test]
fn return_this_on_class_instance_method_is_allowed() {
    let source = "class K {\n    /** @return {this} */\n    m() { return this }\n}\n";
    assert_eq!(ts2526_count(source), 0, "instance method supplies `this`");
}

/// A `static` class method does not, so the host's own modifier must still
/// be consulted — the shared walk starts above the host node.
#[test]
fn return_this_on_class_static_method_reports_ts2526() {
    let source = "class K {\n    /** @return {this} */\n    static s() { return this }\n}\n";
    assert_eq!(ts2526_count(source), 1, "static method supplies no `this`");
}

/// Negative control for the sibling tag: `@type {this}` types the annotated
/// declaration rather than a function return, and a this-property assignment
/// host is member-like. This path is deliberately untouched.
#[test]
fn type_tag_this_on_this_property_assignment_is_allowed() {
    let source =
        "/**\n * @class\n */\nfunction A() {\n    /** @type {this} */\n    this.a = this\n}\n";
    assert_eq!(ts2526_count(source), 0, "`@type` path is not gated here");
}

/// Negative control: a non-`this` return type must not trip the gate.
#[test]
fn return_non_this_type_reports_nothing() {
    let source = "/**\n * @class\n */\nfunction Cpp() {\n}\n/** @return {number} */\nCpp.prototype.m2 = function () { return 1 }\n";
    assert_eq!(ts2526_count(source), 0, "only bare `this` is gated");
}
