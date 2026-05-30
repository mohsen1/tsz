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

#[test]
fn invalid_cross_class_private_reads_do_not_drive_helper_order() {
    let source = r#"
class Base {
    static #prop = 123;
    static method(x: Derived) {
        Derived.#derivedProp;
        Base.#prop = 10;
    }
}
class Derived extends Base {
    static #derivedProp = 10;
    static method(x: Derived) {
        Derived.#derivedProp;
        Base.#prop = 10;
    }
}
"#;
    let output = emit(source, ScriptTarget::ES2015);

    let set_pos = output
        .find("var __classPrivateFieldSet")
        .expect("expected private-field set helper");
    let get_pos = output
        .find("var __classPrivateFieldGet")
        .expect("expected private-field get helper");
    assert!(
        set_pos < get_pos,
        "Invalid cross-class private reads should not force Get before the first emitted Set.\nOutput:\n{output}"
    );
}

// Rule: for a class *declaration* lowered to the WeakMap pattern, the private
// member init statements (`_C_field = new WeakMap()`) must be emitted before the
// static field initialization statements (`C.x = value;`). A static initializer
// can instantiate the class, whose constructor populates the WeakMaps, so the
// storage must exist first. This holds even when the static initializer does not
// reference the class or a private name.

/// Assert that a `new WeakMap()` / `new WeakSet()` private init statement is
/// emitted before the given static-field assignment statement.
fn assert_weakmap_inits_before(output: &str, static_assign: &str) {
    let static_pos = output.find(static_assign).unwrap_or_else(|| {
        panic!("expected static assignment `{static_assign}`\nOutput:\n{output}")
    });
    let weakmap_pos = output
        .find("new WeakMap()")
        .or_else(|| output.find("new WeakSet()"))
        .unwrap_or_else(|| panic!("expected a WeakMap/WeakSet init\nOutput:\n{output}"));
    assert!(
        weakmap_pos < static_pos,
        "WeakMap/WeakSet inits must precede the static field assignment `{static_assign}`.\nOutput:\n{output}"
    );
}

#[test]
fn private_weakmap_inits_precede_static_field_self_instantiation() {
    // Reported repro shape: `static inst = new A()` instantiates the class.
    let source = r#"
class A {
  #foo = 1;
  static inst = new A();
  #prop = 2;
}
"#;
    let output = emit(source, ScriptTarget::ES2015);
    assert_weakmap_inits_before(&output, "A.inst");
}

#[test]
fn private_weakmap_inits_precede_static_field_no_self_reference() {
    // Same ordering rule must hold even when the static initializer is a plain
    // literal that neither references the class nor any private name. Renamed
    // class/field to prove the rule is not keyed on the reported spelling.
    let source = r#"
class Widget {
  #count = 1;
  static label = 5;
}
"#;
    let output = emit(source, ScriptTarget::ES2015);
    assert_weakmap_inits_before(&output, "Widget.label");
}

#[test]
fn private_method_weakset_init_precedes_static_field() {
    // A private instance method produces a `_C_instances = new WeakSet()` init,
    // which must also precede the static field statement. Different target
    // (ES2017) and different member/field names exercise the same rule.
    let source = r#"
class Service {
  #run() { return 1; }
  static version = "1.0";
}
"#;
    let output = emit(source, ScriptTarget::ES2017);
    assert_weakmap_inits_before(&output, "Service.version");
}

#[test]
fn no_static_field_leaves_private_inits_in_place() {
    // Negative/fallback case: a class declaration with only instance private
    // members and no static field still lowers correctly (a WeakMap init is
    // present) and the change does not synthesize a spurious static assignment.
    let source = r#"
class Box {
  #value = 7;
  get() { return this.#value; }
}
"#;
    let output = emit(source, ScriptTarget::ES2015);
    assert!(
        output.contains("new WeakMap()"),
        "expected a private-field WeakMap init.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("Box."),
        "instance-only class must not emit a static field assignment.\nOutput:\n{output}"
    );
}
