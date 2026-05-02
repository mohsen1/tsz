use crate::emitter::ScriptTarget;
use crate::output::printer::{PrintOptions, Printer};
use tsz_parser::ParserState;

fn parse_and_print(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    printer.finish().code
}

fn parse_and_print_for_target(source: &str, target: ScriptTarget) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let opts = PrintOptions {
        target,
        ..Default::default()
    };
    let mut printer = Printer::new(&parser.arena, opts);
    printer.set_source_text(source);
    printer.print(root);
    printer.finish().code
}

/// Regression test: trailing comments on static class fields must be
/// preserved when the field is lowered to `ClassName.field = value;`
/// for targets < ES2022.
#[test]
fn static_field_lowering_preserves_trailing_comment() {
    let source = "class C3 {\n    static intance = new C3(); // ok\n}\n";

    let output = parse_and_print_for_target(source, ScriptTarget::ES2017);

    // The lowered static field should preserve the trailing comment
    assert!(
        output.contains("C3.intance = new C3(); // ok"),
        "Trailing comment '// ok' should be preserved on lowered static field.\nOutput:\n{output}"
    );
}

/// Test: multiple static fields with trailing comments are all preserved.
#[test]
fn static_field_lowering_preserves_multiple_trailing_comments() {
    let source = "class Foo {\n    static a = 1; // first\n    static b = 2; // second\n}\n";

    let output = parse_and_print_for_target(source, ScriptTarget::ES2017);

    assert!(
        output.contains("Foo.a = 1; // first"),
        "Trailing comment '// first' should be preserved.\nOutput:\n{output}"
    );
    assert!(
        output.contains("Foo.b = 2; // second"),
        "Trailing comment '// second' should be preserved.\nOutput:\n{output}"
    );
}

/// Test: static fields without trailing comments still emit correctly.
#[test]
fn static_field_lowering_without_trailing_comment() {
    let source = "class Bar {\n    static x = 42;\n}\n";

    let output = parse_and_print_for_target(source, ScriptTarget::ES2017);

    assert!(
        output.contains("Bar.x = 42;"),
        "Static field should be lowered correctly.\nOutput:\n{output}"
    );
    // Should NOT have any trailing comment text
    assert!(
        !output.contains("Bar.x = 42; //"),
        "Should not have spurious trailing comment.\nOutput:\n{output}"
    );
}

#[test]
fn auto_accessor_instance_fields_emit_getter_setter_with_weakmap() {
    let source =
        "class RegularClass {\n    accessor shouldError: string; // Should still error\n}\n";

    let output = parse_and_print_for_target(source, ScriptTarget::ES2015);

    assert!(
        output.contains("var _RegularClass_shouldError_accessor_storage;"),
        "Auto-accessor storage declaration should be emitted.\nOutput:\n{output}"
    );
    assert!(
        output.contains("constructor() {",),
        "Constructor should be synthesized for auto-accessor initialization.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_RegularClass_shouldError_accessor_storage.set(this, void 0);"),
        "Auto-accessor storage should initialize to void 0 in constructor.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_RegularClass_shouldError_accessor_storage = new WeakMap();"),
        "Auto-accessor storage should be initialized with WeakMap after class body.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "get shouldError() { return __classPrivateFieldGet(this, _RegularClass_shouldError_accessor_storage, \"f\"); } // Should still error",
        ),
        "Auto accessor getter should be lowered.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "set shouldError(value) { __classPrivateFieldSet(this, _RegularClass_shouldError_accessor_storage, value, \"f\"); }",
        ),
        "Auto accessor setter should be lowered.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__classPrivateFieldGet"),
        "Private field helpers should be emitted.\nOutput:\n{output}"
    );
}

