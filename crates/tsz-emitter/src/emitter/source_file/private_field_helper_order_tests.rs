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

#[test]
fn private_field_leading_comments_move_to_weakmap_initializers() {
    let source = r#"
class A {
    /**
     * @public
     */
    #a = 1;
    /**
     * @private
     */
    #b = 2;
}
"#;
    let output = emit(source, ScriptTarget::ES2015);

    let public_comment = output
        .find("* @public")
        .unwrap_or_else(|| panic!("expected public JSDoc on lowered private field\n{output}"));
    let public_init = output
        .find("_A_a.set(this, 1);")
        .unwrap_or_else(|| panic!("expected lowered private #a initializer\n{output}"));
    let private_comment = output
        .find("* @private")
        .unwrap_or_else(|| panic!("expected private JSDoc on lowered private field\n{output}"));
    let private_init = output
        .find("_A_b.set(this, 2);")
        .unwrap_or_else(|| panic!("expected lowered private #b initializer\n{output}"));

    assert!(
        public_comment < public_init && private_comment < private_init,
        "Private field comments should move with constructor WeakMap initializers.\nOutput:\n{output}"
    );
}

#[test]
fn private_field_trailing_comments_move_to_weakmap_initializers() {
    let source = r#"
class A {
    #a = 1; // first
    #b = 2; // second
}
"#;
    let output = emit(source, ScriptTarget::ES2015);

    assert!(
        output.contains("_A_a.set(this, 1); // first"),
        "trailing comment for #a should move with the constructor WeakMap initializer.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_A_b.set(this, 2); // second"),
        "trailing comment for #b should move with the constructor WeakMap initializer.\nOutput:\n{output}"
    );
}

#[test]
fn private_accessor_helpers_follow_source_order() {
    let source = r#"
class A {
    get #a() { return 1; }
    set #a(value) { }
    get #b() { return 2; }
    set #b(value) { }
    get #c() { return 3; }
    set #c(value) { }
}
"#;
    let output = emit(source, ScriptTarget::ES2015);

    let a_pos = output
        .find("_A_a_get = function _A_a_get()")
        .unwrap_or_else(|| panic!("expected #a getter helper\n{output}"));
    let b_pos = output
        .find("_A_b_get = function _A_b_get()")
        .unwrap_or_else(|| panic!("expected #b getter helper\n{output}"));
    let c_pos = output
        .find("_A_c_get = function _A_c_get()")
        .unwrap_or_else(|| panic!("expected #c getter helper\n{output}"));

    assert!(
        a_pos < b_pos && b_pos < c_pos,
        "Private accessor helper initialization order should match source order.\nOutput:\n{output}"
    );
}

#[test]
fn no_body_private_accessors_emit_empty_extracted_helpers() {
    let source = r#"
class A {
    declare get #value(): number;
    declare set #value(value: number);
}
"#;
    let output = emit(source, ScriptTarget::ES2015);

    assert!(
        output.contains("_A_value_get = function _A_value_get() { }"),
        "no-body private getter should recover as an empty extracted helper.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_A_value_set = function _A_value_set(value) { }"),
        "no-body private setter should recover as an empty extracted helper.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("get ()") && !output.contains("set ("),
        "recovered no-body private accessors should not be printed in the class body.\nOutput:\n{output}"
    );
}

