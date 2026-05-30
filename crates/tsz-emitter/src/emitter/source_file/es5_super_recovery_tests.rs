//! ES5 lowering for a bare `super` parser-recovery node (`super` with no
//! member access, TS1034).
//!
//! tsc substitutes the `super` keyword receiver with `_super.prototype`
//! (instance home) regardless of whether a member name follows, then emits the
//! dangling member access verbatim. The recovery AST is `super.<missing>`
//! (`PropertyAccessExpression` with a `NodeIndex::NONE` name), so the lowered
//! output is `_super.prototype.` — matching tsc. The choice is keyed on the
//! base being the `super` keyword, not on the spelling of the (present or
//! missing) property, so these tests vary class, method, and variable names to
//! prove the substitution is structural rather than name-matched.

use crate::context::emit::EmitContext;
use crate::emitter::{ModuleKind, Printer as EmitterPrinter, PrinterOptions};
use crate::lowering::LoweringPass;
use tsz_common::ScriptTarget;
use tsz_parser::ParserState;

/// Emit `source` at ES5 through the lowering pass and return the JS output.
fn emit_es5(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        module: ModuleKind::None,
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
fn bare_super_in_method_arrow_lowers_to_super_prototype_dot() {
    // `super` used as a value (recovery) inside a nested arrow in an instance
    // method must lower its receiver to `_super.prototype`, yielding the
    // dangling `_super.prototype.` form (not `_super.`).
    let source = "class Base { greet() {} }\n\
                  class Derived extends Base {\n\
                  \x20   greet() { var ref = () => () => super; }\n\
                  }\n";
    let output = emit_es5(source);

    assert!(
        output.contains("_super.prototype.;"),
        "bare super in a method should lower to `_super.prototype.`.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("return _super.;"),
        "bare super must not lower to a bare `_super.` receiver.\nOutput:\n{output}"
    );
}

#[test]
fn bare_super_in_constructor_arrow_lowers_to_super_prototype_dot() {
    // Same rule inside a constructor body; different class/variable names prove
    // the substitution is keyed on the `super` keyword, not on identifiers.
    let source = "class Animal { run() {} }\n\
                  class Dog extends Animal {\n\
                  \x20   constructor() { super(); var fn = () => () => super; }\n\
                  }\n";
    let output = emit_es5(source);

    assert!(
        output.contains("_super.prototype.;"),
        "bare super in a constructor should lower to `_super.prototype.`.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("return _super.;"),
        "bare super must not lower to a bare `_super.` receiver.\nOutput:\n{output}"
    );
}

#[test]
fn super_property_access_still_lowers_with_prototype() {
    // The present-name path is unchanged: `super.prop` still lowers to
    // `_super.prototype.prop`. Renamed class/property prove no name matching.
    let source = "class Widget { label = \"w\"; }\n\
                  class Button extends Widget {\n\
                  \x20   show() { return super.label; }\n\
                  }\n";
    let output = emit_es5(source);

    assert!(
        output.contains("_super.prototype.label"),
        "super.label should lower to `_super.prototype.label`.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("_super.prototype.;"),
        "a present member name must not produce a dangling dot.\nOutput:\n{output}"
    );
}

#[test]
fn super_method_call_still_lowers_with_prototype_call() {
    // The call path is unchanged after the shared-receiver refactor:
    // `super.m()` lowers to `_super.prototype.m.call(this)`.
    let source = "class Shape { area() {} }\n\
                  class Circle extends Shape {\n\
                  \x20   area() { super.area(); }\n\
                  }\n";
    let output = emit_es5(source);

    assert!(
        output.contains("_super.prototype.area.call("),
        "super.area() should lower to `_super.prototype.area.call(...)`.\nOutput:\n{output}"
    );
}
