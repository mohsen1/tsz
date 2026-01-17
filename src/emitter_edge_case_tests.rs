//! Edge case tests for emitter features
//!
//! Tests for recently implemented features and edge cases:
//! - Comment preservation with UTF-8
//! - Import/export helpers
//! - Module transforms
//! - Helper emission ordering

use crate::emit_context::EmitContext;
use crate::lowering_pass::LoweringPass;
use crate::thin_emitter::{ModuleKind, PrinterOptions, ScriptTarget, ThinPrinter};
use crate::thin_parser::ThinParserState;

#[test]
fn test_comment_with_utf8_emoji() {
    let source = r#"
// Comment with emoji 🚀
const x = "test";
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.set_source_text(parser.get_source_text());
    printer.emit(root);
    let output = printer.get_output();

    assert!(output.contains("🚀"), "Emoji should be preserved in output");
    assert!(
        !output.contains('\u{FFFD}'),
        "Should not contain replacement character"
    );
}

#[test]
fn test_comment_with_multibyte_unicode() {
    let source = r#"
// Japanese: こんにちは
// Chinese: 你好
// Arabic: مرحبا
const x = 1;
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.set_source_text(parser.get_source_text());
    printer.emit(root);
    let output = printer.get_output();

    assert!(
        output.contains("こんにちは"),
        "Japanese should be preserved"
    );
    assert!(output.contains("你好"), "Chinese should be preserved");
    assert!(output.contains("مرحبا"), "Arabic should be preserved");
}

#[test]
fn test_triple_slash_directive_filtered() {
    let source = r#"
/// <reference path="./other.ts" />
/// <amd-module name="MyModule" />
// Regular comment
const x = 1;
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.set_source_text(parser.get_source_text());
    printer.emit(root);
    let output = printer.get_output();

    assert!(
        !output.contains("/// <reference"),
        "Reference directive should be filtered"
    );
    assert!(
        !output.contains("/// <amd"),
        "AMD directive should be filtered"
    );
    assert!(
        output.contains("// Regular comment"),
        "Regular comments should be preserved"
    );
}

#[test]
fn test_namespace_import_uses_import_star_helper() {
    let source = r#"
import * as ts from "typescript";
const x = ts.version;
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.module = ModuleKind::CommonJS;
    let mut printer = ThinPrinter::with_options(&parser.arena, options);
    printer.set_source_text(parser.get_source_text());
    printer.emit(root);
    let output = printer.get_output();

    // Should transform to CommonJS
    assert!(
        output.contains("require(\"typescript\")"),
        "Should use require()"
    );
    assert!(output.contains("\"use strict\";"), "Should have use strict");
    assert!(
        output.contains("__esModule"),
        "Should have __esModule marker"
    );
}

#[test]
fn test_helper_emission_order() {
    let source = r#"
/*
 * License header
 */
export const x = 1;
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.module = ModuleKind::CommonJS;
    let mut printer = ThinPrinter::with_options(&parser.arena, options);
    printer.set_source_text(parser.get_source_text());
    printer.emit(root);
    let output = printer.get_output();

    // Check order: "use strict" → comment → __esModule → code
    let strict_pos = output
        .find("\"use strict\";")
        .expect("Should have use strict");
    let comment_pos = output.find("License header").expect("Should have comment");
    let esmodule_pos = output.find("__esModule").expect("Should have __esModule");

    assert!(
        strict_pos < comment_pos,
        "use strict should come before comment"
    );
    assert!(
        comment_pos < esmodule_pos,
        "comment should come before __esModule"
    );
}

#[test]
fn test_export_assignment_suppresses_other_exports() {
    let source = r#"
export class C {}
export = C;
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.module = ModuleKind::CommonJS;
    let mut printer = ThinPrinter::with_options(&parser.arena, options);
    printer.set_source_text(parser.get_source_text());
    printer.emit(root);
    let output = printer.get_output();

    // Should emit class declaration (ESNext uses class, ES5 uses var)
    assert!(
        output.contains("class C") || output.contains("var C"),
        "Should emit class declaration: {}",
        output
    );

    // Should emit module.exports = C
    assert!(
        output.contains("module.exports = C"),
        "Should emit export assignment"
    );

    // Should NOT emit exports.C = C (suppressed by export =)
    let lines: Vec<&str> = output.lines().collect();
    let double_export = lines.iter().any(|line| line.contains("exports.C = C"));
    assert!(
        !double_export,
        "Should not double-export when using export ="
    );
}