#[test]
fn auto_accessor_mixed_static_and_instance_fields_emit_expected_helpers_and_storage() {
    let source = "class C1 {\n    accessor a: any;\n    accessor b = 1;\n    static accessor c: any;\n    static accessor d = 2;\n}\n";

    let output = parse_and_print_for_target(source, ScriptTarget::ES2015);

    assert!(
        output.contains("var _a, _C1_a_accessor_storage, _C1_b_accessor_storage, _C1_c_accessor_storage, _C1_d_accessor_storage;"),
        "Static accessor lowering should emit helper storage declarations.\nOutput:\n{output}"
    );
    assert!(
        output.contains("constructor() {"),
        "Instance accessors should cause constructor synthesis.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_C1_a_accessor_storage.set(this, void 0);"),
        "Instance auto-accessor with no initializer should initialize to void 0.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_C1_b_accessor_storage.set(this, 1);"),
        "Instance auto-accessor initializer should assign 1 in constructor.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("this.b = 1"),
        "Instance auto-accessor should not emit direct property assignment.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "static get c() { return __classPrivateFieldGet(_a, _a, \"f\", _C1_c_accessor_storage); }"
        ),
        "Static auto-accessor getter should pass class alias + storage object.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "static set c(value) { __classPrivateFieldSet(_a, _a, value, \"f\", _C1_c_accessor_storage); }"
        ),
        "Static auto-accessor setter should pass class alias + storage object.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "static get d() { return __classPrivateFieldGet(_a, _a, \"f\", _C1_d_accessor_storage); }"
        ),
        "Static auto-accessor getter with initializer should preserve object-backed storage helper form.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "static set d(value) { __classPrivateFieldSet(_a, _a, value, \"f\", _C1_d_accessor_storage); }"
        ),
        "Static auto-accessor setter should preserve object-backed storage helper form.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_a = C1, _C1_a_accessor_storage = new WeakMap(), _C1_b_accessor_storage = new WeakMap();"),
        "Auto-accessor instance storage initialization should be emitted in one aliased statement.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_C1_c_accessor_storage = { value: void 0 }"),
        "Static accessor without initializer should default to void 0.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("C1.d = 2"),
        "Static auto-accessor declarations should not be lowered as static field assignments.\nOutput:\n{output}"
    );
}

#[test]
fn auto_accessor_fields_emit_private_storage_at_es2022() {
    let source = "class C1 {\n    accessor a: any;\n    accessor b = 1;\n    accessor #c: any;\n    accessor #d = 2;\n    static accessor e: any;\n    static accessor f = 3;\n}\n";

    let output = parse_and_print_for_target(source, ScriptTarget::ES2022);

    // ES2022 uses native private storage + getter/setter pairs.
    assert!(
        !output.contains("new WeakMap"),
        "ES2022 should not use WeakMap-backed storage for accessors.\nOutput:\n{output}"
    );
    assert!(
        output.contains("#a_accessor_storage;"),
        "Instance accessor `a` should create a private storage field.\nOutput:\n{output}"
    );
    assert!(
        output.contains("#b_accessor_storage = 1;"),
        "Instance accessor with initializer should inline field initializer.\nOutput:\n{output}"
    );
    assert!(
        output.contains("#c_accessor_storage;"),
        "Private accessor `#c` should create native private storage field.\nOutput:\n{output}"
    );
    assert!(
        output.contains("#d_accessor_storage = 2;"),
        "Private accessor with initializer should inline to private field.\nOutput:\n{output}"
    );
    assert!(
        output.contains("static #e_accessor_storage;"),
        "Static accessor should create native private storage field.\nOutput:\\n{output}"
    );
    assert!(
        output.contains("static #f_accessor_storage = 3;"),
        "Static accessor with initializer should inline static private field initializer.\nOutput:\\n{output}"
    );
    assert!(
        output.contains("get a() { return this.#a_accessor_storage; }"),
        "Instance accessor getter should reference private storage field.\nOutput:\\n{output}"
    );
    assert!(
        output.contains("set a(value) { this.#a_accessor_storage = value; }"),
        "Instance accessor setter should assign private storage field.\nOutput:\\n{output}"
    );
    assert!(
        output.contains("static get e() { return C1.#e_accessor_storage; }"),
        "Static accessor getter should reference class private storage field.\nOutput:\\n{output}"
    );
    assert!(
        output.contains("static set f(value) { C1.#f_accessor_storage = value; }"),
        "Static accessor setter should assign class private storage field.\nOutput:\\n{output}"
    );
    assert!(
        !output.contains("constructor() {"),
        "ES2022 accessor lowering should not synthesize a constructor here.\nOutput:\n{output}"
    );
}

