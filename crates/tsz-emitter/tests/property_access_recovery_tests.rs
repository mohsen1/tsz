//! Integration tests for property-access emit error recovery.
//!
//! When the parser encounters a `.` not followed by an identifier (e.g. EOF,
//! a newline + brace, or a newline + reserved word), it synthesizes a missing
//! identifier and emits TS1003 "Identifier expected." The emitter must mirror
//! tsc's whitespace handling around the missing name:
//!
//! - When the source has a newline between the dot and the next token (e.g.
//!   `bar.\n}`), tsc breaks the dot onto its own line and pushes the
//!   following statement (`;`) to the next indented line:
//!   ```js
//!   bar.
//!   ;
//!   ```
//! - When the source has no newline (e.g. `var p2 = window. ` at EOF), tsc
//!   keeps the trailing `;` on the same line as the dot:
//!   ```js
//!   var p2 = window.;
//!   ```
//!
//! This guards both layouts so future emit changes can't regress one shape
//! while fixing the other.
//!
//! See:
//! - `crates/tsz-emitter/src/emitter/expressions/access.rs`
//!   (`emit_property_access`, missing-name newline branch)

use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_print_with_opts;

fn print_es2015(source: &str) -> String {
    parse_and_print_with_opts(source, PrintOptions::es6())
}

/// Source `var p2 = window. ` (TypeScript test
/// `incompleteDottedExpressionAtEOF`) — the source has only trailing
/// whitespace (no newline) after the dot. tsc emits everything on one line:
/// `var p2 = window.;`. We must NOT inject an extra newline before the `;`.
#[test]
fn dotted_expression_at_eof_keeps_trailing_semicolon_on_same_line() {
    let source = "var p2 = window. ";
    let output = print_es2015(source);
    // The dot must be followed by `;` on the same line — no `\n` between
    // `.` and `;`.
    assert!(
        output.contains("window.;"),
        "expected `window.;` on a single line; output:\n{output}"
    );
    assert!(
        !output.contains("window.\n"),
        "no newline should follow the dot when the source had none; output:\n{output}"
    );
}

/// Source `bar.\n}` (TypeScript test `parse1.ts`) — there IS a newline
/// between the dot and the close-brace. tsc preserves that line break:
/// `bar.\n    ;`. The dot stays on its own line and the synthetic `;`
/// drops to the next indented line.
#[test]
fn dotted_expression_followed_by_newline_breaks_to_new_line() {
    let source = "var bar = 42;\nfunction foo() {\n bar.\n}\n";
    let output = print_es2015(source);
    // The dot must be followed by a newline, then the `;` on its own line.
    // We assert the substring so future indentation-tweak commits stay
    // resilient — what matters is that `bar.\n` precedes `;`, not the exact
    // amount of leading whitespace before the `;`.
    assert!(
        output.contains("bar.\n"),
        "expected `bar.` followed by newline; output:\n{output}"
    );
    // The `;` should be on a separate line from `bar.`.
    let dot_line_end = output.find("bar.\n").expect("bar. with newline");
    let after_dot = &output[dot_line_end + "bar.\n".len()..];
    assert!(
        after_dot.trim_start().starts_with(';'),
        "expected `;` to follow on the next line; output:\n{output}"
    );
}

/// Source `class C { test() { this.\n} } var x = new C();`
/// (TypeScript test `classAbstractCrashedOnce`) — the dot is the last
/// non-whitespace token in a method body, with a newline before the
/// closing braces. tsc emits the dot then breaks to a new line for the
/// synthetic `;`.
#[test]
fn dotted_expression_in_method_body_breaks_to_new_line() {
    let source = "class C {\n    test() {\n        this.\n    }\n}\n";
    let output = print_es2015(source);
    assert!(
        output.contains("this.\n"),
        "expected `this.` followed by newline; output:\n{output}"
    );
    let dot_line_end = output.find("this.\n").expect("this. with newline");
    let after_dot = &output[dot_line_end + "this.\n".len()..];
    assert!(
        after_dot.trim_start().starts_with(';'),
        "expected `;` to follow on the next line; output:\n{output}"
    );
}
