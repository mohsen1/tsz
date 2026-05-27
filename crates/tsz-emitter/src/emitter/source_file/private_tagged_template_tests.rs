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
fn instance_private_accessor_tag_binds_original_receiver() {
    let source = r#"
class Box {
    get #tag() { return function() {}; }
    make() { return new Box(); }
    run() {
        this.#tag`a`;
        this.make().#tag`b`;
    }
}
"#;
    let output = emit(source, ScriptTarget::ES2015);

    assert!(
        output.contains(
            "__classPrivateFieldGet(this, _Box_instances, \"a\", _Box_tag_get).bind(this) `a`"
        ),
        "Simple private accessor tags should bind to `this`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__classPrivateFieldGet((_a = this.make()), _Box_instances, \"a\", _Box_tag_get).bind(_a) `b`"),
        "Side-effecting private accessor tag receivers should be captured once.\nOutput:\n{output}"
    );
}

#[test]
fn static_private_method_tag_binds_class_alias_receiver() {
    let source = r#"
class Widget {
    static #tag() {}
    static factory() { return Widget; }
    run() {
        Widget.#tag`a`;
        Widget.factory().#tag`b`;
    }
}
"#;
    let output = emit(source, ScriptTarget::ES2015);

    assert!(
        output.contains("__classPrivateFieldGet(_a, _a, \"m\", _Widget_tag).bind(_a) `a`"),
        "Static private method tags should bind to the class alias receiver.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "__classPrivateFieldGet((_b = _a.factory()), _a, \"m\", _Widget_tag).bind(_b) `b`"
        ),
        "Side-effecting static private method tag receivers should be captured once.\nOutput:\n{output}"
    );
}

#[test]
fn static_private_method_call_captures_receiver_and_preserves_rest_param() {
    let source = r#"
class Widget {
    static #run(a, ...b) {}
    static factory() { return Widget; }
    run(items) {
        Widget.factory().#run(0, ...items, 3);
    }
}
"#;
    let output = emit(source, ScriptTarget::ES2015);

    assert!(
        output.contains(
            "__classPrivateFieldGet((_b = _a.factory()), _a, \"m\", _Widget_run).call(_b, 0, ...items, 3)"
        ),
        "Side-effecting static private method call receivers should be captured once.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_Widget_run = function _Widget_run(a, ...b) { }"),
        "Extracted private method definitions should preserve rest parameters.\nOutput:\n{output}"
    );
}