#[test]
fn auto_accessor_private_storage_avoids_private_name_collisions_at_es2022() {
    let source = "class C2 {\n        #a1_accessor_storage = 1;\n        accessor a1 = 2;\n    }\n    \n    class C3 {\n        static #a2_accessor_storage = 1;\n        static {\n            class C3_Inner {\n                accessor a2 = 2;\n                static {\n                    #a2_accessor_storage in C3;\n                }\n            }\n        }\n    }\n";

    let output = parse_and_print_for_target(source, ScriptTarget::ES2022);

    assert!(
        output.contains("class C2"),
        "Source class with collision should be emitted.\nOutput:\n{output}"
    );
    assert!(
        output.contains("#a1_1_accessor_storage;"),
        "Public accessor `a1` should avoid colliding with existing #a1_accessor_storage.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("get a1() { return this.#a1_accessor_storage; }"),
        "a1 getter should not use unsuffixed storage name.\nOutput:\n{output}"
    );
    assert!(
        output.contains("get a1() { return this.#a1_1_accessor_storage; }"),
        "a1 getter should reference suffixed storage name.\nOutput:\n{output}"
    );
    assert!(
        output.contains("#a2_1_accessor_storage = 2;"),
        "Nested accessor `a2` should avoid collision with #a2_accessor_storage brand check usage.\nOutput:\n{output}"
    );
}

/// Regression test: class with lowered static fields followed by another
/// statement must not produce an extra blank line. The static field
/// emission ends with `write_line()` after `ClassName.field = value;`,
/// so the source-file-level loop must not add a second newline.
#[test]
fn no_extra_blank_line_after_static_field_lowering() {
    let source = "class Foo {\n    static x = 1;\n}\nconst y = 2;\n";

    let output = parse_and_print_for_target(source, ScriptTarget::ES2017);

    // Should have `Foo.x = 1;\n` immediately followed by `const y = 2;`
    // with NO blank line in between.
    assert!(
        output.contains("Foo.x = 1;\nconst y = 2;"),
        "Should not have blank line between lowered static field and next statement.\nOutput:\n{output}"
    );
}

/// Regression test: class with lowered static field inside a block
/// (e.g., for-loop body) must not produce an extra blank line before
/// the next statement in the block.
#[test]
fn no_extra_blank_line_after_static_field_in_block() {
    let source = "for (const x of [1]) {\n    class Row {\n        static factory = 1;\n    }\n    use(Row);\n}\n";

    let output = parse_and_print_for_target(source, ScriptTarget::ES2017);

    // Should have `Row.factory = 1;\n    use(Row);` with no blank line.
    assert!(
        !output.contains("Row.factory = 1;\n\n"),
        "Should not have blank line after lowered static field in block.\nOutput:\n{output}"
    );
}

/// Regression test: `export default class` with static field in CJS mode
/// must not produce a blank line between the lowered static field init
/// and the `exports.default = ClassName;` assignment.
#[test]
fn no_extra_blank_line_cjs_default_export_with_static_field() {
    use crate::emitter::ModuleKind;

    let source = "export default class MyComponent {\n    static create = 1;\n}\n";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let opts = PrintOptions {
        target: ScriptTarget::ES2017,
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut printer = Printer::new(&parser.arena, opts);
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    // Should have `MyComponent.create = 1;\n` followed by
    // `exports.default = MyComponent;` with NO blank line.
    assert!(
        !output.contains("MyComponent.create = 1;\n\n"),
        "Should not have blank line between lowered static field and CJS export.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports.default = MyComponent;"),
        "Should emit CJS default export assignment.\nOutput:\n{output}"
    );
}

/// Regression test: private fields (#name) with initializers must be
/// emitted verbatim at ES2022+ targets even when useDefineForClassFields
/// is false.  Private fields use native syntax at ES2022+ and are
/// unaffected by the useDefineForClassFields flag (which only controls
/// public field semantics).  Previously, the lowering skip logic dropped
/// them because `identifier_text()` returned empty for `PrivateIdentifier`
/// nodes, causing them to be neither collected for lowering NOR emitted
/// in the class body.
#[test]
fn private_field_with_initializer_emitted_at_es2022() {
    let source =
        "class A {\n    static #field = 10;\n    static #uninitialized;\n    #instance = 1;\n}\n";

    // PrintOptions defaults to use_define_for_class_fields: false via
    // PrinterOptions::default(), which triggers the class field lowering
    // path.  At ES2022+ the lowering should still preserve private fields
    // verbatim.
    let output = parse_and_print_for_target(source, ScriptTarget::ES2022);

    assert!(
        output.contains("static #field = 10;"),
        "Static private field with initializer should be emitted at ES2022.\nOutput:\n{output}"
    );
    assert!(
        output.contains("static #uninitialized;"),
        "Static private field without initializer should be emitted.\nOutput:\n{output}"
    );
    assert!(
        output.contains("#instance = 1;"),
        "Instance private field with initializer should be emitted at ES2022.\nOutput:\n{output}"
    );
}

/// Verify that private fields at targets below ES2022 are still handled
/// by the lowering path (not emitted verbatim with initializers).
#[test]
fn private_field_lowered_at_es2015() {
    let source = "class A {\n    static #field = 10;\n    #instance = 1;\n}\n";

    let output = parse_and_print_for_target(source, ScriptTarget::ES2015);

    // At ES2015, private fields should NOT appear in the class body
    // (they should be lowered to WeakMap-based patterns, though the
    // lowering transform itself may not fully emit them yet).
    assert!(
        !output.contains("static #field = 10;"),
        "Static private field should NOT be emitted verbatim at ES2015.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("#instance = 1;"),
        "Instance private field should NOT be emitted verbatim at ES2015.\nOutput:\n{output}"
    );
}

/// Regression test: bodyless method overload signatures are erased,
/// so their leading comments (JSDoc blocks) must not appear in the output.
/// Previously, `get_function()` was used instead of `get_method_decl()`,
/// so `is_erased` was always false for methods.
#[test]
fn overload_method_comments_erased() {
    let source = r#"class C {
    /** overload 1 */
    foo(x: number): number;
    /** overload 2 */
    foo(x: string): string;
    /** implementation */
    foo(x: any): any {
        return x;
    }
}"#;

    let output = parse_and_print_for_target(source, ScriptTarget::ESNext);

    // Overload JSDoc comments should NOT appear in the output
    assert!(
        !output.contains("overload 1"),
        "JSDoc for overload signature 1 should be erased.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("overload 2"),
        "JSDoc for overload signature 2 should be erased.\nOutput:\n{output}"
    );
    // Implementation JSDoc SHOULD appear
    assert!(
        output.contains("/** implementation */"),
        "JSDoc for implementation should be preserved.\nOutput:\n{output}"
    );
}

