//! Tests for the `var Name = void 0;` reset that `tsc` emits when a
//! block-scoped enum or namespace is downleveled to ES5.
//!
//! Rule: a non-const, non-ambient enum or an instantiated namespace nested in a
//! control-flow / standalone block (not a function body, namespace body, or the
//! source-file top level) downlevels its hoisted `var` binding to
//! `var Name = void 0;` so a stale value cannot leak across re-entry. At ES2015+
//! the binding is upgraded to a properly block-scoped `let` with no reset.
//!
//! The cases vary the declaration's chosen identifier name on purpose: the fix
//! must key on AST structure, never on the spelling of the enum/namespace name.

use crate::context::emit::EmitContext;
use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
use crate::lowering::LoweringPass;
use tsz_common::ScriptTarget;
use tsz_parser::ParserState;

fn emit_with_target(source: &str, target: ScriptTarget) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let options = PrinterOptions {
        target,
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

fn emit_es5(source: &str) -> String {
    emit_with_target(source, ScriptTarget::ES5)
}

fn emit_es2015(source: &str) -> String {
    emit_with_target(source, ScriptTarget::ES2015)
}

#[test]
fn es5_enum_in_control_flow_block_resets_hoisted_var_to_void_zero() {
    // enum inside `if (true) { ... }` — a control-flow block, so the hoisted
    // `var` must be reset. The name `Color` exercises that the rule is keyed on
    // structure, not on a particular identifier spelling.
    let output =
        emit_es5("function f() {\n    if (true) {\n        enum Color { Red, Green }\n    }\n}\n");
    assert!(
        output.contains("var Color = void 0;"),
        "block-scoped enum should reset hoisted var to void 0.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var Color;"),
        "block-scoped enum should not emit a bare `var Color;`.\nOutput:\n{output}"
    );
}

#[test]
fn es5_enum_in_bare_block_resets_hoisted_var_to_void_zero() {
    // A different name (`Flags`) and a bare `{ ... }` standalone block.
    let output = emit_es5("function f() {\n    {\n        enum Flags { A, B }\n    }\n}\n");
    assert!(
        output.contains("var Flags = void 0;"),
        "enum in a bare block should reset hoisted var to void 0.\nOutput:\n{output}"
    );
}

#[test]
fn es5_enum_in_switch_case_resets_hoisted_var_to_void_zero() {
    // A switch `case` clause introduces a block scope without a wrapping Block
    // node; the hoisted var still needs the reset.
    let output = emit_es5(
        "function f(x: number) {\n    switch (x) {\n        case 1:\n            enum Mode { On, Off }\n            break;\n    }\n}\n",
    );
    assert!(
        output.contains("var Mode = void 0;"),
        "enum in a switch case should reset hoisted var to void 0.\nOutput:\n{output}"
    );
}

#[test]
fn es5_enum_directly_in_function_body_keeps_bare_var() {
    // A function body is a scope top, not a nested block: no reset.
    let output = emit_es5("function f() {\n    enum Inner { A, B }\n}\n");
    assert!(
        output.contains("var Inner;"),
        "function-body enum should keep a bare `var Inner;`.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var Inner = void 0;"),
        "function-body enum should not be reset to void 0.\nOutput:\n{output}"
    );
}

#[test]
fn es5_top_level_enum_keeps_bare_var() {
    // Top-level (source file) is a scope top: no reset.
    let output = emit_es5("enum Top { A, B }\n");
    assert!(
        output.contains("var Top;"),
        "top-level enum should keep a bare `var Top;`.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var Top = void 0;"),
        "top-level enum should not be reset to void 0.\nOutput:\n{output}"
    );
}

#[test]
fn es5_enum_in_namespace_body_keeps_bare_var() {
    // A namespace body (`MODULE_BLOCK`) is a scope top: no reset for the inner
    // enum (the namespace var itself is top-level here, also no reset).
    let output = emit_es5("namespace N {\n    enum NsEnum { A, B }\n}\n");
    assert!(
        output.contains("var NsEnum;"),
        "namespace-body enum should keep a bare `var NsEnum;`.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var NsEnum = void 0;"),
        "namespace-body enum should not be reset to void 0.\nOutput:\n{output}"
    );
}

#[test]
fn es5_namespace_in_control_flow_block_resets_hoisted_var_to_void_zero() {
    // The sibling case: a block-scoped instantiated namespace gets the same
    // reset. Name `Widget` proves structural keying.
    let output = emit_es5(
        "function f() {\n    if (true) {\n        namespace Widget { export var x = 1; }\n    }\n}\n",
    );
    assert!(
        output.contains("var Widget = void 0;"),
        "block-scoped namespace should reset hoisted var to void 0.\nOutput:\n{output}"
    );
}

#[test]
fn es5_top_level_namespace_keeps_bare_var() {
    let output = emit_es5("namespace TopNs { export var z = 1; }\n");
    assert!(
        output.contains("var TopNs;"),
        "top-level namespace should keep a bare `var TopNs;`.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var TopNs = void 0;"),
        "top-level namespace should not be reset to void 0.\nOutput:\n{output}"
    );
}

#[test]
fn es2015_block_scoped_enum_uses_let_without_void_zero() {
    // At ES2015+ the binding is a properly block-scoped `let`, so there is no
    // hoist and no `= void 0` reset.
    let output = emit_es2015(
        "function f() {\n    if (true) {\n        enum Color { Red, Green }\n    }\n}\n",
    );
    assert!(
        output.contains("let Color;"),
        "ES2015 block-scoped enum should use `let Color;`.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("void 0"),
        "ES2015 block-scoped enum must not emit a `void 0` reset.\nOutput:\n{output}"
    );
}

#[test]
fn es2015_block_scoped_namespace_uses_let_without_void_zero() {
    let output = emit_es2015(
        "function f() {\n    if (true) {\n        namespace Widget { export var x = 1; }\n    }\n}\n",
    );
    assert!(
        output.contains("let Widget;"),
        "ES2015 block-scoped namespace should use `let Widget;`.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("void 0"),
        "ES2015 block-scoped namespace must not emit a `void 0` reset.\nOutput:\n{output}"
    );
}