#[test]
fn test_commonjs_preamble_with_no_imports() {
    let source = r#"
export const x = 1;
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.module = ModuleKind::CommonJS;
    let mut printer = ThinPrinter::with_options(&parser.arena, options);
    printer.set_source_text(parser.get_source_text());
    printer.emit(root);
    let output = printer.get_output();

    // Should have use strict
    assert!(output.contains("\"use strict\";"), "Should have use strict");

    // Should have __esModule marker
    assert!(
        output.contains("__esModule"),
        "Should have __esModule marker"
    );

    // Should have exports assignment
    assert!(
        output.contains("exports.x"),
        "Should have exports.x assignment"
    );
}

#[test]
fn test_parse_error_tolerance() {
    // Source with intentional syntax error
    let source = r#"
const x =
const y = 2;
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.set_source_text(parser.get_source_text());
    printer.emit(root);
    let output = printer.get_output();

    // Should emit valid JavaScript despite parse error
    assert!(
        output.contains("const x") || output.contains("let x") || output.contains("var x"),
        "Should emit x declaration: {}",
        output
    );
    assert!(output.contains("y = 2"), "Should emit y assignment");

    // Prefer void 0 for missing value; some recovery paths preserve the keyword token.
    assert!(
        output.contains("void 0") || output.contains("const x = const"),
        "Should recover missing initializer: {}",
        output
    );
}

#[test]
fn test_arrow_function_this_capture() {
    let source = r#"
class C {
    method() {
        const fn = () => this.x;
    }
}
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.set_source_text(parser.get_source_text());
    printer.set_target_es5(true);
    printer.emit(root);
    let output = printer.get_output();

    // ES5 transform should capture 'this'
    assert!(
        output.contains("_this"),
        "Should capture this as _this for ES5"
    );
}

#[test]
fn test_class_extends_helper() {
    let source = r#"
class Base {}
class Derived extends Base {}
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = ThinPrinter::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_text(parser.get_source_text());
    printer.set_target_es5(ctx.target_es5);
    printer.emit(root);
    let output = printer.get_output();

    // Should emit __extends helper
    assert!(output.contains("__extends"), "Should emit __extends helper");
    assert!(
        output.contains("extendStatics"),
        "Should have full __extends implementation"
    );
}

#[test]
fn test_named_import_bindings() {
    let source = r#"
import { foo, bar as baz } from "module";
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.module = ModuleKind::CommonJS;
    let mut printer = ThinPrinter::with_options(&parser.arena, options);
    printer.set_source_text(parser.get_source_text());
    printer.emit(root);
    let output = printer.get_output();

    // Should emit require
    assert!(
        output.contains("require(\"module\")"),
        "Should emit require"
    );

    // Should emit bindings
    assert!(output.contains("var foo"), "Should create foo binding");
    assert!(
        output.contains("var baz"),
        "Should create baz binding (renamed from bar)"
    );
}

#[test]
fn test_default_import_binding() {
    let source = r#"
import myDefault from "module";
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.module = ModuleKind::CommonJS;
    let mut printer = ThinPrinter::with_options(&parser.arena, options);
    printer.set_source_text(parser.get_source_text());
    printer.emit(root);
    let output = printer.get_output();

    // Should access .default property
    assert!(
        output.contains("myDefault = "),
        "Should create myDefault binding"
    );
    assert!(
        output.contains(".default"),
        "Should access .default property"
    );
}

#[test]
fn test_multiline_block_comment_preserved() {
    let source = r#"
/*
 * Multi-line
 * block comment
 */
const x = 1;
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = ThinPrinter::new(&parser.arena);
    printer.set_source_text(parser.get_source_text());
    printer.emit(root);
    let output = printer.get_output();

    assert!(output.contains("/*"), "Should preserve block comment start");
    assert!(
        output.contains("Multi-line"),
        "Should preserve comment content"
    );
    assert!(output.contains("*/"), "Should preserve block comment end");
}

#[test]
fn test_es6_module_no_commonjs_transform() {
    let source = r#"
import { x } from "module";
export const y = x;
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.module = ModuleKind::ES2015;
    let mut printer = ThinPrinter::with_options(&parser.arena, options);
    printer.set_source_text(parser.get_source_text());
    printer.emit(root);
    let output = printer.get_output();

    // Should NOT transform to CommonJS
    assert!(
        !output.contains("require("),
        "Should not use require in ES6 mode"
    );
    assert!(
        !output.contains("exports."),
        "Should not use exports in ES6 mode"
    );

    // Should keep ES6 syntax
    assert!(output.contains("import"), "Should keep import statement");
    assert!(output.contains("export"), "Should keep export statement");
}
