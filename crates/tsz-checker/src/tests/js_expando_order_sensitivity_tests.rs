//! Which JS expando receivers have *ordered* property declarations.
//!
//! `tsc` reports TS2565 "used before being assigned" when the receiver is a
//! function or class declaration — there the expando property is a declaration
//! on that object, so a use preceding the assignment is an error. A plain object
//! (`var o = {}`) is not ordered: `tsc` types it from every assignment in the
//! program regardless of position, so a use written before the assignment is
//! fine.
//!
//! A CommonJS `exports`/`module.exports` object IS ordered (#17608) — UNLESS
//! the assignment's right-hand side is an "entity name expression" (a bare
//! identifier or a dotted chain of property accesses rooted in one, e.g. `k` or
//! `ns.k`). tsc binds `exports.f = k` as an alias of `k` rather than as a
//! flow-tracked property declaration, and alias resolution has no
//! temporal-dead-zone, so a read preceding an entity-name-RHS assignment is
//! fine regardless of position. A non-entity-name RHS (function expression,
//! object literal, call, literal, …) is a real declaration and keeps the
//! ordering.
//!
//! Verified against a real tsc 6.0.2 oracle (`--allowJs --checkJs --strict`):
//!
//! ```text
//! function C() {} C.f(); C.f = a;            -> TS2565 (ordered)
//! var o = {};      o.f(); o.f = a;           -> nothing (not ordered)
//! exports.f();     function a(){} exports.f = a; -> nothing (alias RHS, no TDZ)
//! exports.f();     exports.f = function a(){};   -> TS2565 (real declaration)
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

// --- CommonJS `exports`/`module.exports` receivers are ordered (#17608). ---

/// A function-expression RHS is a real declaration, not an alias: a read
/// preceding it is genuinely used-before-assigned.
#[test]
fn commonjs_exports_function_expression_rhs_is_ordered() {
    let source = "var x = exports.j;\nexports.j = function j() { };\n";
    assert!(js_codes(source).contains(&USED_BEFORE_ASSIGNED));
}

/// `module.exports.p` reports the same as `exports.p` — both are the CommonJS
/// export surface, oracle-verified independently (#17608's own repro).
#[test]
fn commonjs_module_exports_function_expression_rhs_is_ordered() {
    let source = "module.exports.jj = module.exports.j;\nmodule.exports.j = function j() { };\n";
    assert!(js_codes(source).contains(&USED_BEFORE_ASSIGNED));
}

/// A different binder/property name, so the ordered CommonJS rule is
/// structural, not keyed on a specific identifier.
#[test]
fn commonjs_module_exports_function_expression_rhs_is_ordered_renamed() {
    let source = "module.exports.readyFlag = module.exports.state;\nmodule.exports.state = function initState() { };\n";
    assert!(js_codes(source).contains(&USED_BEFORE_ASSIGNED));
}

/// An object-literal RHS is equally a real declaration (not just function
/// expressions).
#[test]
fn commonjs_exports_object_literal_rhs_is_ordered() {
    let source = "var x = exports.j;\nexports.j = { };\n";
    assert!(js_codes(source).contains(&USED_BEFORE_ASSIGNED));
}

/// A property never assigned anywhere in the file is a missing-member error
/// (TS2339), not "used before assigned" (TS2565) — the ordered-declaration
/// check must not fire when there is no declaration at all.
#[test]
fn commonjs_exports_never_assigned_reports_missing_member_not_used_before_assigned() {
    let codes = js_codes("var x = exports.neverAssigned;\n");
    assert!(!codes.contains(&USED_BEFORE_ASSIGNED));
    assert!(codes.contains(&2339));
}

// --- Alias (entity-name RHS) assignments have no TDZ. ---

#[test]
fn commonjs_exports_alias_of_hoisted_function_is_not_ordered() {
    let source = "exports.f()\nfunction a() { }\nexports.f = a;\n";
    assert!(!js_codes(source).contains(&USED_BEFORE_ASSIGNED));
}

/// A dotted entity-name chain (`ns.inner`) is exempt exactly like a bare
/// identifier — both bind as aliases in tsc.
#[test]
fn commonjs_exports_alias_of_dotted_property_chain_is_not_ordered() {
    let source = concat!(
        "var x = exports.j;\n",
        "var ns = { inner: { deep: 1 } };\n",
        "exports.j = ns.inner.deep;\n",
    );
    assert!(!js_codes(source).contains(&USED_BEFORE_ASSIGNED));
}

#[test]
fn plain_object_expando_is_not_ordered() {
    let source = "var o = {}\no.f()\nfunction a() { }\no.f = a;\n";
    assert!(!js_codes(source).contains(&USED_BEFORE_ASSIGNED));
}

/// Witness `typeFromPropertyAssignment17`: a property assigned later in the file
/// and used earlier through the module object — the RHS (`undefined`, then `h`)
/// is either a literal preceding the read or an alias, so no ordered
/// declaration ever lands after the read.
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

/// The ordinary in-order case stays silent everywhere.
#[test]
fn assignment_before_use_is_silent_for_every_receiver() {
    for source in [
        "function C() { }\nfunction a() { }\nC.f = a;\nC.f()\n",
        "var o = {}\nfunction a() { }\no.f = a;\no.f()\n",
        "function a() { }\nexports.f = a;\nexports.f()\n",
        "exports.f = { };\nexports.f()\n",
    ] {
        assert!(!js_codes(source).contains(&USED_BEFORE_ASSIGNED));
    }
}
