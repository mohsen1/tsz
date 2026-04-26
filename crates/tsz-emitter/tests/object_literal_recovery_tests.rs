//! Integration tests for object-literal emit error recovery.
//!
//! When the parser encounters an unexpected token in property-name position
//! (e.g. extra commas: `{ x: 0,, }`), it emits a `TS1136 Property assignment
//! expected` diagnostic and synthesizes a placeholder `SHORTHAND_PROPERTY_ASSIGNMENT`.
//! The emitter must NOT print stray commas/text from these zero-width
//! placeholders. This guards the parser/emitter coordination that fixes
//! the `parseErrorDoubleCommaInCall` conformance test.
//!
//! See:
//! - `crates/tsz-parser/src/parser/state_expressions_literals.rs`
//!   (`parse_property_name`)
//! - `crates/tsz-emitter/src/emitter/expressions/literals.rs`
//!   (`emit_object_literal` skip-empty-shorthand guard)

use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_print_with_opts;

fn print_es2015(source: &str) -> String {
    parse_and_print_with_opts(source, PrintOptions::es6())
}

/// Source `Boolean({ x: 0,, });` (TypeScript test
/// `parseErrorDoubleCommaInCall.ts`) must emit the same `Boolean({ x: 0, });`
/// shape that tsc produces — no stray `,,` placeholder line.
#[test]
fn double_comma_in_call_does_not_emit_stray_commas() {
    let source = "Boolean({\n    x: 0,,\n});\n";
    let output = print_es2015(source);
    assert!(
        !output.contains(",,"),
        "double comma should not survive emit; output:\n{output}"
    );
    assert!(
        output.contains("x: 0"),
        "first property must still be emitted; output:\n{output}"
    );
}

/// Trailing `,,}` at end of object literal must collapse to a single trailing
/// comma (or none) — never produce extra empty lines with commas.
#[test]
fn trailing_double_comma_collapses() {
    let source = "var o = { a: 1,, };\n";
    let output = print_es2015(source);
    assert!(
        !output.contains(",,"),
        "trailing double comma should not survive emit; output:\n{output}"
    );
    assert!(
        output.contains("a: 1"),
        "valid property must still be emitted; output:\n{output}"
    );
}

/// Object literal followed by close-brace immediately after a comma in source
/// (`{ a: 1, }`) must continue to emit normally — this guards that the
/// recovery fix doesn't disturb the legitimate trailing-comma path.
#[test]
fn legitimate_trailing_comma_preserved() {
    let source = "var o = { a: 1, b: 2, };\n";
    let output = print_es2015(source);
    assert!(
        output.contains("a: 1"),
        "first property must be emitted; output:\n{output}"
    );
    assert!(
        output.contains("b: 2"),
        "second property must be emitted; output:\n{output}"
    );
}
