//! Regression tests: a down-leveled `for..of` / `for await..of` must request
//! its own iteration helper (`__values` / `__asyncValues`) *after* it has
//! visited the iterable expression, so the helper preamble matches `tsc`.
//!
//! `tsc`'s `transformForOfStatement` / `transformForAwaitOfStatement` visit the
//! iterable first and only then request the loop helper. Because those helpers
//! have no `priority` field, `compareEmitHelpers` keeps them in request order,
//! so any helper the iterable itself pulls in is emitted *before* the loop
//! helper:
//!
//! * a spread iterable (`[...xs]`) pulls in `__read` / `__spreadArray`, which
//!   precede the loop's `__values`;
//! * an inline `async function*` iterable pulls in `__await` /
//!   `__asyncGenerator`, which precede the loop's `__asyncValues`.
//!
//! Conversely the binding-pattern `__read` (emitted for `for (const [a, b] of
//! …)`) is requested *while lowering the pattern*, i.e. after the loop helper,
//! so `__values` precedes that `__read`.
//!
//! tsz previously marked the loop helper before visiting the iterable, which
//! reversed all three orderings. The decision keys only on structural shape, so
//! the fixtures vary binder names to prove no identifier spelling is involved.

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::output::printer::{PrintOptions, lower_and_print};
use tsz_parser::parser::ParserState;

fn emit(source: &str, target: ScriptTarget, downlevel_iteration: bool) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let opts = PrintOptions {
        target,
        module: ModuleKind::ESNext,
        remove_comments: true,
        downlevel_iteration,
        ..PrintOptions::default()
    };
    lower_and_print(&parser.arena, root, opts).code
}

const VALUES_DEF: &str = "var __values = (this && this.__values)";
const READ_DEF: &str = "var __read = (this && this.__read)";
const SPREAD_ARRAY_DEF: &str = "var __spreadArray = (this && this.__spreadArray)";
const AWAIT_DEF: &str = "var __await = (this && this.__await)";
const ASYNC_GENERATOR_DEF: &str = "var __asyncGenerator = (this && this.__asyncGenerator)";
const ASYNC_VALUES_DEF: &str = "var __asyncValues = (this && this.__asyncValues)";

fn index_of(output: &str, def: &str) -> usize {
    output
        .find(def)
        .unwrap_or_else(|| panic!("missing helper definition `{def}`.\nOutput:\n{output}"))
}

#[test]
fn sync_for_of_over_spread_emits_values_after_read_and_spread_array() {
    // `[...source]` pulls in `__read` / `__spreadArray` while the iterable is
    // visited; the loop's `__values` is requested afterwards.
    let output = emit(
        "declare const source: number[];\n\
         export function walk() { for (const item of [...source, 1]) { void item; } }",
        ScriptTarget::ES5,
        true,
    );
    let i_read = index_of(&output, READ_DEF);
    let i_spread = index_of(&output, SPREAD_ARRAY_DEF);
    let i_values = index_of(&output, VALUES_DEF);
    assert!(
        i_read < i_values && i_spread < i_values,
        "expected `__read`/`__spreadArray` before `__values`.\nOutput:\n{output}"
    );
}

#[test]
fn sync_for_of_binding_pattern_emits_values_before_destructuring_read() {
    // No spread in the iterable, so the only `__read` comes from lowering the
    // binding pattern — requested after `__values`.
    let output = emit(
        "declare const pairs: [number, string][];\n\
         export function walk() { for (const [head, tail] of pairs) { void head; void tail; } }",
        ScriptTarget::ES5,
        true,
    );
    let i_values = index_of(&output, VALUES_DEF);
    let i_read = index_of(&output, READ_DEF);
    assert!(
        i_values < i_read,
        "expected `__values` before the binding-pattern `__read`.\nOutput:\n{output}"
    );
}

#[test]
fn for_await_over_inline_async_generator_emits_async_values_last() {
    // The inline `async function*` iterable pulls in `__await` /
    // `__asyncGenerator`; the loop's `__asyncValues` is requested afterwards.
    let output = emit(
        "export async function drive() {\n\
           for await (const tick of (async function*() { yield 1; })()) { void tick; }\n\
         }",
        ScriptTarget::ES2017,
        false,
    );
    let i_await = index_of(&output, AWAIT_DEF);
    let i_async_gen = index_of(&output, ASYNC_GENERATOR_DEF);
    let i_async_values = index_of(&output, ASYNC_VALUES_DEF);
    assert!(
        i_await < i_async_values && i_async_gen < i_async_values,
        "expected `__await`/`__asyncGenerator` before `__asyncValues`.\nOutput:\n{output}"
    );
}
