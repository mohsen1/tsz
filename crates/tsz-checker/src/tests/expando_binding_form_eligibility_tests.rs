//! Expando-host binding-form eligibility: which declaration forms let
//! `Foo.prop = value` declare a new member on `Foo`.
//!
//! `tsc` (oracle-verified against `typeFromPropertyAssignment29.ts`) accepts a
//! TS-file expando host only for a function declaration or a `const`-bound
//! function/arrow expression (plus a function merged with a `namespace`,
//! unaffected by this fix). It rejects:
//! - a `var`/`let`-bound function expression ("must be const"),
//! - a class declaration ("classes already have statics"),
//! - a class expression, `var`/`let`/`const`-bound alike.
//!
//! tsz previously accepted all of these unconditionally in TS files: the
//! binder's per-file expando-property recording (`expression_flow.rs`) never
//! checked const-ness or excluded class expressions outside JS files, and the
//! checker's own write-eligibility gate (`is_expando_function_assignment`)
//! included `CLASS` in its "declared function or class" mask regardless of
//! file kind. JS (`checkJs`) files keep the original, more permissive rules
//! throughout — these are TS-only tightenings.

use crate::context::CheckerOptions;
use crate::test_utils::check_source;

fn ts_codes(source: &str) -> Vec<u32> {
    check_source(source, "test.ts", CheckerOptions::default())
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn js_codes(source: &str) -> Vec<u32> {
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

// --- TS files: rejected binding forms. ---

#[test]
fn ts_class_declaration_rejects_expando_write() {
    let source = "class C {\n  n = 1;\n}\nC.prop = 2;\n";
    assert!(ts_codes(source).contains(&2339));
}

#[test]
fn ts_class_declaration_rejects_expando_read() {
    let source = "class C {\n  n = 1;\n}\nC.prop = 2;\nC.prop;\n";
    assert!(ts_codes(source).contains(&2339));
}

/// Same rule, renamed binder and no instance members, to show the rejection
/// is structural rather than tied to a spelling or an instance-member clash.
#[test]
fn ts_class_declaration_rejects_expando_write_renamed() {
    let source = "class Widget {}\nWidget.factory = 1;\n";
    assert!(ts_codes(source).contains(&2339));
}

#[test]
fn ts_var_bound_function_expression_rejects_expando_write() {
    let source = "var f = function () {};\nf.prop = 2;\n";
    assert!(ts_codes(source).contains(&2339));
}

#[test]
fn ts_let_bound_function_expression_rejects_expando_write() {
    let source = "let f = function () {};\nf.prop = 2;\n";
    assert!(ts_codes(source).contains(&2339));
}

#[test]
fn ts_var_bound_class_expression_rejects_expando_write() {
    let source = "var C = class {\n  n = 1;\n};\nC.prop = 3;\n";
    assert!(ts_codes(source).contains(&2339));
}

/// A class expression never qualifies in TS, even `const`-bound: unlike a
/// function expression, const-ness does not rescue it.
#[test]
fn ts_const_bound_class_expression_rejects_expando_write() {
    let source = "const C = class {\n  n = 1;\n};\nC.prop = 3;\n";
    assert!(ts_codes(source).contains(&2339));
}

// --- TS files: still-accepted binding forms (regression control). ---

#[test]
fn ts_const_bound_function_expression_still_accepts_expando_write() {
    let source = "const f = function () {};\nf.prop = 2;\nf.prop;\n";
    assert!(!ts_codes(source).contains(&2339));
}

#[test]
fn ts_const_bound_arrow_still_accepts_expando_write() {
    let source = "const f = () => {};\nf.prop = 2;\nf.prop;\n";
    assert!(!ts_codes(source).contains(&2339));
}

#[test]
fn ts_function_declaration_still_accepts_expando_write() {
    let source = "function f() {}\nf.prop = 2;\nf.prop;\n";
    assert!(!ts_codes(source).contains(&2339));
}

/// Function-merged-with-namespace hosts are untouched by this fix.
#[test]
fn ts_function_namespace_merge_still_accepts_expando_write() {
    let source = concat!(
        "function f(n: number) { return n; }\n",
        "namespace f {\n  export var p = 1;\n}\n",
        "f.p2 = 2;\nf.p2;\n",
    );
    assert!(!ts_codes(source).contains(&2339));
}

// --- JS (checkJs) files: unaffected, still permissive. ---

#[test]
fn js_var_bound_function_expression_still_accepts_expando_write() {
    let source = "var f = function () {};\nf.prop = 2;\nf.prop;\n";
    assert!(!js_codes(source).contains(&2339));
}

#[test]
fn js_var_bound_class_expression_still_accepts_expando_write() {
    let source = "var C = class {\n  n = 1;\n};\nC.prop = 3;\nC.prop;\n";
    assert!(!js_codes(source).contains(&2339));
}

// --- JS (checkJs) files: namespace/module hosts (oracle-verified via
// typescript@7.0.2). A pure namespace or value-module root grants JS
// expando write/read eligibility only when it is ALSO bound as a
// FUNCTION or CLASS (the `function f() {} namespace f {}` merge
// pattern) — `VALUE_MODULE`/`NAMESPACE_MODULE` alone never qualify,
// mirroring the class/non-const-function rejections above.

fn cross_file_js_codes(files: &[(&str, &str)]) -> Vec<u32> {
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        ..CheckerOptions::default()
    };
    crate::test_utils::check_multi_file_with_global_index(files, "b.js", options)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

/// A plain ambient namespace (no function merge) does not grant expando
/// write eligibility: `C.prototype = {}` in JS has no declared `prototype`
/// member on `typeof C` and is `TS2339`, matching an undeclared-member
/// write like `C.baz` (the pair below).
#[test]
fn js_namespace_only_host_rejects_expando_prototype_write() {
    let codes = cross_file_js_codes(&[
        (
            "a.d.ts",
            "declare namespace C {\n  function bar(): void\n}\n",
        ),
        ("b.js", "C.prototype = {};\n"),
    ]);
    assert!(codes.contains(&2339));
}

/// Same rule, an ordinary undeclared member name instead of `prototype`,
/// to show the rejection is structural (no callable/constructible host)
/// rather than tied to the `prototype` spelling specifically.
#[test]
fn js_namespace_only_host_rejects_expando_undeclared_member_write() {
    let codes = cross_file_js_codes(&[
        (
            "a.d.ts",
            "declare namespace C {\n  function bar(): void\n}\n",
        ),
        ("b.js", "C.baz = 5;\n"),
    ]);
    assert!(codes.contains(&2339));
}

/// A value-only namespace (no callable member at all) is rejected the
/// same way — the exclusion is not specific to namespaces that happen to
/// declare a function member.
#[test]
fn js_value_only_namespace_host_rejects_expando_prototype_write() {
    let codes = cross_file_js_codes(&[
        ("a.d.ts", "declare namespace C {\n  let bar: number;\n}\n"),
        ("b.js", "C.prototype = {};\n"),
    ]);
    assert!(codes.contains(&2339));
}

/// Regression control: a namespace merged with a function declaration
/// still grants expando write eligibility, since the FUNCTION flag alone
/// is sufficient — unaffected by removing the module flags.
#[test]
fn js_function_namespace_merge_still_accepts_expando_prototype_write() {
    let codes = cross_file_js_codes(&[
        (
            "a.d.ts",
            "declare function C(): void;\ndeclare namespace C {\n  function bar(): void\n}\n",
        ),
        ("b.js", "C.prototype = {};\n"),
    ]);
    assert!(!codes.contains(&2339));
}
