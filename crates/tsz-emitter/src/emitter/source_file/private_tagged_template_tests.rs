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

#[test]
fn optional_private_field_call_uses_lowered_get_as_callee() {
    let source = r#"
class Widget {
    static #run = function(a, ...b) {};
    #tap = function() {};
    static factory() { return Widget; }
    test(items) {
        Widget.#run?.(0, ...items, 3);
        Widget.factory().#run?.();
        this.#tap?.();
    }
}
"#;
    let output = emit(source, ScriptTarget::ES2015);

    assert!(
        output.contains(
            "(_b = __classPrivateFieldGet(_a, _a, \"f\", _Widget_run)) === null || _b === void 0 ? void 0 : _b.call(_a, 0, ...items, 3)"
        ),
        "Static private field optional calls should null-check the lowered private get and call with the class alias receiver.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "(_d = __classPrivateFieldGet((_c = _a.factory()), _a, \"f\", _Widget_run)) === null || _d === void 0 ? void 0 : _d.call(_c)"
        ),
        "Side-effecting private field optional call receivers should be captured once and reused for `.call()`.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "(_e = __classPrivateFieldGet(this, _Widget_tap, \"f\")) === null || _e === void 0 ? void 0 : _e.call(this)"
        ),
        "Instance private field optional calls should call with the original receiver.\nOutput:\n{output}"
    );
}

#[test]
fn static_private_async_generator_helpers_preserve_function_kind() {
    let source = r#"
const Widget = class {
    static async #load() { return await Promise.resolve(1); }
    static *#values() { yield 1; }
    static async *#stream() {
        yield (await Promise.resolve(2));
    }
    static run() {
        this.#load();
        this.#values();
        this.#stream();
    }
}
"#;
    let output = emit(source, ScriptTarget::ES2019);

    assert!(
        output.contains("_Widget_load = async function _Widget_load()"),
        "Extracted async private methods should stay async functions.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_Widget_values = function* _Widget_values()"),
        "Extracted generator private methods should stay generator functions.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_Widget_stream = async function* _Widget_stream() {\n        yield (await Promise.resolve(2));\n    }"),
        "Extracted async generator private methods should preserve function kind and multiline body formatting.\nOutput:\n{output}"
    );
}
