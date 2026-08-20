//! Display of JS-inferred function parameters in diagnostic messages (#17227).
//!
//! A bare, unannotated parameter in a JS file is `optional` in the solver
//! signature only for call-arity leniency; `tsc` never renders it with `?`.
//! `tsc` reserves the displayed `?` for a written `?` (TS only), an
//! initializer, or a JSDoc optional marker (`@param [a]` / `@param {T=} a`).
//! Verified against the pinned `typescript@7.0.2` oracle (see #17227's
//! oracle table): `function f(tree) {}` displays as `(tree: any) => void` in
//! error messages and declaration emit, while `/** @param [a] */` and
//! `function f(a = 1) {}` keep `a?`.
//!
//! Call-arity leniency itself is pinned separately by
//! `js_file_function_parameters_as_optional_tests.rs` and MUST NOT change:
//! the display mask never feeds `required_param_count` or subtyping.

use crate::context::CheckerOptions;
use crate::test_utils::{check_multi_file_with_libs_stamped, check_source, load_lib_files};

const NOT_ASSIGNABLE: u32 = 2322;
const PROPERTY_DOES_NOT_EXIST: u32 = 2339;
const TOO_FEW_ARGUMENTS: u32 = 2554;

fn js_options() -> CheckerOptions {
    CheckerOptions {
        allow_js: true,
        check_js: true,
        ..CheckerOptions::default()
    }
}

fn message_of(source: &str, file_name: &str, code: u32) -> String {
    let diags = check_source(source, file_name, js_options());
    diags
        .iter()
        .find(|d| d.code == code)
        .unwrap_or_else(|| {
            panic!(
                "expected TS{code} in {file_name}, got: {:?}",
                diags
                    .iter()
                    .map(|d| (d.code, d.message_text.clone()))
                    .collect::<Vec<_>>()
            )
        })
        .message_text
        .clone()
}

// --- The witness: salsa/moduleExportAssignment2's shape (#17227). ---

#[test]
fn bare_js_param_displays_required_in_export_assignment_property_error() {
    // module.exports = f + a sibling exports property write, then a missing
    // property read off the export= target: the TS2339 receiver renders the
    // function signature. tsc: `(tree: any) => void`, never `tree?`.
    let src = "function f(tree) { }\n\
               module.exports = f;\n\
               module.exports.p = 1;\n\
               module.exports.missing;\n";
    let msg = message_of(src, "npm.js", PROPERTY_DOES_NOT_EXIST);
    assert!(
        msg.contains("(tree: any) => void"),
        "bare JS param must display required, got: {msg}"
    );
    assert!(
        !msg.contains("tree?"),
        "bare JS param must not display an optional marker, got: {msg}"
    );
}

// --- Direct display through assignability, no CommonJS machinery. ---

#[test]
fn bare_js_param_displays_required_in_assignability_error() {
    let src = "function walk(tree) { }\n\
               /** @type {number} */\n\
               var n = walk;\n";
    let msg = message_of(src, "a.js", NOT_ASSIGNABLE);
    assert!(
        msg.contains("(tree: any) => void"),
        "expected required display, got: {msg}"
    );
}

#[test]
fn multiple_bare_js_params_all_display_required() {
    let src = "function pair(left, right) { }\n\
               /** @type {number} */\n\
               var n = pair;\n";
    let msg = message_of(src, "b.js", NOT_ASSIGNABLE);
    assert!(
        msg.contains("(left: any, right: any) => void"),
        "expected both params required, got: {msg}"
    );
}

// --- Controls that must KEEP their `?`. ---

#[test]
fn jsdoc_bracket_optional_param_keeps_optional_marker() {
    // `@param [a]` is a genuine optional marker; tsc displays `a?: any`.
    let src = "/** @param [seed] */\n\
               function init(seed) { }\n\
               /** @type {number} */\n\
               var n = init;\n";
    let msg = message_of(src, "c.js", NOT_ASSIGNABLE);
    assert!(
        msg.contains("seed?"),
        "JSDoc bracket-optional must keep `?`, got: {msg}"
    );
}

