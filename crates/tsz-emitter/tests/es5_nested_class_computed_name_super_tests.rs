//! ES5 `super` lowering inside a nested class's computed property name.
//!
//! Structural rule: when a class is declared inside an enclosing *instance*
//! member body of a derived class, a `super` reference that appears in that
//! nested class's computed property name is evaluated in the enclosing member,
//! so it binds to the outer class's prototype home. At ES5 such a `super`
//! method call must lower to `_super.prototype.m.call(this)`, the instance
//! super form — not the class-definition static form `_super.m.call(this)`.
//!
//! A nested class declared inside a *static* member (or at a class-definition
//! site with no enclosing instance super home) keeps the established
//! static-like computed-name super access. The behaviour is keyed on the
//! structural instance-vs-static enclosing-member distinction, not on any
//! identifier spelling, so the tests vary the super method name, the outer and
//! nested class names, and the member name.

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

/// Reported shape: a nested class declaration inside an instance method whose
/// computed name calls `super.m()` lowers in instance super context.
#[test]
fn nested_class_decl_computed_name_super_call_is_prototype_qualified() {
    let source = r#"class A {
    foo() { return 1; }
}
class B extends A {
    foo() { return 2; }
    bar() {
        class Local {
            [super.foo()]() { return 100; }
        }
        return Local;
    }
}"#;
    let output = emit_es5(source);
    assert!(
        output.contains("_super.prototype.foo.call(this)"),
        "super method call in a nested class's computed name (enclosing instance \
         method) must be prototype-qualified.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("[_super.foo.call(this)]"),
        "must NOT use the static-context base for the instance-home super.\nOutput:\n{output}"
    );
}

/// Same rule with the super method, classes, and member names all renamed:
/// proves the behaviour is structural, not keyed on `foo`/`A`/`B`/`Local`.
#[test]
fn nested_class_decl_computed_name_super_call_renamed_members() {
    let source = r#"class Base {
    ping(): number { return 1; }
}
class Derived extends Base {
    ping(): number { return 2; }
    make() {
        class Inner {
            [super.ping()]() { return 7; }
        }
        return Inner;
    }
}"#;
    let output = emit_es5(source);
    assert!(
        output.contains("_super.prototype.ping.call(this)"),
        "renamed super method must still lower prototype-qualified.\nOutput:\n{output}"
    );
}

/// `super[expr]()` element-access form in the nested computed name lowers the
/// same way.
#[test]
fn nested_class_decl_computed_name_super_element_call_is_prototype_qualified() {
    let source = r#"class A {
    foo() { return 1; }
}
class B extends A {
    foo() { return 2; }
    bar() {
        class Local {
            [super["foo"]()]() { return 100; }
        }
        return Local;
    }
}"#;
    let output = emit_es5(source);
    assert!(
        output.contains("_super.prototype[\"foo\"].call(this)"),
        "super element-access call in a nested computed name (enclosing instance \
         method) must be prototype-qualified.\nOutput:\n{output}"
    );
}

/// Negative / fallback case: when the nested class is declared inside a
/// *static* member, there is no enclosing instance super home, so the computed
/// name keeps the static-like base (`_super.foo`), never `_super.prototype.foo`.
#[test]
fn nested_class_decl_in_static_member_keeps_static_super_base() {
    let source = r#"class A {
    static foo() { return 1; }
}
class B extends A {
    static foo() { return 2; }
    static bar() {
        class Local {
            [super.foo()]() { return 100; }
        }
        return Local;
    }
}"#;
    let output = emit_es5(source);
    assert!(
        !output.contains("_super.prototype.foo.call(this)"),
        "a nested class in a static member must not gain an instance-home super \
         base for its computed name.\nOutput:\n{output}"
    );
}

/// Same instance-home rule for the deferred class-expression path: the
/// returned anonymous class is emitted by a nested printer, but its computed
/// name still evaluates in the enclosing instance method.
#[test]
fn nested_class_expr_computed_name_super_call_is_prototype_qualified() {
    let source = r#"class A {
    foo() { return 1; }
}
class B extends A {
    foo() { return 2; }
    bar() {
        return class {
            [super.foo()]() { return 100; }
        };
    }
}"#;
    let output = emit_es5(source);
    assert!(
        output.contains("_super.prototype.foo.call(this)"),
        "super method call in a nested class expression's computed name must \
         inherit the enclosing instance super home.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("[_super.foo.call(this)]"),
        "deferred class-expression lowering must not fall back to the static \
         super base for an enclosing instance member.\nOutput:\n{output}"
    );
}

/// The deferred class-expression path also handles element access without
/// depending on a particular property spelling.
#[test]
fn nested_class_expr_computed_name_super_element_call_is_prototype_qualified() {
    let source = r#"class Parent {
    label() { return "ok"; }
}
class Child extends Parent {
    make() {
        return class {
            [super["label"]()]() { return 1; }
        };
    }
}"#;
    let output = emit_es5(source);
    assert!(
        output.contains("_super.prototype[\"label\"].call(this)"),
        "super element access in a deferred nested class expression's computed \
         name must inherit the enclosing instance super home.\nOutput:\n{output}"
    );
}

/// Static enclosing members do not provide an instance super home to deferred
/// class expressions.
#[test]
fn nested_class_expr_in_static_member_keeps_static_super_base() {
    let source = r#"class A {
    static foo() { return 1; }
}
class B extends A {
    static make() {
        return class {
            [super.foo()]() { return 100; }
        };
    }
}"#;
    let output = emit_es5(source);
    assert!(
        !output.contains("_super.prototype.foo.call(this)"),
        "a class expression inside a static member must not inherit an instance \
         super home.\nOutput:\n{output}"
    );
}