/// Regression test: bodyless constructor overload signatures are erased,
/// so their leading comments must not appear in the output.
/// Previously, `get_function()` was used instead of `get_constructor()`,
/// so `is_erased` was always false for constructors.
#[test]
fn overload_constructor_comments_erased() {
    let source = r#"class C {
    /** ctor overload 1 */
    constructor(x: number);
    /** ctor overload 2 */
    constructor(x: string);
    /** ctor implementation */
    constructor(x: any) {}
}"#;

    let output = parse_and_print_for_target(source, ScriptTarget::ESNext);

    // Overload JSDoc comments should NOT appear
    assert!(
        !output.contains("ctor overload 1"),
        "JSDoc for ctor overload 1 should be erased.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("ctor overload 2"),
        "JSDoc for ctor overload 2 should be erased.\nOutput:\n{output}"
    );
    // Implementation JSDoc SHOULD appear
    assert!(
        output.contains("/** ctor implementation */"),
        "JSDoc for ctor implementation should be preserved.\nOutput:\n{output}"
    );
}

/// Regression test: when a class member is erased (e.g., a type-only property
/// at ES2015+ with useDefineForClassFields=false), trailing comments on the same
/// line as the class closing `}` must NOT be consumed by the erased member's
/// comment skip logic. For example:
///   `class C extends E { foo: string; } // error`
/// The `// error` comment belongs to the `}`, not to the erased `foo: string;`.
#[test]
fn erased_member_does_not_consume_trailing_comment_after_closing_brace() {
    // Single-line class with an erased property and a trailing comment
    let source = "class C extends E { foo: string; } // error\n";

    let output = parse_and_print_for_target(source, ScriptTarget::ES2015);

    assert!(
        output.contains("// error"),
        "Trailing comment after closing brace should be preserved.\nOutput:\n{output}"
    );
}

