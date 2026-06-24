//! Regression coverage for hoisted temps in single-line async-lowered bodies.
//!
//! When an `async` function/arrow is lowered to
//! `__awaiter(..., function* () { ... })` (target ES2015/ES2016) and the source
//! body is a single line, the body's own downleveling (optional chaining,
//! nullish-coalescing assignment, `for await...of`) produces hoisted temps
//! (`var _a, _b;`). `tsc` keeps the single-line shape and splices the
//! declarations inline right after `function* () {`:
//! `function* () { var _a, _b; <body> }`.
//!
//! tsz previously emitted the inline statements but **dropped** the `var`
//! declaration, producing non-runnable strict-mode output (`ReferenceError: _a
//! is not defined`). These tests pin the inline declaration across the function
//! declaration, function expression, arrow block, arrow concise, parameter
//! forwarding, and captured-`arguments` shapes, with varied binder names.

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::output::printer::{PrintOptions, lower_and_print};
use tsz_parser::parser::ParserState;

fn emit_es2015(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let opts = PrintOptions {
        target: ScriptTarget::ES2015,
        module: ModuleKind::ES2015,
        remove_comments: true,
        ..PrintOptions::default()
    };
    lower_and_print(&parser.arena, root, opts).code
}

/// A single-line async function declaration whose body downlevels optional
/// chaining must declare the read-cache temps inline, not drop them.
#[test]
fn async_function_decl_optional_chaining_declares_temps_inline() {
    let output = emit_es2015("async function load(src: any) { return src?.head.tail?.(); }");

    assert!(
        output.contains("function* () { var _a, _b; return"),
        "optional-chaining temps must be spliced inline in the generator body.\nOutput:\n{output}"
    );
    // The pre-fix bug: the body referenced `_a`/`_b` with no declaration.
    assert!(
        !output.contains("function* () { return ("),
        "the generator body must not reference undeclared temps.\nOutput:\n{output}"
    );
}

/// A single-line async function expression behaves like the declaration form.
#[test]
fn async_function_expression_optional_chaining_declares_temps_inline() {
    let output =
        emit_es2015("const grab = async function (node: any) { return node?.left.right?.(); };");

    assert!(
        output.contains("function* () { var _a, _b; return"),
        "function-expression optional-chaining temps must be declared inline.\nOutput:\n{output}"
    );
}

/// A single-line async arrow with a block body declares its temps inline.
#[test]
fn async_arrow_block_optional_chaining_declares_temps_inline() {
    let output = emit_es2015("const pick = async (item: any) => { return item?.next.prev?.(); };");

    assert!(
        output.contains("function* () { var _a, _b; return"),
        "arrow block optional-chaining temps must be declared inline.\nOutput:\n{output}"
    );
}

/// A single-line async arrow with a concise expression body declares temps too.
#[test]
fn async_arrow_concise_optional_chaining_declares_temps_inline() {
    let output = emit_es2015("const peek = async (entry: any) => entry?.lo.hi?.();");

    assert!(
        output.contains("function* () { var _a, _b; return"),
        "arrow concise optional-chaining temps must be declared inline.\nOutput:\n{output}"
    );
}

/// Nullish-coalescing assignment (`??=`) read-cache temps must be declared too.
#[test]
fn async_function_nullish_assignment_declares_temp_inline() {
    let output = emit_es2015("async function seed(box: any) { box.slot ??= box?.fallback; }");

    assert!(
        output.contains("function* () { var _a;"),
        "nullish-assignment read-cache temp must be declared inline.\nOutput:\n{output}"
    );
}

/// A single-line `for await...of` body keeps the single-line shape with the
/// done/error/return/value temps spliced inline (matching tsc), rather than the
/// pre-fix forced multi-line expansion or dropped declarations.
#[test]
fn async_function_for_await_declares_temps_inline() {
    let output = emit_es2015(
        "declare function feed(): AsyncIterable<number>;\n\
         async function drain() { for await (const v of feed()) { v; } }",
    );

    assert!(
        output.contains("function* () { var _a, e_1, _b, _c; try {"),
        "for-await temps must be spliced inline in the single-line generator body.\nOutput:\n{output}"
    );
}

/// The same single-line `for await...of` shape in an arrow body.
#[test]
fn async_arrow_for_await_declares_temps_inline() {
    let output = emit_es2015(
        "declare function feed(): AsyncIterable<number>;\n\
         const run = async () => { for await (const v of feed()) { v; } };",
    );

    assert!(
        output.contains("function* () { var _a, e_1, _b, _c; try {"),
        "arrow for-await temps must be spliced inline.\nOutput:\n{output}"
    );
}

/// A default-initialized parameter forwards `arguments` into the generator; the
/// body's optional-chaining temps must still be declared inline.
#[test]
fn async_arrow_forwarded_params_optional_chaining_declares_temps_inline() {
    let output = emit_es2015("const wrap = async (cfg: any = {}) => { return cfg?.a.b?.(); };");

    assert!(
        output.contains("function* (cfg = {}) { var _a, _b; return"),
        "forwarded-param arrow optional-chaining temps must be declared inline.\nOutput:\n{output}"
    );
}

/// A negative control: a single-line async body that hoists no temps must stay
/// clean — the splice must not inject a spurious empty `var ;`.
#[test]
fn async_single_line_body_without_temps_stays_clean() {
    let output = emit_es2015("async function plain(v: number) { return v + 1; }");

    assert!(
        output.contains("function* () { return v + 1; }"),
        "a temp-free single-line body must not gain a spurious var declaration.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var ;"),
        "no empty var declaration should be emitted.\nOutput:\n{output}"
    );
}
