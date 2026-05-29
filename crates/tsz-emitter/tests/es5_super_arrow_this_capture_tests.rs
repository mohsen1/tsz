//! ES5 lexical-`this` capture for `super` inside arrow functions.
//!
//! Structural rules under test:
//!
//! 1. A `super.m(...)` / `super[e](...)` **call** inside an arrow lowers to
//!    `_super.prototype.m.call(_this, ...)`: the arrow captures lexical `this`,
//!    so the enclosing scope emits `var _this = this;` and the synthesized
//!    `.call(...)` receiver is the captured `_this`.
//! 2. A bare `super.x` property **access** inside an arrow lowers to
//!    `_super.prototype.x` and references no `this`, so it does NOT by itself
//!    force a `var _this = this;` capture.
//! 3. In a top-level object literal, an arrow method's `super.m(...)` call uses
//!    the captured top-level `_this` receiver, while non-arrow methods use
//!    their own `this`.
//!
//! Tests vary class/method/property names and the super shape so the behaviour
//! is keyed on the structural arrow-vs-method and call-vs-access distinctions,
//! not on any spelling.

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

/// A `super.m()` call inside an arrow in a class method captures `_this`:
/// the receiver of the lowered `.call(...)` must be `_this`, and the method
/// body must declare `var _this = this;`.
#[test]
fn super_call_in_arrow_captures_this_receiver() {
    let source = r#"class Base { greet() {} }
class Derived extends Base {
    greet() {
        var run = () => super.greet();
    }
}"#;
    let output = emit_es5(source);
    assert!(
        output.contains("_super.prototype.greet.call(_this)"),
        "super call inside an arrow must thread the captured receiver `_this`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var _this = this;"),
        "method with a super-calling arrow must capture lexical `this`.\nOutput:\n{output}"
    );
}

/// Same structural rule with different class/method names and an element-access
/// super call (`super["..."]()`): the fix must not depend on any spelling.
#[test]
fn super_element_call_in_arrow_captures_this_receiver_renamed() {
    let source = r#"class Animal { speak() {} }
class Dog extends Animal {
    bark() {
        var go = () => super["speak"]();
    }
}"#;
    let output = emit_es5(source);
    assert!(
        output.contains(r#"_super.prototype["speak"].call(_this)"#),
        "super element-access call inside an arrow must thread `_this`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var _this = this;"),
        "method with a super-calling arrow must capture lexical `this`.\nOutput:\n{output}"
    );
}

/// A bare `super.x` property access inside an arrow does NOT need a `_this`
/// capture: the lowered form is `_super.prototype.x`, which references no
/// `this`. The enclosing method must not emit a spurious `var _this = this;`.
#[test]
fn super_property_access_in_arrow_does_not_capture_this() {
    let source = r#"class Base { value = 1; }
class Derived extends Base {
    read() {
        var pick = () => super.value;
    }
}"#;
    let output = emit_es5(source);
    assert!(
        output.contains("_super.prototype.value"),
        "super property access must remain prototype-qualified.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var _this = this;"),
        "a super property-access-only arrow must not force a `_this` capture.\nOutput:\n{output}"
    );
}

/// Renamed variant of the property-access rule (different class/property names)
/// proving the no-capture behaviour is structural, not name-based.
#[test]
fn super_property_access_in_arrow_does_not_capture_this_renamed() {
    let source = r#"class Shape { area = 0; }
class Circle extends Shape {
    measure() {
        var grab = () => super.area;
    }
}"#;
    let output = emit_es5(source);
    assert!(
        output.contains("_super.prototype.area"),
        "super property access must remain prototype-qualified.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var _this = this;"),
        "a super property-access-only arrow must not force a `_this` capture.\nOutput:\n{output}"
    );
}

/// In a top-level object literal, an arrow method's `super.m()` call uses the
/// captured top-level `_this`, while a non-arrow method uses its own `this`.
#[test]
fn object_literal_arrow_super_call_uses_captured_receiver() {
    let source = r#"var obj = {
    __proto__: { method() {} },
    direct: function () { super.method(); },
    lambda: () => { super.method(); }
};"#;
    let output = emit_es5(source);
    assert!(
        output.contains("var _this = this;"),
        "top-level object literal with a super-calling arrow must capture `_this`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_super.method.call(_this)"),
        "arrow object-literal method super call must use the captured `_this`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_super.method.call(this)"),
        "non-arrow object-literal method super call must use its own `this`.\nOutput:\n{output}"
    );
}

/// Negative/fallback case: an arrow with neither `this` nor a super call must
/// not capture `_this`, even when it lives in a derived class method.
#[test]
fn plain_arrow_without_this_or_super_call_does_not_capture() {
    let source = r#"class Base { run() {} }
class Derived extends Base {
    run() {
        var add = (a, b) => a + b;
    }
}"#;
    let output = emit_es5(source);
    assert!(
        !output.contains("var _this = this;"),
        "an arrow that uses neither `this` nor a super call must not capture `_this`.\nOutput:\n{output}"
    );
}
