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
fn es5_template_expression_escapes_line_terminators_in_string_text() {
    let output = parse_lower_emit(
        "
const y = `before
${value}
after`;
",
        ScriptTarget::ES5,
    );

    assert!(
        output.contains(r#"var y = "before\n".concat(value, "\nafter");"#),
        "ES5 template downlevel should keep template newlines inside string text.\nOutput:\n{output}"
    );
}

#[test]
fn unterminated_template_at_eof_keeps_recovery_newline() {
    let source = "// https://github.com/microsoft/TypeScript/issues/59345\n\
export class ParseThemeData {\n\
  parseButton(button: any) {\n\
    const {type, size} = button;\n\
    for (let item of type) {\n\
      const fontType = item.type;\n\
      const style = (state: string) => `color: var(--button-${fontType}-${state}-font-color)`;\n\
      this.classFormat(`${style('active')});\n\
    }\n\
    for (let item of size) {\n\
      const fontType = item.type;\n\
      this.classFormat(\n\
        [\n\
          `font-size: var(--button-size-${fontType}-fontSize)`,\n\
          `height: var(--button-size-${fontType}-height)`,\n\
        ].join(';')\n\
      );\n\
    }\n\
  }\n\
}";

    let es2015 = parse_lower_emit(source, ScriptTarget::ES2015);
    assert!(
        es2015.contains("}\n            ;"),
        "ES2015 recovery should keep the synthesized empty statement on its own line.\nOutput:\n{es2015}"
    );

    let es5 = parse_lower_emit(source, ScriptTarget::ES5);
    assert!(
        es5.contains("}\\n\""),
        "ES5 template downlevel should preserve the recovery newline inside string text.\nOutput:\n{es5}"
    );
}

#[test]
fn es5_template_downlevel_normalizes_crlf_and_lone_cr_in_string_text() {
    // tsc cooks <CR><LF> and <CR> to <LF> (TV), so ES5 .concat() downlevel
    // emits "\n", never "\r\n". Lone-CR no-substitution literal included.
    let output = parse_lower_emit(
        "const y = `before\r\n${value}\r\nafter`;\nconst z = `a\rb`;\n",
        ScriptTarget::ES5,
    );

    assert!(
        output.contains(r#"var y = "before\n".concat(value, "\nafter");"#),
        "ES5 template downlevel should normalize CRLF to LF in string text.\nOutput:\n{output}"
    );
    assert!(
        output.contains(r#"var z = "a\nb";"#),
        "ES5 no-substitution downlevel should normalize lone CR to LF.\nOutput:\n{output}"
    );
}

#[test]
fn es5_tagged_template_raw_array_normalizes_crlf() {
    // tsc's getRawLiteral (ES6 11.8.6.1 TRV) normalizes <CR><LF> and <CR> to
    // <LF> in the downlevel raw array, matching the cooked array.
    let output = parse_lower_emit(
        "function tag(s: any, ...a: any[]): any { return s; }\nconst t = tag`x\r\n${1}y\r`;\n",
        ScriptTarget::ES5,
    );

    assert!(
        output.contains(r#"__makeTemplateObject(["x\n", "y\n"], ["x\n", "y\n"])"#),
        "ES5 tagged-template raw and cooked arrays should both normalize CR/CRLF to LF.\nOutput:\n{output}"
    );
}

#[test]
fn es2015_template_emit_preserves_source_crlf_verbatim() {
    // Verbatim native template emit copies source bytes (tsc does too); the
    // TV/TRV normalization applies only to cooked values and downlevel raw.
    let output = parse_lower_emit(
        "function tag(s: any, ...a: any[]): any { return s; }\nconst t = tag`x\r\n${1}y`;\nconst u = `p\r\nq`;\n",
        ScriptTarget::ES2015,
    );

    assert!(
        output.contains("tag `x\r\n${1}y`"),
        "ES2015 tagged template should stay native with source CRLF preserved.\nOutput:\n{output:?}"
    );
    assert!(
        output.contains("`p\r\nq`"),
        "ES2015 template literal should keep source CRLF verbatim.\nOutput:\n{output:?}"
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
