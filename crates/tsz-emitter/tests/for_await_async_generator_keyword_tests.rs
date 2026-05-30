//! Regression tests for the implicit-await keyword emitted by the downlevel
//! `for await...of` transform inside an async generator.
//!
//! Rule: when a `for await...of` is lowered inside an `async function*` that is
//! itself downleveled to a `function*` wrapped in `__asyncGenerator` (target
//! below ES2018), the implicit awaits on the async-iterator protocol calls
//! (`iter.next()`, `iter.return()`) must be emitted as `yield __await(...)`,
//! not as a literal `await`. A literal `await` inside the generator body is a
//! `SyntaxError`. This mirrors how an explicit `await` expression is lowered in
//! the same context.

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::output::printer::{PrintOptions, lower_and_print};
use tsz_parser::parser::ParserState;

fn emit(source: &str, target: ScriptTarget) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let opts = PrintOptions {
        target,
        module: ModuleKind::CommonJS,
        remove_comments: true,
        ..PrintOptions::default()
    };
    lower_and_print(&parser.arena, root, opts).code
}

/// Helper asserting the loop transform never emits a bare `await` keyword,
/// which would be a `SyntaxError` inside a `function*` body. The two spellings
/// the transform can produce are `<temp> = await <iter>.next()` (loop step) and
/// `(...)) await <temp>.call(<iter>)` (return cleanup). The `__await(` helper
/// call and the helper's own definition are unaffected by these checks.
fn assert_no_bare_await(output: &str) {
    assert!(
        !output.contains(" = await "),
        "async-generator for-await loop step must not emit a bare `await`.\nOutput:\n{output}"
    );
    assert!(
        !output.contains(")) await "),
        "async-generator for-await return cleanup must not emit a bare `await`.\nOutput:\n{output}"
    );
}

#[test]
fn async_generator_for_await_lowers_next_to_yield_await_es2015() {
    let output = emit(
        "async function* g(source: any) {
            for await (const item of source) { item; }
        }",
        ScriptTarget::ES2015,
    );

    assert!(
        output.contains("__asyncGenerator"),
        "async generator below ES2018 should lower via __asyncGenerator.\nOutput:\n{output}"
    );
    assert!(
        output.contains("yield __await(source_1.next())"),
        "the iterator `.next()` step must be `yield __await(...)` in an async generator.\nOutput:\n{output}"
    );
    assert!(
        output.contains("yield __await(_b.call(source_1))"),
        "the `.return()` cleanup must be `yield __await(...)` in an async generator.\nOutput:\n{output}"
    );
    assert_no_bare_await(&output);
}

#[test]
fn async_generator_for_await_lowers_next_to_yield_await_es2017() {
    let output = emit(
        "async function* g(source: any) {
            for await (const item of source) { item; }
        }",
        ScriptTarget::ES2017,
    );

    assert!(
        output.contains("yield __await(source_1.next())"),
        "ES2017 async generator must also use `yield __await(...)`.\nOutput:\n{output}"
    );
    assert_no_bare_await(&output);
}

/// The fix must be keyed on the async-generator lowering mode, not on any
/// identifier spelling. Renaming the parameter/iteration variable must keep the
/// `yield __await(...)` form.
#[test]
fn async_generator_for_await_keyword_is_name_independent() {
    let output = emit(
        "async function* gen(xs: any) {
            for await (const value of xs) { value; }
        }",
        ScriptTarget::ES2015,
    );

    assert!(
        output.contains("yield __await(xs_1.next())"),
        "renamed iteration source must still emit `yield __await(...)`.\nOutput:\n{output}"
    );
    assert_no_bare_await(&output);
}

/// Negative case: an ordinary `async function` (not a generator) downleveled
/// below ES2017 lowers via `__awaiter`, where the implicit await must be a
/// plain `yield` with NO `__await` wrapper. The fix must not regress this.
#[test]
fn async_non_generator_for_await_uses_plain_yield() {
    let output = emit(
        "async function f(y: any) {
            for await (const x of y) { x; }
        }",
        ScriptTarget::ES2015,
    );

    assert!(
        output.contains("__awaiter"),
        "ordinary async function should lower via __awaiter.\nOutput:\n{output}"
    );
    assert!(
        output.contains("yield y_1.next()"),
        "non-generator async for-await must emit plain `yield iter.next()`.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("yield __await(y_1.next())"),
        "non-generator async for-await must NOT wrap in __await.\nOutput:\n{output}"
    );
}
