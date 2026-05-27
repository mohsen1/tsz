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