/// Regression test: an erased member's OWN trailing comment (on the same
/// line, with only whitespace between the `;` and the comment) should still
/// be consumed. This ensures the fix for closing-brace comments doesn't
/// regress the basic erased-comment-suppression behavior.
#[test]
fn erased_interface_trailing_comment_is_suppressed() {
    let source = "interface Foo {} // type-only\nconst x = 1;\n";

    let output = parse_and_print_for_target(source, ScriptTarget::ESNext);

    assert!(
        !output.contains("// type-only"),
        "Trailing comment on erased interface should be suppressed.\nOutput:\n{output}"
    );
}

/// Abstract methods WITH a body (an error in TS, but tsc still emits them)
/// must NOT be erased — only bodyless methods should be erased.
#[test]
fn abstract_method_with_body_is_emitted() {
    let source = "abstract class H {\n    abstract baz(): number { return 1; }\n}\n";

    let output = parse_and_print_for_target(source, ScriptTarget::ESNext);

    assert!(
        output.contains("baz()"),
        "Abstract method with body should be emitted (tsc parity).\nOutput:\n{output}"
    );
}

/// Abstract methods WITHOUT a body should be erased (standard behavior).
#[test]
fn abstract_method_without_body_is_erased() {
    let source = "abstract class G {\n    abstract qux(): number;\n}\n";

    let output = parse_and_print_for_target(source, ScriptTarget::ESNext);

    assert!(
        !output.contains("qux"),
        "Abstract method without body should be erased.\nOutput:\n{output}"
    );
}

/// Bodyless non-abstract accessors (error case in TS) must NOT be erased.
#[test]
fn bodyless_non_abstract_accessor_is_not_erased() {
    let source = "class C {\n    get foo(): string;\n}\n";

    let output = parse_and_print_for_target(source, ScriptTarget::ESNext);

    assert!(
        output.contains("foo"),
        "Bodyless non-abstract accessor should be emitted (tsc parity).\nOutput:\n{output}"
    );
    assert!(
        output.contains("get foo() { }"),
        "Bodyless accessor body formatting should match tsc (`{{ }}`).\nOutput:\n{output}"
    );
}

