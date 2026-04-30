//! Locks in tsc parity for the strict-mode `eval` / `arguments` binding error
//! inside JS (allowJs / checkJs) files: tsc does **not** raise TS1210 / TS1100
//! when the only reason the binding is in strict mode is that it sits inside a
//! class body. Class bodies are runtime-strict in JS, but tsc's bind-time
//! grammar check defers to the JS engine for that case. Explicit `"use strict"`
//! directives and module-strict scopes still flow through.
//!
//! Regression: conformance test
//! `compiler/jsFileCompilationBindStrictModeErrors.ts` would emit a spurious
//! TS1210 on `class c { a(eval) {} }` in `b.js`.

use tsz_binder::BinderState;
use tsz_checker::CheckerState;
use tsz_checker::context::CheckerOptions;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn diagnostics_with_options(source: &str, file_name: &str, opts: CheckerOptions) -> Vec<u32> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        opts,
    );

    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| d.code)
        .collect()
}

fn js_options() -> CheckerOptions {
    // `is_js_file` keys off the source file's extension, not a compiler flag,
    // so the unit-test harness only needs to hand the checker a `.js`
    // filename to flip `is_js_file()` on.
    CheckerOptions::default()
}

/// `class c { a(eval) {} }` in a JS file: class auto-strict makes the binding
/// invalid in pure-JS strict mode, but tsc does not surface it as a compile
/// error. Without this rule we emitted TS1210 (or TS1100) here.
#[test]
fn js_class_method_param_named_eval_does_not_emit_ts1210_or_ts1100() {
    let codes = diagnostics_with_options(
        "class c { a(eval) {} }\n",
        "b.js",
        js_options(),
    );
    assert!(
        !codes.contains(&1210),
        "expected no TS1210 for `eval` param in JS class method; got: {codes:?}"
    );
    assert!(
        !codes.contains(&1100),
        "expected no TS1100 for `eval` param in JS class method; got: {codes:?}"
    );
}

/// Same rule for `arguments` as a parameter name in a JS class.
#[test]
fn js_class_method_param_named_arguments_does_not_emit_ts1210_or_ts1100() {
    let codes = diagnostics_with_options(
        "class c { a(arguments) {} }\n",
        "b.js",
        js_options(),
    );
    assert!(
        !codes.contains(&1210),
        "expected no TS1210 for `arguments` param in JS class method; got: {codes:?}"
    );
    assert!(
        !codes.contains(&1100),
        "expected no TS1100 for `arguments` param in JS class method; got: {codes:?}"
    );
}

/// Guard: explicit `"use strict"` inside a JS function STILL emits the
/// strict-mode binding error. Skipping must only cover class auto-strict —
/// dropping it for explicit-strict scopes would regress
/// `jsFileCompilationBindErrors.ts`.
#[test]
fn js_function_with_explicit_use_strict_still_emits_ts1100_for_arguments_var() {
    let codes = diagnostics_with_options(
        "function b() { \"use strict\"; var arguments = 0; }\n",
        "a.js",
        js_options(),
    );
    assert!(
        codes.contains(&1100),
        "expected TS1100 for `var arguments = 0` under explicit \"use strict\" in JS; got: {codes:?}"
    );
}