#[test]
fn jsdoc_equals_suffix_optional_param_keeps_optional_marker() {
    // `@param {number=} n` is Closure's optional-type marker.
    let src = "/** @param {number=} amount */\n\
               function bump(amount) { }\n\
               /** @type {string} */\n\
               var s = bump;\n";
    let msg = message_of(src, "d.js", NOT_ASSIGNABLE);
    assert!(
        msg.contains("amount?"),
        "JSDoc `{{T=}}` optional must keep `?`, got: {msg}"
    );
}

#[test]
fn default_valued_js_param_keeps_optional_marker() {
    let src = "function scale(factor = 1) { }\n\
               /** @type {string} */\n\
               var s = scale;\n";
    let msg = message_of(src, "e.js", NOT_ASSIGNABLE);
    assert!(
        msg.contains("factor?"),
        "initializer param must keep `?`, got: {msg}"
    );
}

#[test]
fn mixed_bare_and_default_params_display_independently() {
    let src = "function mix(head, tail = 1) { }\n\
               /** @type {string} */\n\
               var s = mix;\n";
    let msg = message_of(src, "f.js", NOT_ASSIGNABLE);
    assert!(
        msg.contains("head: any") && !msg.contains("head?"),
        "bare param must display required, got: {msg}"
    );
    assert!(
        msg.contains("tail?"),
        "default-valued param must keep `?`, got: {msg}"
    );
}

// --- TS control: identical shapes in TS keep their written form. ---

#[test]
fn ts_written_optional_param_still_displays_optional() {
    let src = "function t(tree?: any) { }\n\
               const n: number = t;\n";
    let msg = message_of(src, "g.ts", NOT_ASSIGNABLE);
    assert!(
        msg.contains("tree?"),
        "TS written `?` must display, got: {msg}"
    );
}

#[test]
fn ts_required_param_still_displays_required() {
    let src = "function t(tree: any) { }\n\
               const n: number = t;\n";
    let msg = message_of(src, "h.ts", NOT_ASSIGNABLE);
    assert!(
        msg.contains("(tree: any) => void") && !msg.contains("tree?"),
        "TS required param display changed: {msg}"
    );
}

// --- Identity split: the SAME program (same file, even) holds a bare JS
// --- param and a genuinely optional JSDoc param whose `FunctionShape`
// --- structs are identical. They must not share a display identity: the
// --- masked shape interns under its own `FunctionShapeId`.

#[test]
fn js_bare_and_jsdoc_optional_same_shape_display_differently_in_one_program() {
    let src = "/** @param [tree] */\n\
               function jsOpt(tree) { }\n\
               function jsBare(tree) { }\n\
               /** @type {number} */\n\
               var n1 = jsOpt;\n\
               /** @type {number} */\n\
               var n2 = jsBare;\n";
    let diags = check_source(src, "pair.js", js_options());
    let msgs: Vec<&String> = diags
        .iter()
        .filter(|d| d.code == NOT_ASSIGNABLE)
        .map(|d| &d.message_text)
        .collect();
    assert!(
        msgs.iter().any(|m| m.contains("(tree?: any)")),
        "JSDoc bracket-optional lost its `?` display: {msgs:?}"
    );
    assert!(
        msgs.iter()
            .any(|m| m.contains("(tree: any) => void") && !m.contains("tree?")),
        "JS bare param lost its required display: {msgs:?}"
    );
}

// --- The arity leniency itself must be untouched by the display mask. ---

#[test]
fn bare_js_param_call_arity_stays_lenient() {
    let files = [
        ("decl.js", "function lenient(alpha, beta) { }\n"),
        ("use.ts", "lenient();\nlenient(1);\n"),
    ];
    let codes: Vec<u32> = check_multi_file_with_libs_stamped(
        &files,
        "use.ts",
        js_options(),
        &load_lib_files(&["es5.d.ts"]),
    )
    .into_iter()
    .map(|d| d.code)
    .collect();
    assert!(
        !codes.contains(&TOO_FEW_ARGUMENTS),
        "display mask must not affect call arity, got: {codes:?}"
    );
}
