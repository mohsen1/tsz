//! Integration tests for prefix-update emit error recovery.
//!
//! When the parser encounters a prefix `++`/`--` followed by another unary
//! operator that cannot be a left-hand side (`delete`, another `++`/`--`),
//! it preserves the outer update with a missing operand and leaves the
//! inner expression to start a fresh statement. The JS emitter must print
//! the bare operator (e.g. `++;`) followed by the inner statement, so that
//! conformance baselines like
//! `tests/cases/conformance/parser/ecmascript5/Expressions/parserUnaryExpression5.ts`
//! and `parserS7.9_A5.7_T1.ts` (Sputnik) round-trip correctly.
//!
//! See:
//! - `crates/tsz-parser/src/parser/state_expressions.rs`
//!   (`parse_unary_expression` `++`/`--` recovery branch)
//! - `crates/tsz-emitter/src/emitter/expressions/core/private_fields.rs`
//!   (`emit_prefix_unary`)

use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_print_with_opts;

fn print_es2015(source: &str) -> String {
    parse_and_print_with_opts(source, PrintOptions::es6())
}

/// Source `++ delete foo.bar` (TypeScript test
/// `parserUnaryExpression5.ts`) must emit `++;` followed by
/// `delete foo.bar;` — the outer `++` keeps a missing operand and the
/// inner `delete` becomes its own statement.
#[test]
fn prefix_update_before_delete_emits_bare_update_then_delete() {
    let source = "++ delete foo.bar\n";
    let output = print_es2015(source);
    let plus_idx = output
        .find("++;")
        .unwrap_or_else(|| panic!("expected `++;` in output:\n{output}"));
    let delete_idx = output
        .find("delete foo.bar")
        .unwrap_or_else(|| panic!("expected `delete foo.bar` in output:\n{output}"));
    assert!(
        plus_idx < delete_idx,
        "`++;` must precede `delete foo.bar`; output:\n{output}"
    );
}

/// Source `++\n++y;` must emit `++;` followed by `++y;`. The outer `++`
/// keeps a missing operand and the inner `++y` becomes its own statement.
#[test]
fn prefix_update_followed_by_prefix_update_emits_two_statements() {
    let source = "++\n++y;\n";
    let output = print_es2015(source);
    let outer_idx = output
        .find("++;")
        .unwrap_or_else(|| panic!("expected bare `++;` in output:\n{output}"));
    let inner_idx = output
        .find("++y")
        .unwrap_or_else(|| panic!("expected `++y` in output:\n{output}"));
    assert!(
        outer_idx < inner_idx,
        "outer `++;` must precede inner `++y`; output:\n{output}"
    );
}

/// Sputnik `S7.9_A5.7_T1`: `var z=\nx\n++\n++\ny\n` — after the `var z = x;`
/// initializer, the emitter must print `++;` followed by `++y;`.
#[test]
fn sputnik_variable_followed_by_double_prefix_update_emits_bare_then_inner() {
    let source = "var x=0, y=0;\nvar z=\nx\n++\n++\ny\n";
    let output = print_es2015(source);
    assert!(
        output.contains("var z = x;"),
        "expected `var z = x;`; output:\n{output}"
    );
    let outer_idx = output
        .find("++;")
        .unwrap_or_else(|| panic!("expected bare `++;` in output:\n{output}"));
    let inner_idx = output
        .find("++y")
        .unwrap_or_else(|| panic!("expected `++y` in output:\n{output}"));
    assert!(
        outer_idx < inner_idx,
        "outer `++;` must precede `++y;`; output:\n{output}"
    );
}
