//! Regression tests for `using` / `await using` lowering in a **single-line
//! source** function body.
//!
//! A single-line function body (`function f() { using x = ...; more(); }`) used
//! to take the emitter's single-line fast path, which emits each statement
//! inline and never sets up the block-level disposal `try`. The `using`
//! declaration was then wrapped individually — its binding hoisted to a bare
//! `var` and only its initializer placed inside a `try`, leaving every
//! following statement *outside* the disposal scope. That is a semantic bug:
//! the resource is disposed before the rest of the body runs, and two `using`
//! declarations produced two separate, non-nested `try`/`finally` regions with
//! the wrong disposal order.
//!
//! tsc instead wraps the whole body in a single `try`/`catch`/`finally` with a
//! shared disposal env and keeps the `using` binding as a `const` in place —
//! exactly what the emitter already does for a multi-line body or a nested
//! block. These tests pin the structural invariants (they are independent of
//! the identifier names a user picks, so they vary the binder names rather than
//! asserting a single fixture's exact text).

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::context::emit::EmitContext;
use tsz_emitter::emitter::{Printer as EmitterPrinter, PrinterOptions};
use tsz_emitter::lowering::LoweringPass;
use tsz_parser::parser::ParserState;

fn parse_lower_emit(source: &str, opts: PrinterOptions) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let ctx = EmitContext::with_options(opts.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, opts);
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

fn opts() -> PrinterOptions {
    PrinterOptions {
        target: ScriptTarget::ES2022,
        module: ModuleKind::ESNext,
        ..Default::default()
    }
}

/// The emitted `__addDisposableResource` / `__disposeResources` helper
/// definitions are injected ahead of the user code and themselves mention
/// those names, so structural assertions must look only at the user code.
/// Return the slice starting at `marker` (the user function/class/const).
fn user_code<'a>(output: &'a str, marker: &str) -> &'a str {
    let at = output
        .find(marker)
        .unwrap_or_else(|| panic!("expected `{marker}` in output.\nOutput:\n{output}"));
    &output[at..]
}

/// A statement following a single-line-body `using` must be emitted *inside*
/// the disposal `try` (before `__disposeResources`), never after the
/// `finally`. The binding is kept as a `const` in place, not hoisted to a
/// bare `var`.
#[test]
fn single_line_using_wraps_trailing_statement_in_try() {
    let output = parse_lower_emit(
        "function scope() { using handle = { [Symbol.dispose]() {} }; sideEffect(); }",
        opts(),
    );

    let body = user_code(&output, "function scope");

    let dispose_at = body
        .find("__disposeResources")
        .unwrap_or_else(|| panic!("expected a __disposeResources call.\nOutput:\n{output}"));
    let side_at = body
        .find("sideEffect()")
        .unwrap_or_else(|| panic!("expected the trailing sideEffect() call.\nOutput:\n{output}"));

    assert!(
        side_at < dispose_at,
        "the statement following a `using` must run inside the disposal try \
         (before __disposeResources), not after disposal.\nOutput:\n{output}"
    );
    assert!(
        body.contains("const handle = ") && body.contains("__addDisposableResource("),
        "the `using` binding must stay a `const` initialized in place.\nOutput:\n{output}"
    );
    assert!(
        !body.contains("var handle;"),
        "the `using` binding must not be hoisted to a bare `var`.\nOutput:\n{output}"
    );
}

/// Two `using` declarations in a single-line body share one disposal env and
/// one `try`/`finally` (a single `__disposeResources`), so both resources are
/// tracked together and disposed in reverse order — never two independent
/// regions.
#[test]
fn single_line_two_usings_share_one_disposal_region() {
    let output = parse_lower_emit(
        "function region() { using first = { [Symbol.dispose]() {} }; using second = { [Symbol.dispose]() {} }; tail(); }",
        opts(),
    );

    let body = user_code(&output, "function region");

    assert_eq!(
        body.matches("__disposeResources").count(),
        1,
        "two single-line-body `using` declarations must share one disposal \
         region (one __disposeResources), not two.\nOutput:\n{output}"
    );
    assert_eq!(
        body.matches("= { stack: [], error: void 0, hasError: false }")
            .count(),
        1,
        "two single-line-body `using` declarations must share one disposal \
         env, not two.\nOutput:\n{output}"
    );
    let dispose_at = body.find("__disposeResources").unwrap();
    let tail_at = body
        .find("tail()")
        .unwrap_or_else(|| panic!("expected the trailing tail() call.\nOutput:\n{output}"));
    assert!(
        tail_at < dispose_at,
        "the trailing statement must run before disposal.\nOutput:\n{output}"
    );
}

/// The same invariant holds for an arrow-function body and a method body, and
/// for any user-chosen identifier — it is structural, not name-keyed.
#[test]
fn single_line_using_wrap_holds_for_arrow_and_method_and_renamed_binders() {
    let arrow = parse_lower_emit(
        "const run = () => { using res = { [Symbol.dispose]() {} }; work(res); };",
        opts(),
    );
    let arrow_body = user_code(&arrow, "const run");
    let arrow_dispose = arrow_body.find("__disposeResources").unwrap();
    assert!(
        arrow_body.find("work(res)").unwrap() < arrow_dispose && !arrow_body.contains("var res;"),
        "arrow single-line body must wrap the trailing statement in the \
         disposal try.\nOutput:\n{arrow}"
    );

    let method = parse_lower_emit(
        "class Owner { method() { using guard = { [Symbol.dispose]() {} }; finish(guard); } }",
        opts(),
    );
    let method_body = user_code(&method, "class Owner");
    let method_dispose = method_body.find("__disposeResources").unwrap();
    assert!(
        method_body.find("finish(guard)").unwrap() < method_dispose
            && !method_body.contains("var guard;"),
        "method single-line body must wrap the trailing statement in the \
         disposal try.\nOutput:\n{method}"
    );
}
