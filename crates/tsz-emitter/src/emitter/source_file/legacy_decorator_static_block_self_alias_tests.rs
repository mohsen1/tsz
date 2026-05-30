//! Tests for legacy-decorator class-self alias publication.
//!
//! Structural rule: when a legacy-decorated (`@experimentalDecorators`) class
//! declaration references itself and the target supports native static blocks
//! (ES2022+), tsc publishes the `C_1` alias via a synthetic leading
//! `static { C_1 = this; }` block and emits `let C = class C { ... }` (plain).
//! At targets below ES2022 it keeps the outer-expression alias prefix
//! `let C = C_1 = class C { ... }`.

use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
use tsz_common::ScriptTarget;
use tsz_parser::ParserState;

fn emit(source: &str, target: ScriptTarget) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut printer = EmitterPrinter::with_options(
        &parser.arena,
        PrinterOptions {
            legacy_decorators: true,
            // Mirror the `@target: es2022` default so native static fields stay
            // in the class body (`static x = ...`) rather than lowering to
            // `static { this.x = ...; }`.
            use_define_for_class_fields: true,
            target,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

// ── ES2022+: internal static-block alias form ──────────────────────────────

#[test]
fn es2022_static_field_self_ref_publishes_alias_via_static_block() {
    let source = "declare var dec: any;\n@dec\nclass C1 {\n    static instance = new C1();\n}\n";
    let output = emit(source, ScriptTarget::ES2022);

    // Plain class expression — no outer-expression alias prefix.
    assert!(
        output.contains("let C1 = class C1 {"),
        "ES2022 decorated self-ref class should keep a plain class expression.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("let C1 = C1_1 = class"),
        "ES2022 should not use the pre-ES2022 outer-alias prefix.\nOutput:\n{output}"
    );
    // Synthetic leading static block publishes the alias.
    assert!(
        output.contains("static { C1_1 = this; }"),
        "ES2022 should publish the self-alias via a synthetic static block.\nOutput:\n{output}"
    );
    // Self-reference rewritten to the alias; decorate assignment carries it.
    assert!(
        output.contains("static instance = new C1_1();"),
        "Self-reference should be rewritten to the alias.\nOutput:\n{output}"
    );
    assert!(
        output.contains("C1 = C1_1 = __decorate("),
        "Decorate assignment should rebind through the alias.\nOutput:\n{output}"
    );
}

#[test]
fn es2022_static_block_self_ref_publishes_alias_via_static_block() {
    let source =
        "declare var dec: any;\n@dec\nclass C2 {\n    static {\n        new C2();\n    }\n}\n";
    let output = emit(source, ScriptTarget::ES2022);

    assert!(
        output.contains("static { C2_1 = this; }"),
        "Static-block self-ref should still publish the alias via a synthetic static block.\nOutput:\n{output}"
    );
    assert!(
        output.contains("new C2_1();"),
        "Self-reference inside a static block should be rewritten to the alias.\nOutput:\n{output}"
    );
    assert!(
        output.contains("C2 = C2_1 = __decorate("),
        "Decorate assignment should rebind through the alias.\nOutput:\n{output}"
    );
    // The file-level alias hoist must NOT leak into the user's static block body.
    assert!(
        !output.contains("var C2_1, "),
        "Alias hoist must not leak inside the static block body.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var C1_1") && !output.contains("var C2_1;\n        "),
        "Alias hoist must live at file top, not inside the static block.\nOutput:\n{output}"
    );
}

#[test]
fn es2022_static_method_self_ref_publishes_alias_via_static_block() {
    // Different member-name / class-name spellings prove the rule is structural,
    // not keyed on identifier text.
    let source = "declare var dec: any;\n@dec\nclass Widget {\n    static x() { return Widget.y; }\n    static y = 1;\n}\n";
    let output = emit(source, ScriptTarget::ES2022);

    assert!(
        output.contains("static { Widget_1 = this; }"),
        "Method self-ref should publish a renamed alias via a static block.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return Widget_1.y;"),
        "Method body self-reference should use the alias.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("let Widget = Widget_1 = class"),
        "ES2022 must not use the outer-alias prefix for the renamed class.\nOutput:\n{output}"
    );
}

#[test]
fn es2022_two_decorated_self_ref_classes_hoist_aliases_at_file_top() {
    // Regression guard: the per-class alias hoists must collect at file top and
    // not be consumed by a later class's native static block.
    let source = "declare var dec: any;\n@dec\nclass A {\n    static self = new A();\n}\n@dec\nclass B {\n    static { new B(); }\n}\n";
    let output = emit(source, ScriptTarget::ES2022);

    assert!(
        output.contains("var A_1, B_1;"),
        "Both decorated self-ref aliases should be hoisted together at file top.\nOutput:\n{output}"
    );
    assert!(
        output.contains("static { A_1 = this; }") && output.contains("static { B_1 = this; }"),
        "Each class should publish its own alias via a synthetic static block.\nOutput:\n{output}"
    );
    // No alias hoist should appear inside B's user static block.
    assert!(
        !output.contains("var A_1, B_1;\n        ") && !output.contains("{ var A_1"),
        "Alias hoist must not leak into a static block body.\nOutput:\n{output}"
    );
}

// ── Pre-ES2022: outer-expression alias prefix preserved (fallback) ─────────

#[test]
fn es2015_static_method_self_ref_keeps_outer_alias_prefix() {
    let source = "declare var dec: any;\n@dec\nclass Widget {\n    static x() { return Widget.y; }\n    static y = 1;\n}\n";
    let output = emit(source, ScriptTarget::ES2015);

    assert!(
        output.contains("let Widget = Widget_1 = class Widget {"),
        "Below ES2022 should keep the outer-expression alias prefix.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("static { Widget_1 = this; }"),
        "Below ES2022 has no native static blocks; no synthetic alias block.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return Widget_1.y;"),
        "Self-reference should still be rewritten to the alias.\nOutput:\n{output}"
    );
}

#[test]
fn es2022_undecorated_self_ref_class_does_not_gain_alias_block() {
    // Negative case: without class decorators there is no `C_1` alias machinery,
    // so no synthetic static block should appear.
    let source = "class Plain {\n    static instance = new Plain();\n}\n";
    let output = emit(source, ScriptTarget::ES2022);

    assert!(
        !output.contains("static { Plain_1 = this; }"),
        "Undecorated classes must not synthesize a self-alias static block.\nOutput:\n{output}"
    );
    assert!(
        output.contains("static instance = new Plain();"),
        "Undecorated self-reference should remain the plain class name.\nOutput:\n{output}"
    );
}
