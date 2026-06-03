//! Regression tests: TC39 (non-legacy) ES5 decorated named-export classes
//! inside a top-level `using` block must not be double-indented.
//!
//! `render_simple_tc39_decorated_class_es5` produces a fully pre-indented
//! string (embedding its own `base_indent`). When that string is written to
//! the output via the transform dispatch, the writer's `ensure_indent()` must
//! NOT prepend a second level of indentation. Previously the dispatch used
//! `write(&output)` which triggered `ensure_indent()` at line-start, yielding
//! e.g. `exports.C =     C = function ()` (4 extra spaces) instead of the
//! correct `exports.C = C = function ()`.
//!
//! Structural rule: the transform dispatch writes pre-indented TC39 ES5
//! decorated class blocks with `write_raw_text`, not `write`, so the
//! `base_indent` already present in the string is the sole indentation source.

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::context::emit::EmitContext;
use tsz_emitter::emitter::{Printer as EmitterPrinter, PrinterOptions};
use tsz_emitter::lowering::LoweringPass;

#[path = "test_support.rs"]
mod test_support;

fn emit(source: &str, module: ModuleKind, target: ScriptTarget) -> String {
    let opts = PrinterOptions {
        module,
        target,
        no_emit_helpers: true,
        ..Default::default()
    };
    let (parser, root) = test_support::parse_source(source);
    let ctx = EmitContext::with_options(opts.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, opts);
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

const SRC_NAMED: &str =
    "export {};\ndeclare var dec: any;\nusing before = null;\n@dec\nexport class C {\n}\n";

/// CommonJS + ES5: no extra indentation inside the CJS try-block.
#[test]
fn cjs_es5_tc39_named_export_in_using_no_extra_indent() {
    let out = emit(SRC_NAMED, ModuleKind::CommonJS, ScriptTarget::ES5);
    assert!(
        out.contains("exports.C = C = function () {"),
        "TC39 ES5 named-export class in CJS using-block must assign without extra indent.\n{out}"
    );
    assert!(
        !out.contains("exports.C =  "),
        "must not have spaces between `exports.C =` and `C =`.\n{out}"
    );
}

/// `ESNext` + ES5: `C = function ()` at the correct one-level indent.
#[test]
fn esnext_es5_tc39_named_export_in_using_correct_indent() {
    let out = emit(SRC_NAMED, ModuleKind::ESNext, ScriptTarget::ES5);
    // ESNext has no inline export prefix; the class is emitted as `C = function () {`.
    // The line must start with exactly one indent level (4 spaces), not two (8 spaces).
    assert!(
        out.contains("    C = function () {") && !out.contains("        C = function () {"),
        "TC39 ES5 named-export class in ESNext using-block must be at one indent level.\n{out}"
    );
}

/// System + ES5: `exports_1("C", C = function () {` on one clean line.
#[test]
fn system_es5_tc39_named_export_in_using_no_extra_indent() {
    let out = emit(SRC_NAMED, ModuleKind::System, ScriptTarget::ES5);
    assert!(
        out.contains("exports_1(\"C\", C = function () {"),
        "TC39 ES5 named-export class in System using-block must not have extra spaces.\n{out}"
    );
    assert!(
        !out.contains("exports_1(\"C\",  "),
        "must not have extra spaces after `exports_1(\"C\",`.\n{out}"
    );
}

/// Top-level (no using block): the fix must not change top-level emission.
/// At top-level (indent=0), `write` and `write_raw_text` are identical since
/// `ensure_indent()` writes nothing. tsc uses the two-statement pattern
/// `var C = …(); exports.C = C;` at top-level, not the inline-export pattern.
#[test]
fn cjs_es5_tc39_top_level_class_unaffected() {
    // No `using` — class is at indent=0, no double-indent was ever possible.
    let src = "declare var dec: any;\n@dec\nexport class C {\n}\n";
    let out = emit(src, ModuleKind::CommonJS, ScriptTarget::ES5);
    assert!(
        out.contains("var C = function () {"),
        "Top-level TC39 ES5 class must emit the standard `var C = function` form.\n{out}"
    );
    assert!(
        out.contains("exports.C = C;"),
        "Top-level TC39 ES5 class must still re-export after the fix.\n{out}"
    );
}
