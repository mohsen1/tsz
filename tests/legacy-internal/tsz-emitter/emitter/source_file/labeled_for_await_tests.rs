//! Tests for label placement on downleveled labeled `for await...of` loops.
//!
//! A label on a `for await...of` loop that is downleveled below ES2018 must
//! attach to the inner lowered `for` loop (the real iteration statement), not
//! the wrapping `try`. Otherwise `continue <label>` / `break <label>` target a
//! non-iteration label and the emitted JavaScript is a `SyntaxError`.
//!
//! The cases below vary the label name and the loop-binding name so the
//! assertions prove the structural rule, not a specific spelling.

use crate::context::emit::EmitContext;
use crate::emitter::{ModuleKind, Printer as EmitterPrinter, PrinterOptions};
use crate::lowering::LoweringPass;
use tsz_common::ScriptTarget;

fn emit_downleveled(source: &str, target: ScriptTarget) -> String {
    let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let options = PrinterOptions {
        target,
        module: ModuleKind::ES2015,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer =
        EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

/// The label index in the emitted output, relative to the `try {` and the inner
/// `for (`. The label must come after `try {` (inside the try) and immediately
/// before the inner `for`.
fn assert_label_on_inner_for(output: &str, label: &str) {
    let label_decl = format!("{label}:");
    let label_pos = output
        .find(&label_decl)
        .unwrap_or_else(|| panic!("label `{label}:` should be emitted.\nOutput:\n{output}"));
    let try_pos = output.find("try {").unwrap_or_else(|| {
        panic!("downleveled for-await should emit `try {{`.\nOutput:\n{output}")
    });
    // The for-await-of for-loop header always starts with `for (var`.
    let for_pos = output[label_pos..]
        .find("for (var")
        .map(|rel| label_pos + rel)
        .unwrap_or_else(|| {
            panic!("inner lowered `for` should follow the label.\nOutput:\n{output}")
        });

    assert!(
        try_pos < label_pos,
        "label `{label}:` must be inside the `try` (try at {try_pos}, label at {label_pos}).\nOutput:\n{output}"
    );
    // Nothing but optional whitespace/comments between label and `for`.
    let between = output[label_pos + label_decl.len()..for_pos].trim();
    assert!(
        between.is_empty(),
        "label `{label}:` must sit directly on the inner `for` loop, but found `{between}` between them.\nOutput:\n{output}"
    );
}

#[test]
fn labeled_for_await_continue_targets_inner_for_es2017() {
    let source = "async function f5() {\n    let y: any;\n    outer: for await (const x of y) {\n        continue outer;\n    }\n}\n";
    let output = emit_downleveled(source, ScriptTarget::ES2017);

    assert!(
        !output.contains("outer: try"),
        "label must not attach to the wrapping `try`.\nOutput:\n{output}"
    );
    assert_label_on_inner_for(&output, "outer");
    assert!(
        output.contains("continue outer;"),
        "continue should target the loop label.\nOutput:\n{output}"
    );
}

#[test]
fn labeled_for_await_continue_targets_inner_for_es2015() {
    let source = "async function f5() {\n    let y: any;\n    outer: for await (const x of y) {\n        continue outer;\n    }\n}\n";
    let output = emit_downleveled(source, ScriptTarget::ES2015);

    assert!(
        !output.contains("outer: try"),
        "label must not attach to the wrapping `try` at es2015.\nOutput:\n{output}"
    );
    assert_label_on_inner_for(&output, "outer");
}

#[test]
fn labeled_for_await_break_targets_inner_for_renamed() {
    // Different label name and binding name to prove the rule is structural.
    let source = "async function g() {\n    let src: any;\n    loopLabel: for await (let item of src) {\n        break loopLabel;\n    }\n}\n";
    let output = emit_downleveled(source, ScriptTarget::ES2017);

    assert!(
        !output.contains("loopLabel: try"),
        "renamed label must not attach to the wrapping `try`.\nOutput:\n{output}"
    );
    assert_label_on_inner_for(&output, "loopLabel");
    assert!(
        output.contains("break loopLabel;"),
        "break should target the loop label.\nOutput:\n{output}"
    );
}

#[test]
fn native_for_await_es2018_keeps_label_on_for() {
    // Negative/fallback case: at ES2018 the loop is native, not downleveled, so
    // the label stays on the `for await` and no `try` wrapper is introduced.
    let source = "async function f5() {\n    let y: any;\n    outer: for await (const x of y) {\n        continue outer;\n    }\n}\n";
    let output = emit_downleveled(source, ScriptTarget::ES2018);

    assert!(
        output.contains("outer: for await"),
        "native for-await should keep the label on the loop.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("__asyncValues"),
        "ES2018 native for-await must not downlevel to the async-iterator helper.\nOutput:\n{output}"
    );
}