/// Erased computed property names with potential side effects (property access)
/// must be emitted as standalone expression statements after the class body.
/// e.g., `[Symbol.iterator]: Type` → class body erased, then `Symbol.iterator;`
#[test]
fn computed_property_side_effect_property_access() {
    let source = "class C {\n    [Symbol.iterator]: any;\n}\n";

    let output = parse_and_print_for_target(source, ScriptTarget::ES2015);

    assert!(
        output.contains("}\nSymbol.iterator;"),
        "Computed property access expression should be emitted as side-effect statement.\nOutput:\n{output}"
    );
}

/// Simple identifier computed property names should NOT produce side-effect
/// statements — tsc does not emit them (no observable side effects).
#[test]
fn computed_property_no_side_effect_for_identifier() {
    let source = "class C {\n    [x]: string;\n}\n";

    let output = parse_and_print_for_target(source, ScriptTarget::ES2015);

    assert!(
        !output.contains("x;"),
        "Simple identifier computed property should NOT produce side-effect statement.\nOutput:\n{output}"
    );
}

/// String literal computed property names should NOT produce side-effect
/// statements — string literals have no observable side effects.
#[test]
fn computed_property_no_side_effect_for_string_literal() {
    let source = "class C {\n    [\"a\"]: string;\n}\n";

    let output = parse_and_print_for_target(source, ScriptTarget::ES2015);

    assert!(
        !output.contains("\"a\";"),
        "String literal computed property should NOT produce side-effect statement.\nOutput:\n{output}"
    );
}

/// Trailing comment on class body opening `{` should be suppressed.
/// tsc: `class E extends A {` (comment dropped)
#[test]
fn class_body_brace_trailing_comment_suppressed() {
    let source =
        "class E extends A { // error -- doesn't implement bar\n    foo() { return 1; }\n}\n";

    let output = parse_and_print(source);

    assert!(
        !output.contains("// error"),
        "Trailing comment on class body `{{` should be suppressed.\nOutput:\n{output}"
    );
}

/// Comment inside class body (not on opening brace) should still be preserved.
#[test]
fn class_body_inner_comment_preserved() {
    let source = "class C {\n    // this is a method\n    foo() { return 1; }\n}\n";

    let output = parse_and_print(source);

    assert!(
        output.contains("// this is a method"),
        "Leading comment of class member should be preserved.\nOutput:\n{output}"
    );
}

/// Trailing comment after closing `}` of an empty single-line class body must be
/// preserved.  Previously, `skip_trailing_same_line_comments` on the opening `{`
/// incorrectly consumed the comment because it used `node.end` (past the next
/// newline) as the scan boundary, making it think there was a newline between `{`
/// and the first member — when in fact the `{` and `}` are adjacent and the comment
/// follows `}`.
#[test]
fn empty_class_trailing_comment_preserved() {
    let source =
        "class Gen extends base() {}  // Error, T not in scope\nclass Spec extends Gen {}\n";

    let output = parse_and_print(source);

    assert!(
        output.contains("} // Error, T not in scope"),
        "Trailing comment after empty class `}}` should be preserved.\nOutput:\n{output}"
    );
}

/// Multi-line class body opening brace comment should still be suppressed.
#[test]
fn multiline_class_opening_brace_comment_still_suppressed() {
    let source = "class C { // opening comment\n    foo() { }\n}\n";

    let output = parse_and_print(source);

    assert!(
        !output.contains("// opening comment"),
        "Comment on multi-line class opening brace should be suppressed.\nOutput:\n{output}"
    );
}

/// `override` alone on a constructor parameter creates a parameter property.
/// The emitter must emit `this.p1 = p1;` in the constructor body.
#[test]
fn override_alone_is_parameter_property() {
    let source = r#"class Base { p1!: string; }
class C extends Base {
    constructor(override p1: "hello") {
        super();
    }
}"#;

    let output = parse_and_print_for_target(source, ScriptTarget::ES2015);

    assert!(
        output.contains("this.p1 = p1;"),
        "`override` on constructor parameter should emit `this.p1 = p1;`.\nOutput:\n{output}"
    );
}

