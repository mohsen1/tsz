//! Tests for the ambient-aware `--importHelpers` tslib class-emit scan.
//!
//! Rule under test: a class that is ambient — carrying the `declare` modifier,
//! or nested inside an ambient `declare namespace`/`declare module` — produces
//! no runtime emit, so it must never trigger the synthesized
//! `var tslib_1 = require("tslib");` import. A genuinely emitted derived class
//! on an es5 target still must.

use crate::context::emit::EmitContext;
use crate::emitter::{ModuleKind, Printer as EmitterPrinter, PrinterOptions};
use crate::lowering::LoweringPass;
use tsz_common::ScriptTarget;
use tsz_parser::ParserState;

fn emit_cjs_es5_import_helpers(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        module: ModuleKind::CommonJS,
        import_helpers: true,
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

#[test]
fn ambient_declare_class_extends_does_not_require_tslib_es5() {
    // A top-level `declare class D extends C` is ambient: no runtime emit, so
    // the `__extends` helper is never used and tslib must not be required.
    let source = "export {};\ndeclare class C { }\ndeclare class D extends C { }\n";

    let output = emit_cjs_es5_import_helpers(source);

    assert!(
        !output.contains("require(\"tslib\")"),
        "Ambient declare-class heritage must not pull in a tslib import.\nOutput:\n{output}"
    );
}

#[test]
fn class_inside_declare_namespace_extends_does_not_require_tslib_es5() {
    // Classes nested in a `declare namespace` are ambient; the whole namespace
    // body is erased, so no member can reference a tslib helper.
    let source = "export {};\ndeclare namespace N {\n\tclass C { }\n\tclass D extends C { }\n}\n";

    let output = emit_cjs_es5_import_helpers(source);

    assert!(
        !output.contains("require(\"tslib\")"),
        "Classes inside a declare-namespace must not pull in a tslib import.\nOutput:\n{output}"
    );
}

#[test]
fn class_inside_declare_namespace_extends_renamed_does_not_require_tslib_es5() {
    // Same rule with different identifiers: the fix must key off the ambient
    // structure, not specific names.
    let source = "export {};\ndeclare namespace Outer {\n\tclass Base { }\n\tclass Sub extends Base { }\n}\n";

    let output = emit_cjs_es5_import_helpers(source);

    assert!(
        !output.contains("require(\"tslib\")"),
        "Renamed declare-namespace classes must not pull in a tslib import.\nOutput:\n{output}"
    );
}

#[test]
fn emitted_derived_class_still_requires_tslib_es5() {
    // Negative control: a real (non-ambient) derived class on es5 genuinely
    // lowers to `__extends`, so the tslib import must still be emitted. This
    // proves the ambient guard does not over-suppress.
    let source = "export {};\nclass C { }\nclass D extends C { }\n";

    let output = emit_cjs_es5_import_helpers(source);

    assert!(
        output.contains("require(\"tslib\")"),
        "An emitted es5 derived class must still require tslib for __extends.\nOutput:\n{output}"
    );
}
