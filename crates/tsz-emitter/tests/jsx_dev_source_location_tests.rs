//! `react-jsxdev` `__source` line/column parity with tsc.
//!
//! tsc derives the `__source` `lineNumber`/`columnNumber` from the JSX
//! transform location range, whose start is `skipTrivia(source, node.pos)`.
//! For tsz's JSX nodes that is the opening `<` token position, not the
//! whitespace or newline before it.
//!
//! Regression coverage for issue #14778.

use tsz_common::common::ScriptTarget;
use tsz_emitter::emitter::JsxEmit;

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_print_named_with_opts;
use tsz_emitter::output::printer::PrintOptions;

fn emit_dev(source: &str) -> String {
    let opts = PrintOptions {
        jsx: JsxEmit::ReactJsxDev,
        target: ScriptTarget::ES2020,
        ..Default::default()
    };
    parse_and_print_named_with_opts("linebug.tsx", source, opts)
}

fn assert_source(output: &str, line: u32, col: u32) {
    let needle = format!("lineNumber: {line}, columnNumber: {col}");
    assert!(
        output.contains(&needle),
        "expected `__source` {needle}\nfull emit:\n{output}"
    );
}

#[test]
fn element_on_following_line_uses_opening_token() {
    // TypeScript skips trivia before building the JSX dev location range, so
    // the source points at the `<div>` rather than the previous `(`.
    let source = "const x = (\n    <div>hi</div>\n);\n";
    let output = emit_dev(source);
    assert!(
        output.contains("_jsxDEV("),
        "expected dev runtime call:\n{output}"
    );
    assert_source(&output, 2, 5);
}

#[test]
fn element_after_inline_whitespace_uses_opening_token() {
    // `const a =     <x/>;` reports the `<` token column, after skipping the
    // spaces that follow `=`.
    let source = "const a =     <x/>;\n";
    let output = emit_dev(source);
    assert_source(&output, 1, 15);
}

#[test]
fn self_closing_after_return_newline_uses_opening_token() {
    let source = "function f() {\n  return (\n    <br/>\n  );\n}\n";
    let output = emit_dev(source);
    assert_source(&output, 3, 5);
}

#[test]
fn fragment_after_leading_whitespace_uses_opening_token() {
    let source = "const f = (\n    <>hi</>\n);\n";
    let output = emit_dev(source);
    assert!(
        output.contains("_Fragment"),
        "expected fragment runtime:\n{output}"
    );
    assert_source(&output, 2, 5);
}

#[test]
fn nested_child_with_no_preceding_whitespace_points_at_tag() {
    // In JSX children, inter-element whitespace is JsxText (not trivia), so the
    // child element still reports its own `<`. A child placed immediately after
    // the parent's `>` must report the child's own column, unchanged.
    let source = "const x = <div><span/></div>;\n";
    let output = emit_dev(source);
    // Parent `<div>` token.
    assert_source(&output, 1, 11);
    // Child `<span/>` sits at col 16 (`const x = <div>` is 15 chars), with no
    // leading trivia before the token.
    assert_source(&output, 1, 16);
}

#[test]
fn nested_child_on_new_line_points_at_child_tag() {
    // Even when a child element is on its own line, the preceding indentation is
    // JsxText, so tsc anchors the child's `__source` at the child's `<`, not at
    // the parent's `>`.
    let source = "const x = (\n  <div>\n    <span/>\n  </div>\n);\n";
    let output = emit_dev(source);
    // Outer `<div>` token after trivia skipping.
    assert_source(&output, 2, 3);
    // Inner `<span/>`: line 3, col 5 (its own `<`).
    assert_source(&output, 3, 5);
}
