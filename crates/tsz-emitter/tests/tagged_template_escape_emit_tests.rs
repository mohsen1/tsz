//! Regression coverage for target-gated tagged-template invalid escape lowering.

use tsz_common::common::ScriptTarget;
use tsz_emitter::context::emit::EmitContext;
use tsz_emitter::emitter::{Printer as EmitterPrinter, PrinterOptions};
use tsz_emitter::lowering::LoweringPass;

#[path = "test_support.rs"]
mod test_support;

fn parse_lower_emit(source: &str, target: ScriptTarget) -> String {
    let (parser, root) = test_support::parse_source(source);
    let opts = PrinterOptions {
        target,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(opts.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, opts);
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

#[test]
fn es2015_lowers_only_tagged_templates_with_invalid_escapes() {
    let output = parse_lower_emit(
        r#"
function tag(str: any, ...args: any[]): any { return str; }
const ok = tag`a${1}b`;
const bad = tag`${1}\x`;
"#,
        ScriptTarget::ES2015,
    );

    assert!(
        output.contains("const ok = tag `a${1}b`;"),
        "Valid tagged templates should stay native for ES2015.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "const bad = tag(__makeTemplateObject([\"\", void 0], [\"\", \"\\\\x\"]), 1);"
        ),
        "Invalid tagged template escape should lower through __makeTemplateObject below ES2018.\nOutput:\n{output}"
    );
}

#[test]
fn es5_template_expression_preserves_invalid_raw_escapes_in_string_text() {
    let output = parse_lower_emit(
        r#"
const y = `\u{hello} ${100} \xtraordinary ${200} wonderful ${300} \uworld`;
"#,
        ScriptTarget::ES5,
    );

    assert!(
        output.contains(
            r#"var y = "\\u{hello} ".concat(100, " \\xtraordinary ").concat(200, " wonderful ").concat(300, " \\uworld");"#
        ),
        "ES5 template downlevel should escape invalid raw template escapes as string text.\nOutput:\n{output}"
    );
}

#[test]
fn es5_tagged_template_cooked_non_bmp_codepoints_use_surrogate_escapes() {
    let output = parse_lower_emit(
        r#"
function tag(str: any, ...args: any[]): any { return str; }
const a = tag`${1}\u{1f622}`;
"#,
        ScriptTarget::ES5,
    );

    assert!(
        output
            .contains(r#"tag(__makeTemplateObject(["", "\uD83D\uDE22"], ["", "\\u{1f622}"]), 1)"#),
        "ES5 cooked template arrays should print non-BMP codepoints as UTF-16 surrogate escapes.\nOutput:\n{output}"
    );
}
