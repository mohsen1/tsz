//! When `super` is captured for an async method/generator/arrow via the
//! `_super = Object.create(null, { x: { get, set } })` / `_superIndex`
//! value-accessor objects, writes to `super.x` / `super["x"]` must go
//! directly through the captured accessor (`_super.x = v`,
//! `_superIndex("x").value = v`, and destructuring `({ f: _super.x } = ...)`),
//! never through the `Reflect.set(_super, "x", v, this)` form. The
//! `Reflect.get`/`Reflect.set` rewrite is only correct for the static-member
//! super alias, so it must be preserved there.

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_print_named_with_opts;

fn emit_es2015(source: &str) -> String {
    let opts = PrintOptions {
        module: ModuleKind::ESNext,
        target: ScriptTarget::ES2015,
        ..Default::default()
    };
    parse_and_print_named_with_opts("a.ts", source, opts)
}

/// Property write through the captured `_super` accessor object.
#[test]
fn async_method_super_property_write_uses_captured_accessor() {
    let source = r#"
class A { x() {} }
class B extends A {
    async m() {
        const f = () => {};
        super.x();
        super.x = f;
    }
}
"#;
    let output = emit_es2015(source);
    assert!(
        output.contains("_super.x = f;"),
        "expected direct `_super.x = f` write. Output:\n{output}"
    );
    assert!(
        !output.contains("Reflect.set"),
        "must NOT use Reflect.set for async-captured super write. Output:\n{output}"
    );
}

/// The rule is about the captured-accessor shape, not the property spelling:
/// renaming the property keeps the direct-accessor write.
#[test]
fn async_method_super_property_write_is_not_name_keyed() {
    let source = r#"
class A { renamed() {} }
class B extends A {
    async m() {
        const f = () => {};
        super.renamed();
        super.renamed = f;
    }
}
"#;
    let output = emit_es2015(source);
    assert!(
        output.contains("_super.renamed = f;"),
        "expected direct `_super.renamed = f` write. Output:\n{output}"
    );
    assert!(!output.contains("Reflect.set"), "Output:\n{output}");
}

/// Element write goes through the `_superIndex("x").value` value accessor.
#[test]
fn async_method_super_element_write_uses_index_value_accessor() {
    let source = r#"
class A { x() {} }
class B extends A {
    async m() {
        const f = () => {};
        super["x"]();
        super["x"] = f;
    }
}
"#;
    let output = emit_es2015(source);
    assert!(
        output.contains("_superIndex(\"x\").value = f;"),
        "expected `_superIndex(\"x\").value = f` write. Output:\n{output}"
    );
    assert!(!output.contains("Reflect.set"), "Output:\n{output}");
}

/// Destructuring assignment target through the captured accessor must stay a
/// plain reference (`({ f: _super.x } = ...)`), not the synthetic
/// `({ set value(_a) { Reflect.set(...) } }).value` wrapper.
#[test]
fn async_method_super_destructuring_target_uses_captured_accessor() {
    let source = r#"
class A { x() {} }
class B extends A {
    async m() {
        const f = () => {};
        super.x = f;
        ({ f: super.x } = { f });
    }
}
"#;
    let output = emit_es2015(source);
    assert!(
        output.contains("({ f: _super.x } = { f });"),
        "expected direct destructuring target through _super.x. Output:\n{output}"
    );
    assert!(
        !output.contains("set value(_a)"),
        "must NOT synthesize a setter-object wrapper. Output:\n{output}"
    );
    assert!(!output.contains("Reflect.set"), "Output:\n{output}");
}

/// Async-captured super writes inside an async *generator* method use the same
/// captured accessors.
#[test]
fn async_generator_super_write_uses_captured_accessor() {
    let source = r#"
class A { x() {} }
class B extends A {
    async *m() {
        const f = () => {};
        super.x = f;
        super["x"] = f;
    }
}
"#;
    let output = emit_es2015(source);
    assert!(output.contains("_super.x = f;"), "Output:\n{output}");
    assert!(
        output.contains("_superIndex(\"x\").value = f;"),
        "Output:\n{output}"
    );
    assert!(!output.contains("Reflect.set"), "Output:\n{output}");
}

/// Negative / preservation case: static-member super (no async capture) must
/// still lower writes through `Reflect.set` against the static base alias.
/// Changing the captured-super write path must not disturb this.
#[test]
fn static_member_super_write_still_uses_reflect() {
    let source = r#"
class Base { static x = 1; }
class Derived extends Base {
    static {
        super.x = 2;
    }
}
"#;
    let opts = PrintOptions {
        module: ModuleKind::ESNext,
        target: ScriptTarget::ES2015,
        ..Default::default()
    };
    let output = parse_and_print_named_with_opts("a.ts", source, opts);
    assert!(
        output.contains("Reflect.set"),
        "static-member super write must keep Reflect.set. Output:\n{output}"
    );
}
