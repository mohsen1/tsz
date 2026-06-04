//! Structural emit tests for CommonJS export ordering of a top-level
//! `export class` lowered to an ES5 IIFE that also carries trailing
//! computed-property-name side-effect statements.
//!
//! Structural rule: when an `export class` is lowered to an ES5
//! `var X = (function () { ... }());` IIFE and the class declares
//! erased computed-named members whose key expressions are NOT
//! side-effect-free, `tsc` emits the deferred CommonJS export assignment
//! `exports.X = X;` immediately AFTER the class IIFE statement and BEFORE
//! the trailing computed-property side-effect statements. tsz must match
//! this order (it mirrors the ES2015+ ordering in `emit_es6.rs`).
//!
//! The rule is keyed on the AST/IR shape (export class -> ES5 IIFE +
//! computed-property side effects), not on any identifier spelling, so the
//! tests below vary the class name, the key-source object name, and the
//! member key while asserting the same ordering.

use crate::context::emit::EmitContext;
use crate::emitter::{ModuleKind, Printer as EmitterPrinter, PrinterOptions};
use crate::lowering::LoweringPass;
use tsz_common::ScriptTarget;
use tsz_parser::ParserState;

fn emit_commonjs(source: &str, target: ScriptTarget, use_define_for_class_fields: bool) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let options = PrinterOptions {
        target,
        module: ModuleKind::CommonJS,
        use_define_for_class_fields,
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

/// Index of the FIRST line equal to (trimmed) `needle`, or `None`.
fn line_index_eq(output: &str, needle: &str) -> Option<usize> {
    output.lines().position(|l| l.trim() == needle)
}

/// Index of the first line that (trimmed) starts with `prefix`, or `None`.
fn line_index_starts_with(output: &str, prefix: &str) -> Option<usize> {
    output.lines().position(|l| l.trim().starts_with(prefix))
}

// --- Reported repro shape (es5, useDefineForClassFields=false) ---

#[test]
fn export_class_es5_export_assignment_precedes_computed_side_effects() {
    // The instance computed member `[K.name]` is erased (no `=` initializer,
    // udf=false) but its key expression `K.name` is not side-effect-free, so
    // it is emitted as a trailing side-effect statement after the IIFE.
    let source = "const K = { name: 'name' } as const;\n\
                  export class Exported {\n\
                  \x20\x20\x20\x20static [K.name]: number;\n\
                  \x20\x20\x20\x20[K.name]: string;\n\
                  }\n";
    let output = emit_commonjs(source, ScriptTarget::ES5, false);

    let export_line = line_index_starts_with(&output, "exports.Exported = Exported;");
    let side_effect_line = line_index_starts_with(&output, "K.name,");

    assert!(
        export_line.is_some(),
        "Expected a deferred `exports.Exported = Exported;` line.\nOutput:\n{output}"
    );
    assert!(
        side_effect_line.is_some(),
        "Expected a trailing computed-name side-effect statement `K.name, ...`.\nOutput:\n{output}"
    );
    assert!(
        export_line < side_effect_line,
        "`exports.Exported = Exported;` must be emitted BEFORE the trailing \
         computed-property side-effect statement.\nOutput:\n{output}"
    );
}

// --- Renamed class / key-source / member key: proves the rule is keyed on the
// shape, not on the spelling `Exported`/`K`/`name`. ---

#[test]
fn export_class_es5_renamed_symbols_keep_export_before_side_effects() {
    let source = "const Names = { len: 'len' } as const;\n\
                  export class Widget {\n\
                  \x20\x20\x20\x20static [Names.len]: number;\n\
                  \x20\x20\x20\x20[Names.len]: string;\n\
                  }\n";
    let output = emit_commonjs(source, ScriptTarget::ES5, false);

    let export_line = line_index_starts_with(&output, "exports.Widget = Widget;");
    let side_effect_line = line_index_starts_with(&output, "Names.len,");

    assert!(
        export_line.is_some() && side_effect_line.is_some(),
        "Renamed symbols must still produce the export line and the trailing \
         side-effect statement.\nOutput:\n{output}"
    );
    assert!(
        export_line < side_effect_line,
        "Renaming the class/key-source/member must NOT change the export-before-\
         side-effects ordering (rule is structural).\nOutput:\n{output}"
    );
}

// --- No trailing computed side effects: the deferred export still lands right
// after the class IIFE. Negative-shape guard so the handoff doesn't drop the
// assignment when there is nothing to order it against. ---

#[test]
fn export_class_es5_without_computed_members_still_emits_export() {
    let source = "export class Plain {\n\
                  \x20\x20\x20\x20m() {}\n\
                  }\n";
    let output = emit_commonjs(source, ScriptTarget::ES5, false);

    assert!(
        line_index_starts_with(&output, "exports.Plain = Plain;").is_some(),
        "A plain `export class` must still emit its CommonJS export assignment.\nOutput:\n{output}"
    );
    assert!(
        line_index_eq(&output, "var Plain = /** @class */ (function () {").is_some()
            || output.contains("var Plain ="),
        "The class must be lowered to an ES5 IIFE binding.\nOutput:\n{output}"
    );
}

#[test]
fn export_alias_before_es5_class_emits_alias_before_class_export() {
    let source = "export { Renamed as Alias };\nexport class Renamed {}\n";
    let output = emit_commonjs(source, ScriptTarget::ES5, false);

    let alias_line = line_index_starts_with(&output, "exports.Alias = Renamed;");
    let class_line = line_index_starts_with(&output, "exports.Renamed = Renamed;");
    assert!(
        alias_line.is_some() && class_line.is_some(),
        "ES5 class export aliases should both emit at the class IIFE boundary.\nOutput:\n{output}"
    );
    assert!(
        alias_line < class_line,
        "A source-earlier alias export must precede the direct class export in ES5 output.\nOutput:\n{output}"
    );
}

#[test]
fn export_alias_before_es2015_class_keeps_native_class_export_order() {
    let source = "export { Later as Alias };\nexport class Later {}\n";
    let output = emit_commonjs(source, ScriptTarget::ES2015, false);

    let class_line = line_index_starts_with(&output, "exports.Later = Later;");
    let alias_line = line_index_starts_with(&output, "exports.Alias = Later;");
    assert!(
        class_line.is_some() && alias_line.is_some(),
        "ES2015 class export and alias export should both emit once.\nOutput:\n{output}"
    );
    assert!(
        class_line < alias_line,
        "ES2015 native class output should keep the direct class export before the later alias assignment.\nOutput:\n{output}"
    );
}

#[test]
fn es5_commonjs_exported_class_private_storage_hoists_before_preamble() {
    let source = "export class Box {\n    #value: unknown;\n}\n";
    let output = emit_commonjs(source, ScriptTarget::ES5, false);

    let marker_pos = output
        .find("Object.defineProperty(exports, \"__esModule\", { value: true });")
        .expect("CommonJS ES module marker should be emitted");
    let export_init_pos = output
        .find("exports.Box = void 0;")
        .expect("CommonJS export initializer should be emitted");
    let class_pos = output
        .find("var Box = /** @class */")
        .expect("ES5 class IIFE should be emitted");
    let export_assign_pos = output
        .find("exports.Box = Box;")
        .expect("CommonJS export assignment should follow the class");
    let storage_pos = output
        .find("var _Box_value;")
        .expect("private storage var should be emitted before the preamble");
    let storage_init_pos = output
        .find("_Box_value = new WeakMap();")
        .expect("private storage init should follow the export assignment");

    assert!(
        storage_pos < marker_pos
            && marker_pos < export_init_pos
            && export_init_pos < class_pos
            && class_pos < export_assign_pos
            && export_assign_pos < storage_init_pos,
        "Private storage declarations for exported ES5 classes should hoist before the CJS preamble.\nOutput:\n{output}"
    );
}
