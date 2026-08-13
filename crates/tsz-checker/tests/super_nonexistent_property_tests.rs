//! `super.<name>` that does not resolve on the receiver side must reach the
//! nonexistent-property diagnostics, exactly like an ordinary access.
//!
//! Regression for the second half of the `super` property-access family
//! (issue #17370): a `super.` access that does not resolve on the receiver side
//! — the base *instance* type in an instance context, the base *constructor*
//! type in a static context — is reported through `reportNonexistentProperty`:
//!   * TS2576 with the "did you mean to access the static member" suggestion
//!     when the name exists on the class's static side and the receiver is the
//!     instance type (`super.S1` in an instance method);
//!   * plain TS2339 against the receiver type (`typeof C` in static contexts)
//!     otherwise (`super.x` in a static method, `x` instance-side).
//!
//! The element-access path (`super["name"]`) already did this; the
//! property-access path now converges. A `super` that carries grammar errors
//! (e.g. `super` in a parameter default) suppresses these dependent
//! diagnostics for the file, matching tsc and the sibling super accessibility
//! gate.
//!
//! Binder names are varied across cases so no diagnostic is keyed on a
//! particular identifier.

use tsz_checker::test_utils::{
    check_source_code_messages, check_source_codes, check_source_codes_with_parse_health,
};

fn codes(msgs: &[(u32, String)]) -> Vec<u32> {
    msgs.iter().map(|(c, _)| *c).collect()
}

/// Instance-context `super.member` where `member` exists only on the base
/// static side → TS2576 with the static-member suggestion.
#[test]
fn instance_super_static_only_member_is_ts2576_with_suggestion() {
    let source = r#"
class Animal {
    static Legs: number = 4;
}
class Dog extends Animal {
    bark() {
        return super.Legs;
    }
}
"#;
    let msgs = check_source_code_messages(source);
    let (_, message) = msgs
        .iter()
        .find(|(c, _)| *c == 2576)
        .unwrap_or_else(|| panic!("expected TS2576 for super.Legs; got {:?}", codes(&msgs)));
    assert!(
        message.contains("Legs") && message.contains("Animal") && message.contains("static member"),
        "TS2576 message should name the property, base type, and static-member suggestion; got: {message}",
    );
}

/// A method that exists only on the base static side, accessed through an
/// instance-context `super.`, is TS2576 as well (`superPropertyAccess2.ts`
/// `super.bar` in the constructor).
#[test]
fn instance_super_static_only_method_is_ts2576() {
    let source = r#"
class Base {
    static helper() {}
    get value() { return 1; }
}
class Derived extends Base {
    constructor() {
        super();
        super.helper();
    }
}
"#;
    let cs = check_source_codes(source);
    assert!(
        cs.contains(&2576),
        "expected TS2576 for super.helper (static-only method via instance super); got {cs:?}",
    );
}

/// Static-context `super.member` where `member` exists only on the base
/// instance side → plain TS2339 against `typeof C`.
#[test]
fn static_super_instance_only_member_is_ts2339_typeof() {
    let source = r#"
class Widget {
    render(): number { return 0; }
}
class Button extends Widget {
    static build() {
        return super.render();
    }
}
"#;
    let msgs = check_source_code_messages(source);
    let (_, message) = msgs.iter().find(|(c, _)| *c == 2339).unwrap_or_else(|| {
        panic!(
            "expected TS2339 for super.render in static ctx; got {:?}",
            codes(&msgs)
        )
    });
    assert!(
        message.contains("render") && message.contains("typeof"),
        "static-context TS2339 should render the constructor type `typeof C`; got: {message}",
    );
    assert!(
        !codes(&msgs).contains(&2576),
        "static-context instance-member miss must not use the TS2576 static-suggestion form; got {:?}",
        codes(&msgs),
    );
}

