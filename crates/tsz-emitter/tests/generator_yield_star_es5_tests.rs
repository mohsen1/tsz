//! Regression tests for `yield*` (delegated yield) down-leveling in the ES5
//! `__generator` state machine.
//!
//! A delegating `yield* expr` must lower to the `[5 /*yield**/, __values(expr)]`
//! op (op code 5 drives the delegate's iterator protocol), not the plain
//! `[4 /*yield*/, expr]` op that yields the operand as a single value. The
//! `__values` iterator helper must also be emitted. Asserting on the op-code
//! structure (not specific identifiers) keeps these guards name-agnostic.

use tsz_common::common::ScriptTarget;
use tsz_emitter::output::printer::{PrintOptions, lower_and_print};
use tsz_parser::parser::ParserState;

fn emit_es5(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let opts = PrintOptions {
        target: ScriptTarget::ES5,
        remove_comments: true,
        ..PrintOptions::default()
    };
    lower_and_print(&parser.arena, root, opts).code
}

fn emit_es2015(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let opts = PrintOptions {
        target: ScriptTarget::ES2015,
        remove_comments: true,
        ..PrintOptions::default()
    };
    lower_and_print(&parser.arena, root, opts).code
}

#[test]
fn yield_star_over_array_uses_delegated_op_and_values_helper() {
    let output = emit_es5("function* gen() { yield* [2, 3]; }");
    assert!(
        output.contains("[5 /*yield**/, __values([2, 3])]"),
        "`yield*` must lower to the delegated op (5) wrapping the operand in `__values`.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("[4 /*yield*/, [2, 3]]"),
        "`yield*` must not lower to a plain `yield` of the whole operand.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var __values ="),
        "The `__values` iterator helper must be emitted for a delegated `yield*`.\nOutput:\n{output}"
    );
}

#[test]
fn yield_star_over_generator_call_delegates() {
    let output = emit_es5("function* outer() { yield* inner(); } function* inner() { yield 1; }");
    assert!(
        output.contains("[5 /*yield**/, __values(inner())]"),
        "`yield*` over a generator call must delegate via `__values`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var __values ="),
        "The `__values` helper must be emitted.\nOutput:\n{output}"
    );
}

#[test]
fn yield_star_renamed_binders_keep_delegated_structure() {
    // Anti-hardcoding: every user binder is renamed; the lowering decision keys
    // on the structural `yield*` (asterisk) marker, not any identifier text.
    let output =
        emit_es5("function* produce() { yield* makeSeq(); } function* makeSeq() { yield 7; }");
    assert!(
        output.contains("[5 /*yield**/, __values(makeSeq())]"),
        "Delegated structure must be name-agnostic.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("[4 /*yield*/, makeSeq()]"),
        "Renamed binders must not change `yield*` into a plain `yield`.\nOutput:\n{output}"
    );
}

#[test]
fn plain_yield_keeps_value_op_and_omits_values_helper() {
    // Negative control: a non-delegating `yield` stays op 4 and does not pull in
    // the `__values` iterator helper.
    let output = emit_es5("function* gen() { yield 1; yield 2; }");
    assert!(
        output.contains("[4 /*yield*/, 1]") && output.contains("[4 /*yield*/, 2]"),
        "Plain `yield` must keep the value op (4).\nOutput:\n{output}"
    );
    assert!(
        !output.contains("/*yield**/"),
        "Plain `yield` must not emit the delegated op.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var __values ="),
        "Plain `yield` must not pull in the `__values` helper.\nOutput:\n{output}"
    );
}

#[test]
fn yield_star_value_is_resumed_from_sent() {
    // `const total = yield* delegate()` resumes the delegate's return value via
    // `_a.sent()`, exactly like a plain yield's resume point.
    let output = emit_es5(
        "function* gen() { const total = yield* delegate(); return total; }
         function* delegate() { return 1; }",
    );
    assert!(
        output.contains("[5 /*yield**/, __values(delegate())]"),
        "`yield*` used as a value still delegates via `__values`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_a.sent()"),
        "The delegated value must be resumed from `_a.sent()`.\nOutput:\n{output}"
    );
}

#[test]
fn native_generator_keeps_yield_star_without_helper() {
    // At ES2015+ the generator is native: `yield*` is preserved verbatim and no
    // `__generator`/`__values` lowering is performed.
    let output = emit_es2015("function* gen() { yield* [2, 3]; }");
    assert!(
        output.contains("yield* [2, 3]"),
        "Native generators must preserve `yield*`.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("__values") && !output.contains("/*yield**/"),
        "Native generators must not emit down-level helpers/ops.\nOutput:\n{output}"
    );
}
