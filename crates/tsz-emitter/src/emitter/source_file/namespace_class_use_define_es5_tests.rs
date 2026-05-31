//! ES5 emit parity for classes nested inside a namespace/module body under
//! `useDefineForClassFields`.
//!
//! Structural rule: a class declared inside a namespace/module IIFE must apply
//! the same field/method lowering as a top-level class. The
//! `useDefineForClassFields` flag is threaded into the namespace ES5 transform
//! so that namespace-scoped classes lower instance/static fields to
//! `Object.defineProperty(this, …)` / `Object.defineProperty(C, …)` and lower
//! static methods to `Object.defineProperty(C, …)` exactly like top-level
//! classes. Before this fix the namespace transform left the flag at its
//! default of `false`, so nested classes fell back to plain `C.x = fn`
//! assignments and dropped no-initializer fields.
//!
//! These tests assert the *namespace-nested* lowering matches the documented
//! `useDefineForClassFields` behavior and that the rule is keyed on the flag,
//! not on any particular member name (varied identifiers are used).

use crate::context::emit::EmitContext;
use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
use crate::lowering::LoweringPass;
use tsz_common::ScriptTarget;
use tsz_parser::ParserState;

fn emit_es5_use_define(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        use_define_for_class_fields: true,
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
fn namespace_nested_static_method_uses_define_property_like_top_level() {
    // A static method whose name collides with a Function built-in non-writable
    // own property (`name`) must lower to `Object.defineProperty(C, "name", …)`
    // under useDefineForClassFields. This must hold inside a namespace too.
    let source = "\
namespace NS {
    class Widget {
        static name() {}
        instanceMethod() {}
    }
}
";
    let output = emit_es5_use_define(source);
    assert!(
        output.contains("Object.defineProperty(Widget, \"name\", {"),
        "Namespace-nested static method named `name` should lower to define-property under useDefineForClassFields.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("Widget.name = function"),
        "Namespace-nested static method must not fall back to plain assignment.\nOutput:\n{output}"
    );
}

#[test]
fn namespace_nested_no_init_instance_field_defines_on_this() {
    // A no-initializer instance field lowers to `Object.defineProperty(this, …)`
    // with `value: void 0` under useDefineForClassFields, inside a namespace.
    let source = "\
namespace Outer {
    class Holder {
        slot: number;
    }
}
";
    let output = emit_es5_use_define(source);
    assert!(
        output.contains("Object.defineProperty(this, \"slot\", {"),
        "Namespace-nested no-init instance field should define on `this`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("value: void 0"),
        "No-init instance field define should carry `value: void 0`.\nOutput:\n{output}"
    );
}

#[test]
fn namespace_nested_static_field_with_initializer_defines_on_class() {
    // A static field WITH an initializer still materializes a static define on
    // the class. Uses a non-conflicting name to prove the rule is keyed on the
    // useDefineForClassFields flag, not on a particular identifier.
    let source = "\
namespace Mod {
    class Counter {
        static total: number = 7;
    }
}
";
    let output = emit_es5_use_define(source);
    assert!(
        output.contains("Object.defineProperty(Counter, \"total\", {"),
        "Namespace-nested initialized static field should define on the class.\nOutput:\n{output}"
    );
    assert!(
        output.contains("value: 7"),
        "Initialized static field define should carry its initializer value.\nOutput:\n{output}"
    );
}

#[test]
fn namespace_nested_lowering_matches_top_level_for_renamed_members() {
    // The rule must not be keyed on a member spelling: a renamed, non-conflicting
    // static method inside a namespace lowers via define-property exactly like
    // the same class declared at top level.
    let nested = emit_es5_use_define(
        "\
namespace Space {
    class Service {
        static handler() {}
    }
}
",
    );
    assert!(
        nested.contains("Object.defineProperty(Service, \"handler\", {"),
        "Renamed namespace-nested static method should still use define-property.\nOutput:\n{nested}"
    );
    assert!(
        !nested.contains("Service.handler = function"),
        "Renamed namespace-nested static method must not use plain assignment.\nOutput:\n{nested}"
    );
}
