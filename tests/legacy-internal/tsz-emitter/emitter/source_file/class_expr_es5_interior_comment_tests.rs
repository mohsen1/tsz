//! Regression tests for interior-comment handling when an ES6 class
//! expression is downleveled to the ES5 IIFE form.
//!
//! Structural rule: when an ES5 class expression is lowered to an IIFE by the
//! sub-emitter (which prints interior member comments itself by reading the
//! source text), the main emitter must advance its global comment cursor past
//! every comment inside that class expression's source range. Otherwise those
//! interior comments (e.g. a constructor's JSDoc) remain pending and are
//! re-emitted as a leaked leading/trailing comment after the statement,
//! duplicating them.
//!
//! These cases vary the class/base names, named vs anonymous, and with/without
//! `extends` to prove the fix keys on the lowered node shape — not on any
//! particular identifier spelling.

use crate::context::emit::EmitContext;
use crate::emitter::{ModuleKind, Printer as EmitterPrinter, PrinterOptions};
use crate::lowering::LoweringPass;
use tsz_common::ScriptTarget;
use tsz_parser::ParserState;

fn emit_es5_cjs(file: &str, source: &str) -> String {
    let mut parser = ParserState::new(file.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        module: ModuleKind::CommonJS,
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

/// The constructor JSDoc must appear exactly once (inside the IIFE), never as a
/// trailing duplicate after `}())`.
fn count_param_jsdoc(output: &str) -> usize {
    output.matches("@param {number} p").count()
}

#[test]
fn named_export_assigned_class_expression_does_not_duplicate_ctor_jsdoc() {
    // `module.exports = class Thing { ... }` — named class expression.
    let source = "module.exports = class Thing {\n    /**\n     * @param {number} p\n     */\n    constructor(p) {\n        this.t = 12 + p;\n    }\n}\n";
    let output = emit_es5_cjs("index.js", source);

    assert_eq!(
        count_param_jsdoc(&output),
        1,
        "constructor JSDoc must be emitted exactly once (inside the IIFE).\nOutput:\n{output}"
    );
    assert!(
        output.contains("function Thing(p)"),
        "class expression must lower to an ES5 constructor function.\nOutput:\n{output}"
    );
    // The IIFE close must not be immediately followed by a leaked JSDoc block.
    assert!(
        !output.contains("}());\n/**"),
        "no JSDoc should leak after the IIFE close.\nOutput:\n{output}"
    );
}

#[test]
fn anonymous_export_assigned_class_expression_does_not_duplicate_ctor_jsdoc() {
    // `module.exports = class { ... }` — anonymous class expression. The
    // synthesized constructor name differs from the named case, proving the fix
    // is not keyed to a specific identifier.
    let source = "module.exports = class {\n    /**\n     * @param {number} p\n     */\n    constructor(p) {\n        this.t = 12 + p;\n    }\n}\n";
    let output = emit_es5_cjs("index.js", source);

    assert_eq!(
        count_param_jsdoc(&output),
        1,
        "constructor JSDoc must be emitted exactly once for anonymous class.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("}());\n/**"),
        "no JSDoc should leak after the anonymous IIFE close.\nOutput:\n{output}"
    );
}

#[test]
fn extending_export_assigned_class_expression_does_not_duplicate_ctor_jsdoc() {
    // Class expression with `extends` — exercises the derived-constructor IIFE
    // path (different base name `Base`/`Widget` to vary spelling).
    let source = "module.exports = class Widget extends Base {\n    /**\n     * @param {number} p\n     */\n    constructor(p) {\n        super();\n        this.t = 12 + p;\n    }\n}\n";
    let output = emit_es5_cjs("index.js", source);

    assert_eq!(
        count_param_jsdoc(&output),
        1,
        "constructor JSDoc must be emitted exactly once for a derived class expression.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__extends"),
        "derived class expression must use the __extends helper at ES5.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("}\n/**"),
        "no JSDoc should leak after the derived IIFE.\nOutput:\n{output}"
    );
}

#[test]
fn class_expression_without_interior_comment_is_unaffected() {
    // Negative/fallback case: a class expression with no interior comments must
    // emit no stray comments and still lower correctly.
    let source = "module.exports = class Plain {\n    constructor(p) {\n        this.t = 12 + p;\n    }\n}\n";
    let output = emit_es5_cjs("index.js", source);

    assert_eq!(
        count_param_jsdoc(&output),
        0,
        "no JSDoc exists in source, so none should be emitted.\nOutput:\n{output}"
    );
    assert!(
        output.contains("function Plain(p)"),
        "class expression must still lower to an ES5 constructor function.\nOutput:\n{output}"
    );
}
