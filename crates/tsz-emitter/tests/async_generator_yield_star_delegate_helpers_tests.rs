//! Regression tests: a delegating `yield* x` inside a down-leveled async
//! generator must register the async-iteration runtime helpers
//! (`__asyncValues` + `__asyncDelegator`) in the emitted preamble, not just
//! reference them in the body.
//!
//! `tsc` lowers `yield* x` inside an `async function*` (target below ES2018) to
//! `yield __await(yield* __asyncDelegator(__asyncValues(x)))` (ES2015/ES2016) or
//! the `__generator` state-machine equivalent (ES5). Both forms *call*
//! `__asyncDelegator` and `__asyncValues`, so their helper definitions must be
//! emitted; otherwise the output references undefined names (a `ReferenceError`
//! at runtime). tsz previously emitted the calls but never marked the two
//! helpers, so their `var __asyncValues = ...` / `var __asyncDelegator = ...`
//! definitions were missing.
//!
//! The decision keys on the structural presence of a delegating `yield*` in the
//! generator's own body, never on identifier spelling — the tests vary binder
//! names to prove that.

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

const ASYNC_VALUES_DEF: &str = "var __asyncValues = (this && this.__asyncValues)";
const ASYNC_DELEGATOR_DEF: &str = "var __asyncDelegator = (this && this.__asyncDelegator)";
const AWAIT_DEF: &str = "var __await = (this && this.__await)";
const ASYNC_GENERATOR_DEF: &str = "var __asyncGenerator = (this && this.__asyncGenerator)";

/// Assert the four async-iteration helper definitions are present and emitted
/// in `tsc`'s canonical tslib order: `__asyncValues`, `__await`,
/// `__asyncDelegator`, `__asyncGenerator`.
fn assert_delegate_helpers_in_order(output: &str) {
    for def in [
        ASYNC_VALUES_DEF,
        ASYNC_DELEGATOR_DEF,
        AWAIT_DEF,
        ASYNC_GENERATOR_DEF,
    ] {
        assert!(
            output.contains(def),
            "missing helper definition `{def}`.\nOutput:\n{output}"
        );
    }
    let i_values = output.find(ASYNC_VALUES_DEF).unwrap();
    let i_await = output.find(AWAIT_DEF).unwrap();
    let i_delegator = output.find(ASYNC_DELEGATOR_DEF).unwrap();
    let i_async_gen = output.find(ASYNC_GENERATOR_DEF).unwrap();
    assert!(
        i_values < i_await && i_await < i_delegator && i_delegator < i_async_gen,
        "async-iteration helpers must emit in tsc order \
         (__asyncValues, __await, __asyncDelegator, __asyncGenerator).\nOutput:\n{output}"
    );
}

#[test]
fn delegating_yield_star_registers_async_iteration_helpers_es2015() {
    let output = emit(
        "export async function* f(g: AsyncIterable<number>) { yield* g; yield 1; }",
        ScriptTarget::ES2015,
    );
    assert_delegate_helpers_in_order(&output);
    assert!(
        output.contains("yield* __asyncDelegator(__asyncValues(g))"),
        "delegate body must route through the async iteration helpers.\nOutput:\n{output}"
    );
}

#[test]
fn delegating_yield_star_registers_async_iteration_helpers_es2017() {
    let output = emit(
        "export async function* f(g: AsyncIterable<number>) { yield* g; yield 1; }",
        ScriptTarget::ES2017,
    );
    assert_delegate_helpers_in_order(&output);
}

#[test]
fn delegating_yield_star_registers_async_iteration_helpers_es5() {
    let output = emit(
        "export async function* f(g: AsyncIterable<number>) { yield* g; yield 1; }",
        ScriptTarget::ES5,
    );
    // ES5 lowers through the `__generator` state machine but still calls the
    // async-iteration helpers, so their definitions must be present.
    assert!(
        output.contains(ASYNC_VALUES_DEF) && output.contains(ASYNC_DELEGATOR_DEF),
        "ES5 delegating async generator must emit __asyncValues/__asyncDelegator defs.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__asyncDelegator(__asyncValues(g))"),
        "ES5 delegate body must call the async iteration helpers.\nOutput:\n{output}"
    );
}