/// A genuinely absent base member (neither side) still reports TS2339, not a
/// spurious static suggestion.
#[test]
fn instance_super_absent_member_is_plain_ts2339() {
    let source = r#"
class Vehicle {
    speed: number = 0;
}
class Car extends Vehicle {
    go() {
        return super.altitude;
    }
}
"#;
    let cs = check_source_codes(source);
    assert!(
        cs.contains(&2339),
        "expected TS2339 for a genuinely absent super member; got {cs:?}",
    );
    assert!(
        !cs.contains(&2576),
        "an absent member has no static side to suggest; got {cs:?}",
    );
}

/// A valid `super.member` that resolves on the receiver side emits neither
/// TS2576 nor TS2339 (`superPropertyAccess1.ts` shape).
#[test]
fn resolving_super_member_has_no_nonexistent_diagnostic() {
    let source = r#"
class Shape {
    area(): number { return 0; }
    get label() { return "s"; }
}
class Circle extends Shape {
    describe() {
        super.area();
        return super.label;
    }
}
"#;
    let cs = check_source_codes(source);
    assert!(
        !cs.contains(&2576) && !cs.contains(&2339),
        "resolving super member must not report a nonexistent-property diagnostic; got {cs:?}",
    );
}

/// A `super` carrying grammar errors (a `super` parameter default emits TS1034)
/// suppresses the dependent super nonexistent-property diagnostics for the
/// file, matching tsc (`superAccess2.ts` → TS1034 only) and the super
/// accessibility gate.
#[test]
fn grammar_erroneous_super_suppresses_nonexistent_diagnostics() {
    let source = r#"
class Parent {
    inst() {}
    static stat() {}
}
class Child extends Parent {
    run(cb = super) {
        super.stat();
    }
    static make(cb = super) {
        super.inst();
    }
}
"#;
    // `check_source_codes_with_parse_health` wires the parser diagnostics and
    // sets `has_syntax_parse_errors`, exactly like `tsz-cli`; the plain
    // `check_source_codes` harness never sees the parse error, so the guard
    // could not be exercised there.
    let cs = check_source_codes_with_parse_health(source);
    assert!(
        cs.contains(&1034),
        "expected the TS1034 grammar error for `super` in a parameter default; got {cs:?}",
    );
    assert!(
        !cs.contains(&2576) && !cs.contains(&2339),
        "grammar-erroneous super must suppress the dependent nonexistent-property diagnostics; got {cs:?}",
    );
}

/// A `super.member` whose `super` crosses a regular-function boundary is an
/// invalid reference (TS2660), so the dependent nonexistent-property
/// diagnostics are suppressed even though `member` exists on the base static
/// side. Regression for the #17370 fix's parse-error-only guard, which did not
/// cover the no-parse-error function-boundary case
/// (`typeOfThisInStaticMembers9.ts`: `static f = 1` in the base,
/// `function () { return super.f }` in the derived static field).
#[test]
fn function_boundary_super_static_member_suppresses_ts2576() {
    let source = r#"
class Root {
    static tag: number = 1;
}
class Leaf extends Root {
    static viaFunctionExpr = function () { return super.tag };
}
"#;
    let cs = check_source_codes(source);
    assert!(
        cs.contains(&2660),
        "a regular-function boundary breaks the super binding (TS2660); got {cs:?}",
    );
    assert!(
        !cs.contains(&2576),
        "an invalid (function-boundary) super must not gain the TS2576 static suggestion; got {cs:?}",
    );
}

/// A `super.member` inside a nested class expression with no base of its own is
/// an invalid reference (TS2335); the dependent nonexistent-property
/// diagnostics are suppressed even though `member` exists on the outer base's
/// static side.
#[test]
fn nested_nonderived_class_super_static_member_suppresses_ts2576() {
    let source = r#"
class Root {
    static tag: number = 1;
}
class Leaf extends Root {
    static viaNestedClass = class { field = super.tag };
}
"#;
    let cs = check_source_codes(source);
    assert!(
        cs.contains(&2335),
        "a nested non-derived class makes super invalid (TS2335); got {cs:?}",
    );
    assert!(
        !cs.contains(&2576),
        "an invalid (nested-non-derived) super must not gain the TS2576 static suggestion; got {cs:?}",
    );
}
