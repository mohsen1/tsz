//! Regression tests for the `emit_await_as_yield_await` async-generator emit
//! context leaking into nested function scopes.
//!
//! A down-leveled `async function*` sets an emit context in which `await`
//! lowers to `yield __await(x)` and a delegating `yield*` lowers to
//! `yield __await(yield* __asyncDelegator(__asyncValues(x)))`. That context
//! belongs only to the async generator's own body — a nested function
//! establishes its own async context. Without resetting the flag on descent,
//! a nested sync `function*`'s `yield* x` and a nested async function/arrow's
//! `await x` were wrongly emitted in the async-generator form.

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::output::printer::{PrintOptions, lower_and_print};
use tsz_parser::parser::ParserState;

fn emit(source: &str, target: ScriptTarget) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let opts = PrintOptions {
        target,
        module: ModuleKind::ESNext,
        remove_comments: true,
        ..PrintOptions::default()
    };
    lower_and_print(&parser.arena, root, opts).code
}

#[test]
fn nested_sync_generator_yield_star_stays_native_at_es2015() {
    // The inner sync generator's `yield* [1, 2]` is native at ES2015; it must
    // not inherit the enclosing async generator's `__asyncDelegator` lowering.
    let output = emit(
        "export async function* outer() {
            function* inner() { yield* [1, 2]; }
            yield inner();
        }",
        ScriptTarget::ES2015,
    );
    assert!(
        output.contains("function* inner() { yield* [1, 2]; }"),
        "nested sync generator yield* must stay native:\n{output}"
    );
    assert!(
        !output.contains("__asyncDelegator") && !output.contains("__asyncValues"),
        "nested sync generator must not pull async-iteration lowering:\n{output}"
    );
}

#[test]
fn nested_async_function_await_uses_plain_yield_at_es2015() {
    // The inner async (non-generator) function lowers `await x` to `yield x`
    // via __awaiter, never the async-generator `yield __await(x)` form.
    let output = emit(
        "export async function* stream() {
            async function pull() { return await Promise.resolve(7); }
            yield pull();
        }",
        ScriptTarget::ES2015,
    );
    assert!(
        output.contains("return yield Promise.resolve(7)"),
        "nested async function await must lower to plain `yield`:\n{output}"
    );
    assert!(
        !output.contains("yield __await(Promise.resolve(7))"),
        "nested async function await must not use the async-generator __await form:\n{output}"
    );
}

#[test]
fn nested_async_arrow_await_uses_plain_yield_at_es2015() {
    let output = emit(
        "export async function* feed() {
            const grab = async () => await Promise.resolve(9);
            yield grab();
        }",
        ScriptTarget::ES2015,
    );
    assert!(
        output.contains("return yield Promise.resolve(9)"),
        "nested async arrow await must lower to plain `yield`:\n{output}"
    );
    assert!(
        !output.contains("yield __await(Promise.resolve(9))"),
        "nested async arrow await must not use the async-generator __await form:\n{output}"
    );
}

#[test]
fn nested_async_generator_reestablishes_await_context_at_es2015() {
    // Positive control: a nested async generator is itself down-leveled, so its
    // own delegating `yield*` MUST use the `__asyncDelegator`/`__asyncValues`
    // form — proving the reset is scoped (save/restore) rather than permanent.
    let output = emit(
        "export async function* outer() {
            async function* inner(src: AsyncIterable<number>) { yield* src; }
            yield inner(null as any);
        }",
        ScriptTarget::ES2015,
    );
    assert!(
        output.contains("yield* __asyncDelegator(__asyncValues(src))"),
        "a nested async generator's own yield* must keep the delegator form:\n{output}"
    );
}

#[test]
fn outer_async_generator_body_still_uses_await_form_at_es2015() {
    // Regression guard: the async generator's own `await` still lowers to the
    // `yield __await(x)` form; only nested functions reset the context.
    let output = emit(
        "export async function* ticker(p: Promise<number>) { yield await p; }",
        ScriptTarget::ES2015,
    );
    assert!(
        output.contains("yield __await(p)"),
        "the async generator's own await must keep the __await form:\n{output}"
    );
}

#[test]
fn nested_sync_generator_yield_star_stays_native_at_es5() {
    // At ES5 a sync generator delegates via the `__generator` state machine and
    // `__values`, never the async-iteration helpers.
    let output = emit(
        "export async function* outer() {
            function* inner() { yield* [3, 4]; }
            yield inner();
        }",
        ScriptTarget::ES5,
    );
    assert!(
        output.contains("__values([3, 4])"),
        "ES5 nested sync generator yield* must delegate via __values:\n{output}"
    );
    assert!(
        !output.contains("__asyncDelegator") && !output.contains("__asyncValues"),
        "ES5 nested sync generator must not pull async-iteration helpers:\n{output}"
    );
}
