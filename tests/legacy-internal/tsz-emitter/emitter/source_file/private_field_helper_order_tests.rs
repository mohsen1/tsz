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

#[test]
fn es5_private_name_bundle_stays_inside_plain_class_iife() {
    let source = r#"
class Holder {
    #field = 1;
    #method() {}
    static #staticField = "value";
    static #staticMethod() {}
    get #accessor() { return this.#field; }
    set #accessor(value) { this.#field = value; }
}
"#;
    let output = emit(source, ScriptTarget::ES5);

    let class_pos = output
        .find("var Holder = /** @class */")
        .expect("expected an ES5 class IIFE");
    let storage_decl_pos = output
        .find("var _Holder_instances, _a, _Holder_field, _Holder_method")
        .expect("expected private storage declaration bundle");
    let return_pos = output
        .find("return Holder;")
        .expect("expected class IIFE return");
    let storage_init_pos = output
        .find("_a = Holder, _Holder_field = new WeakMap(), _Holder_instances = new WeakSet()")
        .expect("expected private storage initialization bundle");

    assert!(
        class_pos < storage_decl_pos
            && storage_decl_pos < storage_init_pos
            && storage_init_pos < return_pos,
        "Plain ES5 classes keep private-name storage bundles inside the IIFE.\nOutput:\n{output}"
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
fn es5_getter_only_private_accessor_omits_setter_helper() {
    // A getter-only private accessor lowered for ES5 must reserve only the
    // getter helper var. `tsc` never mints a `_set` helper for an accessor that
    // has no setter; a spurious `var ..., _X_size_set` is a dead binding.
    let source = r#"
class Widget {
    #v = 1;
    get #size() { return this.#v; }
    probe() { return this.#size; }
}
"#;
    let output = emit(source, ScriptTarget::ES5);

    assert!(
        output.contains("_Widget_size_get"),
        "getter-only private accessor should reserve its getter helper.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("_Widget_size_set"),
        "getter-only private accessor must not reserve a dead setter helper.\nOutput:\n{output}"
    );
}

#[test]
fn es5_setter_only_private_accessor_omits_getter_helper() {
    // Symmetric to the getter-only case: a setter-only private accessor reserves
    // only the setter helper var, never a dead `_get`.
    let source = r#"
class Gauge {
    #v = 0;
    set #level(n: number) { this.#v = n; }
    probe(n: number) { this.#level = n; }
}
"#;
    let output = emit(source, ScriptTarget::ES5);

    assert!(
        output.contains("_Gauge_level_set"),
        "setter-only private accessor should reserve its setter helper.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("_Gauge_level_get"),
        "setter-only private accessor must not reserve a dead getter helper.\nOutput:\n{output}"
    );
}

#[test]
fn es5_set_before_get_private_accessor_reserves_and_inits_setter_first() {
    // When the setter is written before the getter in source, `tsc` reserves and
    // initializes the `_set` helper before `_get` (source order), not a fixed
    // get-before-set order.
    let source = r#"
class Meter {
    #v = 0;
    set #value(n: number) { this.#v = n; }
    get #value() { return this.#v; }
    probe() { this.#value = 2; return this.#value; }
}
"#;
    let output = emit(source, ScriptTarget::ES5);

    let decl_set = output
        .find("_Meter_value_set,")
        .or_else(|| output.find("_Meter_value_set;"))
        .unwrap_or_else(|| panic!("expected setter helper declaration\n{output}"));
    let decl_get = output
        .find("_Meter_value_get,")
        .or_else(|| output.find("_Meter_value_get;"))
        .unwrap_or_else(|| panic!("expected getter helper declaration\n{output}"));
    assert!(
        decl_set < decl_get,
        "set-before-get accessor should declare the setter helper first.\nOutput:\n{output}"
    );

    let init_set = output
        .find("_Meter_value_set = function")
        .unwrap_or_else(|| panic!("expected setter helper init\n{output}"));
    let init_get = output
        .find("_Meter_value_get = function")
        .unwrap_or_else(|| panic!("expected getter helper init\n{output}"));
    assert!(
        init_set < init_get,
        "set-before-get accessor should initialize the setter helper first.\nOutput:\n{output}"
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

// A public auto-accessor's instance storage WeakMap is part of the single
// combined private-name initialization statement that `tsc` emits at the end of
// a lowered class declaration (private-field WeakMaps, then the private-method
// WeakSet, then the auto-accessor storages, then private method/accessor
// function defs) — not a separate trailing statement. These tests lock that
// folding and var-declaration ordering. Binder names are varied across tests so
// the assertions track structure, not a particular spelling.
//
// `tsc --target es2015` (and every target < ES2022, since auto-accessors lower
// to the WeakMap pattern there) for `class C { #x = 1; accessor y = 3; ... }`:
//   var _C_x, _C_y_accessor_storage;
//   ...
//   _C_x = new WeakMap(), _C_y_accessor_storage = new WeakMap();

#[test]
fn auto_accessor_storage_folds_into_private_field_init_statement() {
    let source = r#"
class Wrapper {
    #value = 1;
    accessor label = "x";
    get peek() { return this.#value; }
}
"#;
    let output = emit(source, ScriptTarget::ES2015);

    // Single combined init statement, private field first then accessor storage.
    assert!(
        output.contains(
            "_Wrapper_value = new WeakMap(), _Wrapper_label_accessor_storage = new WeakMap();"
        ),
        "auto-accessor storage must fold into the private-field init statement, after the field.\nOutput:\n{output}"
    );
    // The accessor storage must not start its own statement (the pre-fix bug
    // emitted `\n_Wrapper_label_accessor_storage = new WeakMap()` on its own line).
    assert!(
        !output.contains("\n_Wrapper_label_accessor_storage = new WeakMap()"),
        "auto-accessor storage must not be emitted as its own statement.\nOutput:\n{output}"
    );
    // var-declaration order: field name before accessor-storage name.
    let field_decl = output
        .find("var _Wrapper_value, _Wrapper_label_accessor_storage;")
        .or_else(|| output.find("_Wrapper_value, _Wrapper_label_accessor_storage"));
    assert!(
        field_decl.is_some(),
        "var declaration must list the private field before the accessor storage.\nOutput:\n{output}"
    );
}

#[test]
fn auto_accessor_storage_folds_after_private_method_weakset() {
    // Private method (→ instances WeakSet) plus a public auto-accessor: tsc emits
    // `_K_instances = new WeakSet(), _K_a_accessor_storage = new WeakMap(), _K_run = function ...`
    let source = r#"
class Kit {
    accessor a = 1;
    #run() { return 2; }
    call() { return this.#run(); }
}
"#;
    let output = emit(source, ScriptTarget::ES2015);

    assert!(
        output.contains(
            "_Kit_instances = new WeakSet(), _Kit_a_accessor_storage = new WeakMap(), _Kit_run = function"
        ),
        "accessor storage must sit between the instance WeakSet and the private-method def, in one statement.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("_Kit_a_accessor_storage = new WeakMap();"),
        "accessor storage must not be emitted as its own statement when private lowering is present.\nOutput:\n{output}"
    );
}

#[test]
fn multiple_auto_accessors_fold_in_source_order_after_private_fields() {
    let source = r#"
class Grid {
    #m = 1;
    accessor p = 2;
    #n = 3;
    accessor q = 4;
    get sum() { return this.#m + this.#n; }
}
"#;
    let output = emit(source, ScriptTarget::ES2015);

    // Private fields first (source order), then accessor storages (source order),
    // all in one statement.
    assert!(
        output.contains(
            "_Grid_m = new WeakMap(), _Grid_n = new WeakMap(), _Grid_p_accessor_storage = new WeakMap(), _Grid_q_accessor_storage = new WeakMap();"
        ),
        "fields then accessor storages, source order, single statement.\nOutput:\n{output}"
    );
    assert!(
        output
            .contains("var _Grid_m, _Grid_n, _Grid_p_accessor_storage, _Grid_q_accessor_storage;"),
        "var declaration order must match: fields then accessor storages.\nOutput:\n{output}"
    );
}

#[test]
fn pure_auto_accessors_keep_their_standalone_init_statement() {
    // With no private-name lowering there is nothing to fold into; the
    // standalone auto-accessor init statement is preserved (tsc parity).
    let source = r#"
class Plain {
    accessor a = 1;
    accessor b = 2;
}
"#;
    let output = emit(source, ScriptTarget::ES2015);

    assert!(
        output.contains(
            "_Plain_a_accessor_storage = new WeakMap(), _Plain_b_accessor_storage = new WeakMap();"
        ),
        "pure auto-accessor classes still emit their combined storage init.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var _Plain_a_accessor_storage, _Plain_b_accessor_storage;"),
        "pure auto-accessor var declaration is unchanged.\nOutput:\n{output}"
    );
}

#[test]
fn auto_accessor_storage_folds_when_static_field_forces_pre_static_inits() {
    // A static field routes the private inits through the pre-static emission
    // path (private-name storage must exist before a static initializer could
    // instantiate the class). The auto-accessor storage must still fold into
    // that single combined statement.
    let source = r#"
class Reg {
    #x = 1;
    accessor y = 2;
    static count = 0;
    get g() { return this.#x; }
}
"#;
    let output = emit(source, ScriptTarget::ES2015);

    assert!(
        output.contains("_Reg_x = new WeakMap(), _Reg_y_accessor_storage = new WeakMap();"),
        "auto-accessor storage folds into the pre-static private-init statement.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("\n_Reg_y_accessor_storage = new WeakMap()"),
        "auto-accessor storage must not be emitted as its own statement.\nOutput:\n{output}"
    );
    // The static field assignment follows the private inits.
    let inits = output
        .find("_Reg_x = new WeakMap()")
        .expect("private inits present");
    let static_assign = output.find("Reg.count = 0").expect("static field present");
    assert!(
        inits < static_assign,
        "private/auto-accessor storage initializes before the static field.\nOutput:\n{output}"
    );
}

#[test]
fn auto_accessor_storage_precedes_static_field_without_private_lowering() {
    // No private-name lowering to fold into, but a static field is present: tsc
    // still emits the auto-accessor instance storage WeakMap *before* the static
    // field initializer (the storage must exist before a static initializer could
    // instantiate the class).
    let source = r#"
class Conf {
    accessor opt = 2;
    static total = 0;
}
"#;
    let output = emit(source, ScriptTarget::ES2015);

    let storage = output
        .find("_Conf_opt_accessor_storage = new WeakMap();")
        .expect("auto-accessor storage init present");
    let static_assign = output.find("Conf.total = 0").expect("static field present");
    assert!(
        storage < static_assign,
        "auto-accessor storage must initialize before the static field, even with no private lowering.\nOutput:\n{output}"
    );
    // It is its own statement (no private statement to fold into) and not left
    // trailing after the static field with a blank line (the pre-fix bug).
    assert!(
        !output.contains("Conf.total = 0;\n\n_Conf_opt_accessor_storage = new WeakMap()"),
        "auto-accessor storage must not trail after the static field.\nOutput:\n{output}"
    );
}

// Rule: at `--target es5`, a class with private storage but NO static private
// member needs no class-value alias, so tsc hoists `var _C_x;` before the class
// IIFE and runs `_C_x = new WeakMap();` after it (issue #14767). This applies to
// every non-CommonJS class shape (plain script, ESM, non-exported); the CommonJS
// export path already drove the same lift. Classes WITH a static private member
// keep storage inside the IIFE alongside the `_a = C` alias.

#[test]
fn es5_instance_private_field_storage_hoists_around_iife() {
    // The reported repro: a single instance private field.
    let source = r#"
class C {
  #x = 1;
  read() { return this.#x; }
}
"#;
    let output = emit(source, ScriptTarget::ES5);
    assert!(
        output.contains("var _C_x;\nvar C = /** @class */"),
        "WeakMap decl must be hoisted immediately before the IIFE.\nOutput:\n{output}"
    );
    assert!(
        output.contains("}());\n_C_x = new WeakMap();"),
        "WeakMap init must run immediately after the IIFE.\nOutput:\n{output}"
    );
}

#[test]
fn es5_instance_private_field_storage_hoists_with_renamed_binders() {
    // Same rule, different class/field spellings: the lift is structural, not
    // keyed on the reported `C`/`#x` names.
    let source = r#"
class Widget {
  #value = 1;
  peek() { return this.#value; }
}
"#;
    let output = emit(source, ScriptTarget::ES5);
    assert!(
        output.contains("var _Widget_value;\nvar Widget = /** @class */"),
        "renamed WeakMap decl must hoist before the IIFE.\nOutput:\n{output}"
    );
    assert!(
        output.contains("}());\n_Widget_value = new WeakMap();"),
        "renamed WeakMap init must run after the IIFE.\nOutput:\n{output}"
    );
}

#[test]
fn es5_instance_private_method_weakset_hoists_around_iife() {
    // A private instance method adds a `_C_instances = new WeakSet()` brand and a
    // `_C_m = function ...` slot. With no static private member they hoist out of
    // the IIFE together with the field WeakMap, matching tsc.
    let source = r#"
class C {
  #x = 1;
  #m() { return this.#x; }
}
"#;
    let output = emit(source, ScriptTarget::ES5);
    assert!(
        output.contains("var _C_instances, _C_x, _C_m;\nvar C = /** @class */"),
        "instance method storage bundle must hoist before the IIFE.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "}());\n_C_x = new WeakMap(), _C_instances = new WeakSet(), _C_m = function _C_m()"
        ),
        "instance method storage inits must run after the IIFE.\nOutput:\n{output}"
    );
}

#[test]
fn es5_static_private_member_keeps_storage_inside_iife() {
    // Counter-case: a static private member forces the `_a = C` class alias, so
    // tsc keeps the whole storage bundle inside the IIFE. The lift must NOT fire.
    let source = r#"
class C {
  static #s = 1;
  read() { return C.#s; }
}
"#;
    let output = emit(source, ScriptTarget::ES5);
    let iife = output
        .find("var C = /** @class */")
        .expect("expected ES5 class IIFE");
    let ret = output.find("return C;").expect("expected IIFE return");
    let storage = output
        .find("var _a, _C_s;")
        .expect("expected in-IIFE storage decl");
    assert!(
        iife < storage && storage < ret,
        "static private storage must stay inside the IIFE.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var _a, _C_s;\nvar C ="),
        "static private storage must not be hoisted before the IIFE.\nOutput:\n{output}"
    );
}