#[test]
fn private_async_helpers_downlevel_for_es2015_but_preserve_native_for_es2019() {
    let source = r#"
class A {
    async #method() { return 1; }
    async *#stream() { return 2; }
    async get #value() { return 3; }
    async set #value(value: number) { }
}
"#;
    let es2015 = emit(source, ScriptTarget::ES2015);
    assert!(
        es2015.contains("_A_method = function _A_method()")
            && es2015
                .contains("return __awaiter(this, void 0, void 0, function* () { return 1; });"),
        "private async methods should lower through __awaiter for ES2015.\nOutput:\n{es2015}"
    );
    assert!(
        es2015.contains("_A_stream = function _A_stream() { return __asyncGenerator(this, arguments, function* _A_stream_1() { return yield __await(2); }); }"),
        "private async generators should lower through __asyncGenerator for ES2015.\nOutput:\n{es2015}"
    );
    assert!(
        es2015.contains("_A_value_get = function _A_value_get()")
            && es2015
                .contains("return __awaiter(this, void 0, void 0, function* () { return 3; });"),
        "private async getters should lower through __awaiter for ES2015.\nOutput:\n{es2015}"
    );
    assert!(
        es2015.contains("_A_value_set = function _A_value_set(value)")
            && es2015.contains("return __awaiter(this, void 0, void 0, function* () { });"),
        "private async setters should lower through __awaiter for ES2015.\nOutput:\n{es2015}"
    );
    assert!(
        !es2015.contains("async function _A_method")
            && !es2015.contains("async function* _A_stream"),
        "ES2015 private helpers should not keep native async helper functions.\nOutput:\n{es2015}"
    );

    let es2019 = emit(source, ScriptTarget::ES2019);
    assert!(
        es2019.contains("_A_method = async function _A_method()"),
        "ES2019 should preserve native async private method helpers.\nOutput:\n{es2019}"
    );
    assert!(
        es2019.contains("_A_stream = async function* _A_stream()"),
        "ES2019 should preserve native async-generator private method helpers.\nOutput:\n{es2019}"
    );
}

#[test]
fn declare_and_abstract_private_fields_do_not_allocate_storage() {
    let source = r#"
class A {
    declare #erased: number;
    declare #method(): void;
    #kept = 1;
}
abstract class B {
    abstract #missing: number;
}
"#;
    let output = emit(source, ScriptTarget::ES2015);

    assert!(
        output.contains("_A_kept = new WeakMap()"),
        "ordinary private field should still allocate storage.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("_A_erased")
            && !output.contains("_A_method")
            && !output.contains("_B_missing"),
        "declare/abstract private members should not allocate runtime storage.\nOutput:\n{output}"
    );
}

#[test]
fn duplicate_private_field_and_static_field_use_last_static_storage() {
    let source = r#"
class A {
    #foo = 1;
    static #foo = true;
}
"#;
    let output = emit(source, ScriptTarget::ES2015);

    assert!(
        output.contains("#foo = 1;") && output.contains("static #foo = true;"),
        "conflicting private fields should be preserved in the class body.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_A_foo_1 = { value: 1 };")
            && output.contains("_A_foo_1 = { value: true };"),
        "field initializers in a conflict ending with a static field should use the selected value storage.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("_A_foo.set(this, 1);"),
        "the earlier instance storage should not receive the conflicting initializer.\nOutput:\n{output}"
    );
}

#[test]
fn duplicate_static_field_then_field_uses_last_instance_storage() {
    let source = r#"
class A {
    static #foo = "static";
    #foo = "instance";
}
"#;
    let output = emit(source, ScriptTarget::ES2015);

    assert!(
        output.contains("static #foo = \"static\";") && output.contains("#foo = \"instance\";"),
        "conflicting static/instance fields should stay in the class body.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_A_foo_1 = new WeakMap()")
            && output.contains("_A_foo_1.set(this, \"instance\");")
            && output.contains("_A_foo_1.set(A, \"static\");"),
        "a conflict ending with an instance field should route both field initializers through that WeakMap.\nOutput:\n{output}"
    );
}

#[test]
fn duplicate_private_methods_are_preserved_without_extracted_defs() {
    let source = r#"
class A {
    #foo() { return 1; }
    #foo() { return 2; }
}
"#;
    let output = emit(source, ScriptTarget::ES2015);

    assert!(
        output.contains("#foo() { return 1; }") && output.contains("#foo() { return 2; }"),
        "conflicting private methods should remain in the class body.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("_A_foo = function") && !output.contains("_A_foo_1 = function"),
        "conflicting private methods should not emit extracted helper definitions.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_A_instances = new WeakSet()"),
        "instance-brand storage is still emitted for conflicting private methods.\nOutput:\n{output}"
    );
}
