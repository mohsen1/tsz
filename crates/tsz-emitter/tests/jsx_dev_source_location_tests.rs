//! `react-jsxdev` `__source` line/column parity with tsc.
//!
//! tsc derives the `__source` `lineNumber`/`columnNumber` from the JSX
//! element/fragment node's trivia-inclusive full start (`node.pos`), which is
//! the position immediately after the previous token and INCLUDES any leading
//! whitespace/newlines. tsz anchors `Node::pos` at the `<` token start for
//! diagnostics, so the dev emit must instead use the captured `full_start`.
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
fn element_on_following_line_uses_full_start() {
    // The `<div>` sits on line 2 col 5, but tsc points `__source` at the
    // position right after `(` on line 1 (col 12).
    let source = "const x = (\n    <div>hi</div>\n);\n";
    let output = emit_dev(source);
    assert!(
        output.contains("_jsxDEV("),
        "expected dev runtime call:\n{output}"
    );
    assert_source(&output, 1, 12);
}

#[test]
fn element_after_inline_whitespace_uses_full_start() {
    // `const a =     <x/>;` — tsc reports the column right after `=` (col 10),
    // not the `<` token (col 15).
    let source = "const a =     <x/>;\n";
    let output = emit_dev(source);
    assert_source(&output, 1, 10);
}

#[test]
fn self_closing_after_return_newline_uses_full_start() {
    // A self-closing element after `return\n` reports the position right after
    // `return` (end of line 1), not the `<` on line 2.
    let source = "function f() {\n  return (\n    <br/>\n  );\n}\n";
    let output = emit_dev(source);
    // `return (` — the `(` ends line 2; full start of `<br/>` is right after
    // `(` on line 2, col 11.
    assert_source(&output, 2, 11);
}

#[test]
fn fragment_after_leading_whitespace_uses_full_start() {
    let source = "const f = (\n    <>hi</>\n);\n";
    let output = emit_dev(source);
    assert!(
        output.contains("_Fragment"),
        "expected fragment runtime:\n{output}"
    );
    assert_source(&output, 1, 12);
}

#[test]
fn nested_child_with_no_preceding_whitespace_points_at_tag() {
    // In JSX children, inter-element whitespace is JsxText (not trivia), so the
    // child element's full start equals its `<`. A child placed immediately
    // after the parent's `>` must report the child's own column, unchanged.
    let source = "const x = <div><span/></div>;\n";
    let output = emit_dev(source);
    // Parent `<div>` full start: right after `=` (col 10).
    assert_source(&output, 1, 10);
    // Child `<span/>` sits at col 16 (`const x = <div>` is 15 chars), with no
    // leading trivia, so its full start equals its token start.
    assert_source(&output, 1, 16);
}

#[test]
fn nested_child_on_new_line_points_at_child_tag() {
    // Even when a child element is on its own line, the preceding indentation is
    // JsxText, so tsc anchors the child's `__source` at the child's `<`, not at
    // the parent's `>`.
    let source = "const x = (\n  <div>\n    <span/>\n  </div>\n);\n";
    let output = emit_dev(source);
    // Outer `<div>`: full start right after `(` on line 1 (col 12).
    assert_source(&output, 1, 12);
    // Inner `<span/>`: line 3, col 5 (its own `<`).
    assert_source(&output, 3, 5);
}
