//! Regression coverage for malformed `for` header recovery.
//!
//! When tsc recovers `for () { // comment }`, it synthesizes the empty
//! `for (;;` header and prints the block-opening trailing comment before the
//! recovered closing `)`, while still preserving the original comment on the
//! `{` line.

use tsz_common::common::ScriptTarget;
use tsz_emitter::context::emit::EmitContext;
use tsz_emitter::emitter::{Printer as EmitterPrinter, PrinterOptions};
use tsz_emitter::lowering::LoweringPass;

#[path = "test_support.rs"]
mod test_support;

fn parse_lower_emit(source: &str, opts: PrinterOptions) -> String {
    let (parser, root) = test_support::parse_source(source);
    let ctx = EmitContext::with_options(opts.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, opts);
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

fn emit_es2015(source: &str) -> String {
    parse_lower_emit(
        source,
        PrinterOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    )
}

#[test]
fn recovered_empty_for_header_duplicates_line_comment_before_closing_paren() {
    let output = emit_es2015("for () { // error\n}\n");

    assert!(
        output.contains("for (;; // error\n) { // error\n}"),
        "Recovered empty `for ()` header should duplicate the block-opening line comment before `)`.\nOutput:\n{output}"
    );
}

#[test]
fn recovered_empty_for_header_duplicates_block_comment_before_closing_paren() {
    let output = emit_es2015("for () { /* error */\n}\n");

    assert!(
        output.contains("for (;; /* error */) { /* error */"),
        "Recovered empty `for ()` header should duplicate the block-opening block comment before `)`.\nOutput:\n{output}"
    );
}

#[test]
fn recovered_empty_for_header_without_body_open_comment_stays_plain() {
    let output = emit_es2015("for () {\n}\n");

    assert!(
        output.contains("for (;;) {\n}"),
        "Recovered empty `for ()` without a block-opening comment should stay plain.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("for (;; \n"),
        "Recovered empty `for ()` without a comment should not add header trivia.\nOutput:\n{output}"
    );
}

#[test]
fn valid_empty_for_header_does_not_duplicate_body_open_comment() {
    let output = emit_es2015("for (; ;) { // ok\n}\n");

    assert!(
        output.contains("for (;;) { // ok\n}"),
        "Valid empty `for (; ;)` header should preserve the block-opening comment only at the block.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("for (;; // ok\n)"),
        "Valid empty `for (; ;)` header should not use recovered-header comment duplication.\nOutput:\n{output}"
    );
}
