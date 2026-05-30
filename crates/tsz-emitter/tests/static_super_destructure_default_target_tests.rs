//! Valid lowering for a `super`-property used as a destructuring
//! assignment-*target* with a default, inside a static class member.
//!
//! When a static field initializer at ES2015+ (with
//! `useDefineForClassFields`) contains a destructuring assignment whose target
//! element is a `super` property with a default — e.g.
//! `static z = [super.a = 0] = [0]` or `static z = { x: super.a = 0 } = ...` —
//! the `super.a = 0` element is an assignment *target* (an array/object pattern
//! element with a default), not a value-producing assignment expression.
//!
//! tsc emits the scoped-static-super setter-descriptor member-expression as the
//! target, with the right-hand side as the destructuring default:
//!
//! ```js
//! [({ set value(_a) { Reflect.set(_b, "a", _a, _b); } }).value = 0] = [0]
//! ```
//!
//! Previously tsz lowered the inner `super.a = 0` through the value/IIFE path,
//! emitting `[(() => { ...; return ...; })()] = [0]`. An IIFE *call* is not a
//! legal destructuring assignment target, so that output is a `SyntaxError`
//! (non-runnable JS). These tests pin the valid setter-descriptor shape and
//! guard against the IIFE-as-target regression. They intentionally vary the
//! class/member/property identifier spellings so the rule cannot be satisfied
//! by name-matching.

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_lower_print as parse_lower_emit;

fn es2015_define_opts() -> PrintOptions {
    PrintOptions {
        target: ScriptTarget::ES2015,
        module: ModuleKind::ESNext,
        use_define_for_class_fields: true,
        ..Default::default()
    }
}

/// An IIFE in array/object destructuring assignment-target position is invalid
/// JS. No valid lowering for these shapes should ever contain one.
fn assert_no_iife_in_target(output: &str) {
    assert!(
        !output.contains("(() => {"),
        "static-super destructure target must not lower to an IIFE \
         (invalid destructuring assignment target).\nOutput:\n{output}"
    );
    assert!(
        !output.contains("})()] ="),
        "found an IIFE call as an array-destructuring element target.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("})() }"),
        "found an IIFE call as an object-destructuring element target.\nOutput:\n{output}"
    );
}

#[test]
fn array_destructure_super_target_with_default_uses_setter_descriptor() {
    // z9 witness shape: `[super.a = 0] = [0]`.
    let source = "\
declare class B { static a: any; }
class C extends B {
    static z = [super.a = 0] = [0];
}
";
    let output = parse_lower_emit(source, es2015_define_opts());

    assert_no_iife_in_target(&output);
    // The `super.a` target renders as the setter-descriptor member-expression,
    // and the default `= 0` follows it as the destructuring default.
    assert!(
        output.contains("set value(_a) { Reflect.set("),
        "expected scoped-static-super setter-descriptor for the target.\nOutput:\n{output}"
    );
    assert!(
        output.contains(".value = 0] = [0]"),
        "expected `({{ set value(_a) {{ ... }} }}).value = 0] = [0]` target form.\nOutput:\n{output}"
    );
}

#[test]
fn object_destructure_super_target_with_default_uses_setter_descriptor() {
    // z12 witness shape: `{ x: super.a = 0 } = { x: 0 }`.
    let source = "\
declare class B { static a: any; }
class C extends B {
    static z = { x: super.a = 0 } = { x: 0 };
}
";
    let output = parse_lower_emit(source, es2015_define_opts());

    assert_no_iife_in_target(&output);
    assert!(
        output.contains("set value(_a) { Reflect.set("),
        "expected scoped-static-super setter-descriptor for the target.\nOutput:\n{output}"
    );
    assert!(
        output.contains(".value = 0 } = { x: 0 }"),
        "expected `{{ x: ({{ set value(_a) {{ ... }} }}).value = 0 }} = {{ x: 0 }}` form.\nOutput:\n{output}"
    );
}

#[test]
fn renamed_class_member_property_still_uses_valid_target_lowering() {
    // Same rule, different identifier spellings (class `Widget`, base `Base`,
    // static member `slot`, super property `count`). If the fix were keyed on
    // a specific name it would regress here.
    let source = "\
declare class Base { static count: any; }
class Widget extends Base {
    static slot = [super.count = 7] = [7];
}
";
    let output = parse_lower_emit(source, es2015_define_opts());

    assert_no_iife_in_target(&output);
    assert!(
        output.contains("set value(_a) { Reflect.set("),
        "expected setter-descriptor target regardless of identifier names.\nOutput:\n{output}"
    );
    assert!(
        output.contains(".value = 7] = [7]"),
        "expected the renamed-case array target to carry its default.\nOutput:\n{output}"
    );
}

#[test]
fn element_access_super_target_with_default_uses_setter_descriptor() {
    // Super *element* access target with a default: `[super["a"] = 0] = [0]`.
    // Exercises the element-access branch, not just property access.
    let source = "\
declare class B { static a: any; }
class C extends B {
    static z = [super[\"a\"] = 0] = [0];
}
";
    let output = parse_lower_emit(source, es2015_define_opts());

    assert_no_iife_in_target(&output);
    assert!(
        output.contains("set value(_a) { Reflect.set("),
        "expected setter-descriptor target for super element-access.\nOutput:\n{output}"
    );
    assert!(
        output.contains(".value = 0] = [0]"),
        "expected element-access array target to carry its default.\nOutput:\n{output}"
    );
}

#[test]
fn super_target_without_default_unaffected() {
    // Negative/baseline: a super target *without* a default (z8 shape) already
    // used the setter-descriptor and must keep doing so (no IIFE, no `.value =`
    // default appended).
    let source = "\
declare class B { static a: any; }
class C extends B {
    static z = [super.a] = [0];
}
";
    let output = parse_lower_emit(source, es2015_define_opts());

    assert_no_iife_in_target(&output);
    assert!(
        output.contains("set value(_a) { Reflect.set("),
        "expected setter-descriptor target for the no-default case.\nOutput:\n{output}"
    );
    assert!(
        output.contains(".value] = [0]"),
        "no-default target must remain `({{ ... }}).value] = [0]`.\nOutput:\n{output}"
    );
}