/// `public override` on a constructor parameter should also emit `this.p1 = p1;`.
#[test]
fn public_override_is_parameter_property() {
    let source = r#"class Base { p1!: string; }
class C extends Base {
    constructor(public override p1: "hello") {
        super();
    }
}"#;

    let output = parse_and_print_for_target(source, ScriptTarget::ES2015);

    assert!(
        output.contains("this.p1 = p1;"),
        "`public override` on constructor parameter should emit `this.p1 = p1;`.\nOutput:\n{output}"
    );
}

/// `declare` class fields must NOT emit `this.X = X;` in the constructor.
/// They are ambient declarations that should be erased.
#[test]
fn declare_class_field_not_emitted_in_constructor() {
    let source = "class C {\n    declare foo = 1;\n    bar = 2;\n}\n";

    let output = parse_and_print_for_target(source, ScriptTarget::ES2015);

    assert!(
        !output.contains("this.foo"),
        "`declare` field should NOT emit `this.foo = 1;` in constructor.\nOutput:\n{output}"
    );
    assert!(
        output.contains("this.bar = 2;"),
        "Non-declare field should still emit in constructor.\nOutput:\n{output}"
    );
}

/// Trailing comment scan for the last class member must not overshoot into
/// comments belonging to the closing `}` line.
///
/// Without capping, a comment like `} // end` could be stolen by the last
/// member's trailing comment scanner and emitted after the member instead.
/// Uses a method declaration to avoid class field lowering at default target.
#[test]
fn trailing_comment_capped_at_class_close_brace() {
    let source = "class C {\n    m() {} // method comment\n} // end of class\n";

    let output = parse_and_print(source);

    // The method's trailing comment should stay with the method
    assert!(
        output.contains("// method comment"),
        "Method's trailing comment should be preserved.\nOutput:\n{output}"
    );
    // The closing brace comment must NOT be stolen by the method
    assert!(
        !output.contains("// method comment // end of class"),
        "Closing brace comment must not be stolen by last member.\nOutput:\n{output}"
    );
}

/// Regression test: malformed source where the first `{` found from
/// `node.pos` is inside a broken expression (not the class body) must not
/// cause a slice panic when the first member's `pos` precedes that `{`.
#[test]
fn malformed_source_class_brace_scan_no_panic() {
    // Simulate broken source where the parser produces a class whose first
    // member's position is before the `{` found by the byte scanner.
    // `retValue != 0 ^= {` creates a `{` inside a broken expression.
    let source = r#"class C {
    constructor() {
        if (x != 0 ^= {
            return 1;
        }
    }
}
"#;

    // Should not panic — just produce some output
    let _output = parse_and_print(source);
}

/// Class-expression comma-emission must interleave static field
/// initializers and static block IIFEs in source order so observable
/// evaluation order is preserved. Without the fix all static blocks
/// were emitted after all field initializers, so a static block that
/// reads `this.a` would see the value assigned by a later `static b = 2`
/// initializer instead of the in-source value.
/// Devin review: <https://github.com/mohsen1/tsz/pull/2279#discussion_r3176494185>
#[test]
fn test_class_expression_static_field_and_block_evaluation_order_preserved() {
    let source = r"const X = class {
    static a = 1;
    static { console.log(this.a); }
    static b = 2;
};
";
    let output = parse_and_print_for_target(source, ScriptTarget::ES2021);

    // Find the comma-list portion of the comma expression.
    let a_pos = output
        .find(".a = 1")
        .expect("static field `a = 1` must appear");
    let block_pos = output
        .find("(() =>")
        .expect("static block IIFE must appear");
    let b_pos = output
        .find(".b = 2")
        .expect("static field `b = 2` must appear");

    assert!(
        a_pos < block_pos && block_pos < b_pos,
        "static block must be emitted between `a = 1` and `b = 2` to match source order. \
         a_pos={a_pos}, block_pos={block_pos}, b_pos={b_pos}.\nOutput:\n{output}"
    );
}
