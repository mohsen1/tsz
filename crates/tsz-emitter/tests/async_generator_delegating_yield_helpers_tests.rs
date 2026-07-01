//! Regression tests for helper registration when a down-leveled
//! `async function*` body contains a delegating `yield* x`.
//!
//! Rule: when an `async function*` whose target down-levels the async generator
//! (target `< ES2018`) contains a delegating `yield* x` that binds to *that*
//! generator, `tsc` lowers it to
//! `yield __await(yield* __asyncDelegator(__asyncValues(x)))` (ES2015+) or the
//! `__generator` state-machine equivalent (ES5). The emit therefore registers
//! `__asyncValues`/`__asyncDelegator` in addition to `__await`/`__asyncGenerator`,
//! in `tsc`'s canonical tslib order (`__asyncValues`, `__await`,
//! `__asyncDelegator`, `__asyncGenerator`). Without the registration the emitted
//! module references two undefined names — a `ReferenceError` at runtime.
//!
//! The decision keys on the structural presence of a delegating `yield*` in the
//! generator's own body, never on identifier spelling or rendered output; a
//! `yield*` inside a nested function-like binds to that inner function and must
//! not be attributed here. Binder names vary across the cases below so no logic
//! can key on a particular spelling.

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

/// Position of a helper *definition* (`var __name = ...`) in the preamble, or
/// `None` when it is not registered.
fn def_pos(output: &str, name: &str) -> Option<usize> {
    output.find(&format!("var {name} = "))
}

fn assert_registered(output: &str, name: &str) {
    assert!(
        def_pos(output, name).is_some(),
        "expected helper `{name}` to be registered in the preamble.\nOutput:\n{output}"
    );
}

/// Asserts the four async-iteration helper definitions appear in `tsc`'s
/// canonical tslib order: `__asyncValues`, `__await`, `__asyncDelegator`,
/// `__asyncGenerator`.
fn assert_canonical_async_iteration_order(output: &str) {
    let names = [
        "__asyncValues",
        "__await",
        "__asyncDelegator",
        "__asyncGenerator",
    ];
    let mut last = 0usize;
    for name in names {
        assert_registered(output, name);
        let pos = def_pos(output, name).unwrap();
        assert!(
            pos >= last,
            "helper `{name}` is out of canonical tslib order.\nOutput:\n{output}"
        );
        last = pos;
    }
}

#[test]
fn es2015_delegating_yield_registers_iteration_helpers_in_canonical_order() {
    let output = emit(
        "export async function* f(g: AsyncIterable<number>) { yield* g; yield 1; }",
        ScriptTarget::ES2015,
    );
    assert_canonical_async_iteration_order(&output);
    assert!(
        output.contains("yield __await(yield* __asyncDelegator(__asyncValues(g)))"),
        "the delegating `yield*` body must lower through both iteration helpers.\nOutput:\n{output}"
    );
}

#[test]
fn es2016_delegating_yield_registers_iteration_helpers() {
    let output = emit(
        "async function* stream(source: AsyncIterable<string>) { yield* source; }",
        ScriptTarget::ES2016,
    );
    assert_canonical_async_iteration_order(&output);
}

#[test]
fn es2017_delegating_yield_registers_iteration_helpers() {
    let output = emit(
        "async function* pipe(upstream: AsyncIterable<string>) { yield* upstream; }",
        ScriptTarget::ES2017,
    );
    assert_canonical_async_iteration_order(&output);
}

#[test]
fn es5_delegating_yield_registers_iteration_helpers_alongside_generator() {
    let output = emit(
        "async function* drain(src: AsyncIterable<number>) { yield* src; }",
        ScriptTarget::ES5,
    );
    // ES5 down-levels via the `__generator` state machine but still pulls in the
    // async-iteration helpers for the delegating `yield*`.
    assert_registered(&output, "__generator");
    assert_registered(&output, "__asyncValues");
    assert_registered(&output, "__await");
    assert_registered(&output, "__asyncDelegator");
    assert_registered(&output, "__asyncGenerator");
    // The ES2018-tier async helpers precede the ES2015 iteration tier (`__values`).
    if let (Some(av), Some(vals)) = (
        def_pos(&output, "__asyncValues"),
        def_pos(&output, "__values"),
    ) {
        assert!(
            av < vals,
            "ES2018-tier `__asyncValues` must precede ES2015-tier `__values`.\nOutput:\n{output}"
        );
    }
}

#[test]
fn async_generator_method_delegating_yield_registers_helpers() {
    let output = emit(
        "class Repo { async *rows(cursor: AsyncIterable<number>) { yield* cursor; } }",
        ScriptTarget::ES2015,
    );
    assert_canonical_async_iteration_order(&output);
}

#[test]
fn call_expression_delegate_registers_helpers() {
    let output = emit(
        "declare function make(): AsyncIterable<number>;\
         async function* chain() { yield* make(); }",
        ScriptTarget::ES2015,
    );
    assert_canonical_async_iteration_order(&output);
}

#[test]
fn plain_async_generator_without_delegating_yield_omits_iteration_helpers() {
    let output = emit(
        "async function* counter() { yield 1; yield 2; }",
        ScriptTarget::ES2015,
    );
    assert_registered(&output, "__await");
    assert_registered(&output, "__asyncGenerator");
    assert!(
        def_pos(&output, "__asyncValues").is_none()
            && def_pos(&output, "__asyncDelegator").is_none(),
        "a non-delegating async generator must not pull in the iteration helpers.\nOutput:\n{output}"
    );
}

#[test]
fn nested_sync_generator_yield_star_not_attributed_to_outer_async_generator() {
    // The `yield*` belongs to the nested SYNC `function*`, which binds `yield`
    // to itself; the outer async generator has no delegating `yield*` of its own.
    let output = emit(
        "async function* outer() { function* inner() { yield* [1, 2]; } yield 3; }",
        ScriptTarget::ES2015,
    );
    assert_registered(&output, "__await");
    assert_registered(&output, "__asyncGenerator");
    assert!(
        def_pos(&output, "__asyncValues").is_none()
            && def_pos(&output, "__asyncDelegator").is_none(),
        "a nested sync generator's `yield*` must not register the async-iteration \
         helpers on the outer async generator.\nOutput:\n{output}"
    );
}

#[test]
fn native_async_generator_at_es2018_needs_no_iteration_helpers() {
    let output = emit(
        "async function* passthrough(g: AsyncIterable<number>) { yield* g; }",
        ScriptTarget::ES2018,
    );
    assert!(
        def_pos(&output, "__asyncValues").is_none()
            && def_pos(&output, "__asyncDelegator").is_none()
            && def_pos(&output, "__asyncGenerator").is_none()
            && def_pos(&output, "__await").is_none(),
        "at ES2018+ the async generator and `yield*` are native; no helpers needed.\nOutput:\n{output}"
    );
    assert!(
        output.contains("yield*"),
        "native `yield*` must be preserved at ES2018+.\nOutput:\n{output}"
    );
}
