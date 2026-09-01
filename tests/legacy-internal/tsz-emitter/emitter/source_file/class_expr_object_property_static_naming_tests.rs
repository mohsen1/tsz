//! Structural emit tests for the named-evaluation rule on anonymous class
//! expressions with static fields assigned as object property values.
//!
//! Structural rule: when an anonymous class expression with static members
//! is the initializer of an object literal property (e.g.,
//! `{ Foo: class { static x = 1; } }`), tsc must emit
//! `__setFunctionName(_temp, "Foo")` inside the comma expression, using the
//! property key as the binding name.
//!
//! Named class expressions do NOT get `__setFunctionName` because their own
//! class name already provides the binding. Classes without static fields
//! get the property key as the function name through JS named evaluation in
//! the emitted IIFE (no helper needed).

use crate::context::emit::EmitContext;
use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
use crate::lowering::LoweringPass;
use tsz_common::ScriptTarget;
use tsz_parser::ParserState;

fn emit(source: &str, target: ScriptTarget) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let options = PrinterOptions {
        target,
        use_define_for_class_fields: false,
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

// Anonymous class expression with a static field in an object property must
// get `__setFunctionName` using the property key as the name.

#[test]
fn anonymous_class_with_static_field_in_object_property_emits_set_function_name_es5() {
    let source = "var obj = { Foo: class { static x = 1; } };\n";
    let output = emit(source, ScriptTarget::ES5);

    assert!(
        output.contains("__setFunctionName"),
        "Anonymous class with static field in object property must emit __setFunctionName.\n\
         Output:\n{output}"
    );
    assert!(
        output.contains("\"Foo\""),
        "The function name must be the property key 'Foo'.\nOutput:\n{output}"
    );
}

// Renamed property key — proves the rule uses the actual key, not a fixed string.
#[test]
fn renamed_anonymous_class_with_static_field_in_object_property_emits_set_function_name_es5() {
    let source = "var obj = { Widget: class { static count = 0; } };\n";
    let output = emit(source, ScriptTarget::ES5);

    assert!(
        output.contains("__setFunctionName"),
        "Renaming the property key must not suppress __setFunctionName.\nOutput:\n{output}"
    );
    assert!(
        output.contains("\"Widget\""),
        "The function name must be the renamed property key 'Widget'.\nOutput:\n{output}"
    );
}

// Multiple object properties with anonymous classes that have static fields:
// each should get `__setFunctionName` with its own property key.
#[test]
fn multiple_anonymous_classes_with_static_fields_in_object_property_each_get_set_function_name() {
    let source = "var obj = { Alpha: class { static a = 1; }, Beta: class { static b = 2; } };\n";
    let output = emit(source, ScriptTarget::ES5);

    assert!(
        output.contains("\"Alpha\""),
        "First property must get __setFunctionName with 'Alpha'.\nOutput:\n{output}"
    );
    assert!(
        output.contains("\"Beta\""),
        "Second property must get __setFunctionName with 'Beta'.\nOutput:\n{output}"
    );
}

// Named class expression in an object property must NOT get __setFunctionName
// because the class already has its own name binding.
#[test]
fn named_class_in_object_property_does_not_emit_set_function_name_es5() {
    let source = "var obj = { Foo: class Bar { static x = 1; } };\n";
    let output = emit(source, ScriptTarget::ES5);

    // The class name `Bar` is used; __setFunctionName is not needed.
    assert!(
        !output.contains("__setFunctionName"),
        "Named class expression in object property must NOT emit __setFunctionName.\n\
         Output:\n{output}"
    );
}

// Anonymous class with NO static fields in an object property keeps named
// evaluation through the property key (used as the IIFE function name).
// No `__setFunctionName` helper is needed.
#[test]
fn anonymous_class_without_static_fields_in_object_property_no_set_function_name_es5() {
    let source = "var obj = { Greet: class { hello() { return 1; } } };\n";
    let output = emit(source, ScriptTarget::ES5);

    assert!(
        !output.contains("__setFunctionName"),
        "Anonymous class without static members in object property must NOT emit \
         __setFunctionName (named evaluation through IIFE name).\nOutput:\n{output}"
    );
    // The property key should be used as the IIFE function name.
    assert!(
        output.contains("function Greet"),
        "Property key 'Greet' must become the IIFE function name.\nOutput:\n{output}"
    );
}

// String literal property key: still uses the string as the name.
#[test]
fn anonymous_class_with_string_key_and_static_field_emits_set_function_name_es5() {
    let source = "var obj = { \"myClass\": class { static v = 42; } };\n";
    let output = emit(source, ScriptTarget::ES5);

    assert!(
        output.contains("__setFunctionName"),
        "Anonymous class with static field under a string property key must emit \
         __setFunctionName.\nOutput:\n{output}"
    );
    assert!(
        output.contains("\"myClass\""),
        "The function name must be the string property key 'myClass'.\nOutput:\n{output}"
    );
}

// The helper boilerplate must be emitted when only object-property classes
// need __setFunctionName (i.e., no variable-declaration class triggers it).
#[test]
fn set_function_name_helper_boilerplate_emitted_for_object_property_class_es5() {
    let source = "var obj = { Svc: class { static instances = 0; } };\n";
    let output = emit(source, ScriptTarget::ES5);

    assert!(
        output.contains("__setFunctionName = "),
        "The __setFunctionName helper boilerplate must be emitted when only \
         object-property static classes trigger it.\nOutput:\n{output}"
    );
}
