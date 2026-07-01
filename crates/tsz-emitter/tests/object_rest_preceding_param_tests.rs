//! ES2018 "preceding object rest/spread" rule (issue #15249).
//!
//! At an ES module target below ES2018 but at/above ES2015 (binding patterns
//! native, object rest/spread not), once a parameter's binding contains an
//! object rest, that parameter *and every parameter after it* is rewritten: the
//! leading object-rest parameter keeps a native binding (`{ a } = _a`), and every
//! following binding parameter is replaced by a generated temp with its
//! destructuring fully flattened into the body, in parameter order. A following
//! plain identifier stays in the list; its default is hoisted as an
//! `if (name === void 0) { … }` guard, which forces a single-line body multi-line.
//!
//! Structural (not name-keyed): the tests vary binder names and assert on shape.

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_print_with_opts;
use tsz_common::common::ScriptTarget;
use tsz_emitter::output::printer::PrintOptions;

fn es2017() -> PrintOptions {
    PrintOptions {
        target: ScriptTarget::ES2017,
        ..Default::default()
    }
}

/// The issue's core repro: a following array binding (with a hole) is lowered to
/// a temp and flattened to indexed reads, in parameter order after the leading
/// object-rest preamble.
#[test]
fn following_array_pattern_is_flattened_after_leading_object_rest() {
    let source = "function f({ a, ...r }: any, [c, , d]: number[]) { return a + c + d + r; }";
    let output = parse_and_print_with_opts(source, es2017());
    assert!(
        output.contains(
            "function f(_a, _b) { var { a } = _a, r = __rest(_a, [\"a\"]); var c = _b[0], d = _b[2]; return a + c + d + r; }"
        ),
        "following array pattern must be flattened to indexed reads.\nOutput:\n{output}"
    );
}

/// A following object binding is flattened to property reads (`var b = _b.b`),
/// unlike the leading object-rest which keeps a native binding (`{ a } = _a`).
#[test]
fn following_object_pattern_is_flattened_to_property_reads() {
    let source = "function g({ a, ...r }: any, { b }: any) { return a + b + r; }";
    let output = parse_and_print_with_opts(source, es2017());
    assert!(
        output.contains(
            "function g(_a, _b) { var { a } = _a, r = __rest(_a, [\"a\"]); var b = _b.b; return a + b + r; }"
        ),
        "following object pattern must be flattened to property reads.\nOutput:\n{output}"
    );
}

/// A following object binding that carries its own object rest is flattened and
/// still routes the rest through `__rest`.
#[test]
fn following_object_pattern_with_own_rest_uses_rest_helper() {
    let source = "function h({ a, ...r }: any, { b, ...r2 }: any) { return r2; }";
    let output = parse_and_print_with_opts(source, es2017());
    assert!(
        output.contains(
            "function h(_a, _b) { var { a } = _a, r = __rest(_a, [\"a\"]); var b = _b.b, r2 = __rest(_b, [\"b\"]); return r2; }"
        ),
        "following object pattern with own rest must flatten and keep __rest.\nOutput:\n{output}"
    );
}

/// A binding parameter *before* the first object-rest parameter stays native in
/// the parameter list; only the object-rest parameter and everything after it is
/// rewritten.
#[test]
fn binding_before_object_rest_stays_native() {
    let source = "function j([c, d]: number[], { a, ...r }: any) { return c + d + a + r; }";
    let output = parse_and_print_with_opts(source, es2017());
    assert!(
        output.contains(
            "function j([c, d], _a) { var { a } = _a, r = __rest(_a, [\"a\"]); return c + d + a + r; }"
        ),
        "a binding before the first object-rest parameter must stay native.\nOutput:\n{output}"
    );
}

/// A following plain-identifier default is hoisted to an `if (name === void 0)`
/// guard and forces the single-line source body multi-line.
#[test]
fn following_plain_default_hoists_and_forces_multiline() {
    let source = "function k({ a, ...r }: any, x = 5) { return a + x + r; }";
    let output = parse_and_print_with_opts(source, es2017());
    assert!(
        output.contains("function k(_a, x) {\n"),
        "a following plain-identifier default must force a multi-line body.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var { a } = _a, r = __rest(_a, [\"a\"]);"),
        "leading object-rest preamble missing.\nOutput:\n{output}"
    );
    assert!(
        output.contains("if (x === void 0) { x = 5; }"),
        "following plain default guard missing.\nOutput:\n{output}"
    );
}

/// A following `...rest` parameter stays native in the parameter list.
#[test]
fn following_rest_identifier_stays_in_parameter_list() {
    let source = "function m({ a, ...r }: any, ...rest: number[]) { return rest; }";
    let output = parse_and_print_with_opts(source, es2017());
    assert!(
        output.contains(
            "function m(_a, ...rest) { var { a } = _a, r = __rest(_a, [\"a\"]); return rest; }"
        ),
        "a following rest identifier must stay in the parameter list.\nOutput:\n{output}"
    );
}

/// Renamed binders produce the same structural lowering — the rule is keyed on
/// the pattern shape, not any identifier text.
#[test]
fn renamed_binders_produce_same_structural_lowering() {
    let source = "function fn({ alpha, ...beta }: any, [gamma, delta]: number[]) { return alpha + gamma + delta + beta; }";
    let output = parse_and_print_with_opts(source, es2017());
    assert!(
        output.contains(
            "function fn(_a, _b) { var { alpha } = _a, beta = __rest(_a, [\"alpha\"]); var gamma = _b[0], delta = _b[1]; return alpha + gamma + delta + beta; }"
        ),
        "renamed binders must lower structurally.\nOutput:\n{output}"
    );
}

/// A method with the same shape lowers identically (the parameter path is shared
/// by function, arrow, and method builders).
#[test]
fn method_following_binding_is_flattened() {
    let source = "class C { m({ a, ...r }: any, [c, d]: number[]) { return a + c + d + r; } }";
    let output = parse_and_print_with_opts(source, es2017());
    assert!(
        output.contains("var { a } = _a, r = __rest(_a, [\"a\"]); var c = _b[0], d = _b[1];"),
        "method following binding must be flattened.\nOutput:\n{output}"
    );
}

/// Deep nested case: the leading object-rest's own nested temp is numbered
/// before the following parameter's nested temp (`_c` then `_d`), because both
/// parameter-level temps (`_a`, `_b`) are reserved before any flattening —
/// matching `tsc`'s print-order temp numbering.
#[test]
fn nested_temps_are_numbered_after_all_parameter_temps() {
    let source = "function n({ a: { p, ...q }, ...r }: any, [{ x, ...y }]: any[]) { return q; }";
    let output = parse_and_print_with_opts(source, es2017());
    assert!(
        output.contains(
            "function n(_a, _b) { var _c = _a.a, { p } = _c, q = __rest(_c, [\"p\"]), r = __rest(_a, [\"a\"]); var _d = _b[0], x = _d.x, y = __rest(_d, [\"x\"]); return q; }"
        ),
        "nested temps must be numbered after all parameter temps.\nOutput:\n{output}"
    );
}