#[test]
fn delegating_yield_star_in_async_method_registers_helpers() {
    // Renamed binders (class/method/param) prove the decision is structural.
    let output = emit(
        "export class Container { async *stream(sourceSeq: AsyncIterable<string>) { yield* sourceSeq; } }",
        ScriptTarget::ES2015,
    );
    assert_delegate_helpers_in_order(&output);
}

#[test]
fn delegating_yield_star_over_call_expression_registers_helpers() {
    let output = emit(
        "declare function make(): AsyncIterable<number>;\n\
         export async function* f() { yield* make(); }",
        ScriptTarget::ES2015,
    );
    assert_delegate_helpers_in_order(&output);
}

#[test]
fn plain_async_generator_does_not_register_delegate_helpers() {
    // No delegating `yield*`: only __await + __asyncGenerator are needed.
    let output = emit(
        "export async function* f() { yield 1; yield 2; }",
        ScriptTarget::ES2015,
    );
    assert!(
        output.contains(AWAIT_DEF) && output.contains(ASYNC_GENERATOR_DEF),
        "plain async generator still needs __await + __asyncGenerator.\nOutput:\n{output}"
    );
    assert!(
        !output.contains(ASYNC_DELEGATOR_DEF),
        "plain async generator (no yield*) must not emit __asyncDelegator.\nOutput:\n{output}"
    );
    assert!(
        !output.contains(ASYNC_VALUES_DEF),
        "plain async generator (no yield*) must not emit __asyncValues.\nOutput:\n{output}"
    );
}

/// Assert the four async-iteration helper definitions emit in the order used
/// when a plain `yield`/`await` is reached before the delegating `yield*`:
/// `__await`, `__asyncValues`, `__asyncDelegator`, `__asyncGenerator`.
fn assert_delegate_helpers_await_first(output: &str) {
    for def in [
        ASYNC_VALUES_DEF,
        ASYNC_DELEGATOR_DEF,
        AWAIT_DEF,
        ASYNC_GENERATOR_DEF,
    ] {
        assert!(
            output.contains(def),
            "missing helper definition `{def}`.\nOutput:\n{output}"
        );
    }
    let i_await = output.find(AWAIT_DEF).unwrap();
    let i_values = output.find(ASYNC_VALUES_DEF).unwrap();
    let i_delegator = output.find(ASYNC_DELEGATOR_DEF).unwrap();
    let i_async_gen = output.find(ASYNC_GENERATOR_DEF).unwrap();
    assert!(
        i_await < i_values && i_values < i_delegator && i_delegator < i_async_gen,
        "when a plain yield/await precedes the delegating yield*, async-iteration \
         helpers must emit in tsc order (__await, __asyncValues, __asyncDelegator, \
         __asyncGenerator).\nOutput:\n{output}"
    );
}

#[test]
fn plain_yield_before_delegating_yield_star_puts_await_first_es2015() {
    // `yield 1` is lowered to `yield yield __await(1)`, which requests `__await`
    // before the later `yield* g` requests `__asyncValues`, so `tsc` emits
    // `__await` first. (Mirror of the `yield*`-first case, which is __asyncValues
    // first.) The decision is source-order structural, not identifier-based.
    let output = emit(
        "export async function* f(g: AsyncIterable<number>) { yield 1; yield* g; }",
        ScriptTarget::ES2015,
    );
    assert_delegate_helpers_await_first(&output);
}

#[test]
fn plain_yield_before_delegating_yield_star_puts_await_first_es2017() {
    let output = emit(
        "export async function* f(g: AsyncIterable<number>) { yield 1; yield* g; }",
        ScriptTarget::ES2017,
    );
    assert_delegate_helpers_await_first(&output);
}

#[test]
fn plain_yield_before_delegating_yield_star_puts_await_first_es5() {
    let output = emit(
        "export async function* f(g: AsyncIterable<number>) { yield 1; yield* g; }",
        ScriptTarget::ES5,
    );
    assert_delegate_helpers_await_first(&output);
}

#[test]
fn await_nested_in_delegate_operand_puts_await_first() {
    // `yield* wrap(await x)`: the `await` inside the delegate operand evaluates
    // before the delegation, so `tsc` requests `__await` before `__asyncValues`
    // even though the `yield*` token appears first in source.
    let output = emit(
        "declare function wrap(x: number): AsyncIterable<number>;\n\
         export async function* f() { yield* wrap(await Promise.resolve(1)); }",
        ScriptTarget::ES2015,
    );
    assert_delegate_helpers_await_first(&output);
}

