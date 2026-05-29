//! ES5 object-literal accessor `super` base selection.
//!
//! Structural rule: when an ES5-lowered `super` property access sits inside an
//! object-literal accessor (`get`/`set`) whose home is the literal's
//! `__proto__`, the base is the bare home object (`_super.X`), never the
//! prototype-qualified class form (`_super.prototype.X`). The same accessor
//! syntax inside a derived class instead binds `super` to the class prototype.
//!
//! These tests vary member/property/class names so the behaviour is keyed on
//! the structural object-literal-vs-class distinction, not on any spelling.

use tsz_common::common::ScriptTarget;
use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_lower_print;

fn emit_es5(source: &str) -> String {
    let opts = PrintOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    parse_and_lower_print(source, opts)
}

/// A `get` accessor in an object literal must use `_super.X` (object-literal
/// home), not `_super.prototype.X`.
#[test]
fn object_literal_get_accessor_super_is_not_prototype_qualified() {
    let source = r#"var obj = {
    __proto__: { ping() {} },
    get value() {
        super.ping();
        return 1;
    }
};"#;
    let output = emit_es5(source);
    assert!(
        output.contains("_super.ping.call(this)"),
        "object-literal get accessor super should bind to the literal home (_super.ping).\nOutput:\n{output}"
    );
    assert!(
        !output.contains("_super.prototype.ping"),
        "object-literal get accessor super must NOT be prototype-qualified.\nOutput:\n{output}"
    );
}

/// A `set` accessor in an object literal must use `_super.X` too. Different
/// member/property names than the `get` test to prove the rule is structural.
#[test]
fn object_literal_set_accessor_super_is_not_prototype_qualified() {
    let source = r#"var bag = {
    __proto__: { notify() {} },
    set flag(next) {
        super.notify();
    }
};"#;
    let output = emit_es5(source);
    assert!(
        output.contains("_super.notify.call(this)"),
        "object-literal set accessor super should bind to the literal home (_super.notify).\nOutput:\n{output}"
    );
    assert!(
        !output.contains("_super.prototype.notify"),
        "object-literal set accessor super must NOT be prototype-qualified.\nOutput:\n{output}"
    );
}

/// Object-literal accessors nested inside a derived class method still bind
/// `super` to the literal home (the object-literal accessor establishes its own
/// non-prototype super home, independent of the enclosing class).
#[test]
fn object_literal_accessor_inside_class_method_uses_literal_home() {
    let source = r#"class Animal { roar() {} }
class Lion extends Animal {
    build() {
        var spec = {
            __proto__: { roar() {} },
            get loud() {
                super.roar();
                return 2;
            }
        };
        return spec;
    }
}"#;
    let output = emit_es5(source);
    assert!(
        output.contains("_super.roar.call(this)"),
        "object-literal accessor inside a class method should use the literal home (_super.roar).\nOutput:\n{output}"
    );
    assert!(
        !output.contains("_super.prototype.roar.call(this)"),
        "object-literal accessor inside a class method must NOT be prototype-qualified.\nOutput:\n{output}"
    );
}

/// Negative/contrast case: a `get` accessor declared directly on a derived
/// class binds `super` to the class prototype (`_super.prototype.X`). This
/// proves the fix did not over-broaden to all accessors.
#[test]
fn class_get_accessor_super_remains_prototype_qualified() {
    let source = r#"class Base { greet() {} }
class Greeter extends Base {
    get message() {
        super.greet();
        return 3;
    }
}"#;
    let output = emit_es5(source);
    assert!(
        output.contains("_super.prototype.greet.call(this)"),
        "class get accessor super must remain prototype-qualified (_super.prototype.greet).\nOutput:\n{output}"
    );
}
