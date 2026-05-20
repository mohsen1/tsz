//! Regression test for `import x = T;` script-mode lowering when `T` is a
//! top-level interface or type alias.
//!
//! In script mode (no top-level imports/exports — the file is not a
//! module), tsc preserves `var x = T;` for `import x = T` aliases that
//! target a top-level *interface* or *type alias* identifier. The runtime
//! emit is broken on purpose (the alias name resolves to nothing), but
//! tsc emits it because the file's globals can be consumed externally and
//! the alias name might be referenced or assigned later.
//!
//! Non-instantiated namespace targets are *not* preserved: tsc still
//! elides `import a = M;` when `M` is an empty namespace, so a
//! pre-existing `var a;` doesn't get shadowed by `var a = M;`.
//!
//! Source: `crates/tsz-emitter/src/emitter/module_emission/imports.rs`
//! (the `script_mode_preserves_alias` branch in
//! `emit_import_equals_declaration_inner`).

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

#[test]
fn script_mode_import_equals_to_interface_emits_var_alias() {
    let source = "interface I { id: number; }\nimport i = I;\n";
    let opts = PrinterOptions {
        target: ScriptTarget::ES2015,
        ..Default::default()
    };
    let output = parse_lower_emit(source, opts);

    assert!(
        output.contains("var i = I;"),
        "Script-mode import-equals to a top-level interface should emit `var i = I;`.\nOutput:\n{output}"
    );
}

#[test]
fn script_mode_import_equals_to_type_alias_emits_var_alias() {
    let source = "type T = number;\nimport t = T;\n";
    let opts = PrinterOptions {
        target: ScriptTarget::ES2015,
        ..Default::default()
    };
    let output = parse_lower_emit(source, opts);

    assert!(
        output.contains("var t = T;"),
        "Script-mode import-equals to a top-level type alias should emit `var t = T;`.\nOutput:\n{output}"
    );
}

#[test]
fn script_mode_import_equals_to_non_instantiated_namespace_still_elides() {
    // Counter-regression: tsc elides `import a = M` when `M` is an empty
    // namespace, even in script mode. The pre-existing `var a;` is not
    // duplicated by a runtime alias.
    let source = "var a;\nnamespace M { }\nimport a = M;\n";
    let opts = PrinterOptions {
        target: ScriptTarget::ES2015,
        ..Default::default()
    };
    let output = parse_lower_emit(source, opts);

    assert!(
        !output.contains("var a = M"),
        "Non-instantiated namespace alias must still be elided in script mode.\nOutput:\n{output}"
    );
}

#[test]
fn script_mode_import_equals_missing_trailing_entity_identifier_emits_alias() {
    let source = "namespace N { export type A = { value: string }; }\nimport x = N.A.\n";
    let opts = PrinterOptions {
        target: ScriptTarget::ES2015,
        ..Default::default()
    };
    let output = parse_lower_emit(source, opts);

    assert!(
        output.contains("var x = N.A.;"),
        "Recovered import-equals entity names with a missing final identifier should still emit the runtime alias.\nOutput:\n{output}"
    );
}

#[test]
fn type_only_import_equals_missing_trailing_entity_identifier_still_elides() {
    let source = "namespace N { export type A = { value: string }; }\nimport type x = N.A.\n";
    let opts = PrinterOptions {
        target: ScriptTarget::ES2015,
        ..Default::default()
    };
    let output = parse_lower_emit(source, opts);

    assert!(
        !output.contains("var x = N.A."),
        "`import type` aliases with recovered entity names should stay type-only.\nOutput:\n{output}"
    );
}

#[test]
fn script_mode_import_equals_literal_recovery_still_elides_alias_prefix() {
    let source = "import x = null\n";
    let opts = PrinterOptions {
        target: ScriptTarget::ES2015,
        ..Default::default()
    };
    let output = parse_lower_emit(source, opts);

    assert!(
        output.contains("null;"),
        "Recovered literal import-equals RHS should still emit the recovered expression.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var x = null"),
        "Recovered literal import-equals RHS should not gain an alias assignment prefix.\nOutput:\n{output}"
    );
}