#[test]
fn bare_delegating_yield_star_puts_async_values_first() {
    // No plain yield/await before the delegating `yield*`: `__asyncValues` leads.
    let output = emit(
        "export async function* f(g: AsyncIterable<number>) { yield* g; }",
        ScriptTarget::ES2015,
    );
    assert_delegate_helpers_in_order(&output);
}

/// Assert `__asyncValues` is registered (needed for a `for await…of`) and, when
/// present, ordered before `__asyncGenerator` — `tsc` emits its no-priority
/// async-iteration helpers ahead of the generator wrapper in request order.
fn assert_async_values_before_generator(output: &str) {
    for def in [ASYNC_VALUES_DEF, AWAIT_DEF, ASYNC_GENERATOR_DEF] {
        assert!(
            output.contains(def),
            "missing helper definition `{def}`.\nOutput:\n{output}"
        );
    }
    let i_values = output.find(ASYNC_VALUES_DEF).unwrap();
    let i_async_gen = output.find(ASYNC_GENERATOR_DEF).unwrap();
    assert!(
        i_values < i_async_gen,
        "`for await…of` in an async generator must register __asyncValues before \
         __asyncGenerator, not after it.\nOutput:\n{output}"
    );
}

#[test]
fn for_await_of_in_async_generator_puts_async_values_first_es2017() {
    // `for await` requests `__asyncValues` at loop setup, before the inner
    // `yield` requests `__await`, so `tsc` leads with `__asyncValues`. (No
    // `yield*`, so no `__asyncDelegator`.)
    let output = emit(
        "export async function* f(s: AsyncIterable<number>) { for await (const x of s) { yield x; } }",
        ScriptTarget::ES2017,
    );
    assert_async_values_before_generator(&output);
    let i_values = output.find(ASYNC_VALUES_DEF).unwrap();
    let i_await = output.find(AWAIT_DEF).unwrap();
    assert!(
        i_values < i_await,
        "for-await-of before any plain yield/await must lead with __asyncValues.\nOutput:\n{output}"
    );
    assert!(
        !output.contains(ASYNC_DELEGATOR_DEF),
        "for-await-of without a delegating yield* must not emit __asyncDelegator.\nOutput:\n{output}"
    );
}

#[test]
fn plain_yield_before_for_await_of_puts_await_first_es2017() {
    // A plain `yield` precedes the `for await`, so `__await` leads, but
    // `__asyncValues` must still precede `__asyncGenerator`.
    let output = emit(
        "export async function* f(s: AsyncIterable<number>) { yield 0; for await (const x of s) { yield x; } }",
        ScriptTarget::ES2017,
    );
    assert_async_values_before_generator(&output);
    let i_await = output.find(AWAIT_DEF).unwrap();
    let i_values = output.find(ASYNC_VALUES_DEF).unwrap();
    assert!(
        i_await < i_values,
        "a plain yield before the for-await-of must lead with __await.\nOutput:\n{output}"
    );
}

#[test]
fn for_await_of_before_delegating_yield_star_orders_all_four_helpers_es2017() {
    // `for await` (asyncValues) reached first, then a delegating `yield*`
    // (asyncDelegator): __asyncValues, __await, __asyncDelegator, __asyncGenerator.
    let output = emit(
        "export async function* f(s: AsyncIterable<number>) { for await (const x of s) { yield x; } yield* s; }",
        ScriptTarget::ES2017,
    );
    assert_delegate_helpers_in_order(&output);
}

#[test]
fn nested_sync_generators_yield_star_does_not_pull_async_delegate_helpers() {
    // The delegating `yield*` belongs to the inner *sync* generator, which uses
    // native `yield*` (ES2015+) and does not need the async iteration helpers.
    // The outer async generator has no delegating `yield*` of its own.
    let output = emit(
        "export async function* f() {\n\
             function* inner() { yield* [1, 2]; }\n\
             yield 1;\n\
         }",
        ScriptTarget::ES2015,
    );
    assert!(
        !output.contains(ASYNC_DELEGATOR_DEF) && !output.contains(ASYNC_VALUES_DEF),
        "a nested sync generator's yield* must not pull async delegate helpers into \
         the enclosing async generator.\nOutput:\n{output}"
    );
}
