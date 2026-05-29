//! `super` error-recovery emit: bare `super` not followed by an argument list
//! or member access.
//!
//! Structural rule: when `super` is not followed by `(`, `.`, `[`, or `<`, tsc
//! recovers by parsing it as a `super.<missing-identifier>` property access
//! (it consumes an expected `.` token and a missing right-hand identifier).
//! That property access is then emitted verbatim as `super.`, and — when it
//! appears in a static-member context that requires downleveling — it is
//! lowered through the static-member `super` rewrite with an empty property
//! key (`Reflect.get(base, "", self)`). Before this fix tsz parsed bare
//! `super` as a lone `SuperKeyword`, emitting `super` (no trailing dot) and
//! skipping the static-super lowering.
//!
//! These tests vary class/member names so the behaviour is keyed on the
//! structural "super has no following member access" shape, not on any
//! particular identifier spelling.

use tsz_common::common::ScriptTarget;
use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::{parse_and_lower_print, parse_and_print_with_opts};

fn print_es2015(source: &str) -> String {
    let opts = PrintOptions {
        target: ScriptTarget::ES2015,
        ..Default::default()
    };
    parse_and_print_with_opts(source, opts)
}

fn lower_es2015(source: &str) -> String {
    let opts = PrintOptions {
        target: ScriptTarget::ES2015,
        ..Default::default()
    };
    parse_and_lower_print(source, opts)
}

/// Bare `super` in a non-class function context emits `super.` verbatim (the
/// recovered missing-member property access), not a lone `super`.
#[test]
fn bare_super_in_function_emits_trailing_dot() {
    let source = "function outer() {\n    var captured = super;\n}\n";
    let output = print_es2015(source);
    assert!(
        output.contains("var captured = super.;"),
        "bare `super` should recover as `super.` (missing-member property access).\nOutput:\n{output}"
    );
    assert!(
        !output.contains("super;"),
        "bare `super` must not be emitted as a lone `super` keyword.\nOutput:\n{output}"
    );
}

/// Same rule with a different identifier and an arrow body, proving the fix is
/// keyed on the structural "no member access after super" shape.
#[test]
fn bare_super_in_arrow_emits_trailing_dot() {
    let source = "function host() {\n    var ref = () => super;\n}\n";
    let output = print_es2015(source);
    assert!(
        output.contains("() => super."),
        "bare `super` in an arrow should recover as `super.`.\nOutput:\n{output}"
    );
}

/// A bare `super` inside a static field initializer must be lowered through the
/// static-member `super` rewrite with an empty property key.
#[test]
fn bare_super_in_static_field_lowers_to_reflect_get_empty_key() {
    let source = concat!(
        "class Animal { static run() {} }\n",
        "class Dog extends Animal {\n",
        "    static legs = super;\n",
        "}\n",
    );
    let output = lower_es2015(source);
    assert!(
        output.contains("Reflect.get(") && output.contains(", \"\", "),
        "bare `super` in a static field should lower to `Reflect.get(base, \"\", self)`.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("= super;") && !output.contains("= super,"),
        "bare `super` in a static field must not survive as a lone `super`.\nOutput:\n{output}"
    );
}

/// Same static-member lowering with different class/member names, confirming
/// the empty-key `Reflect.get` rewrite is structural, not name-specific.
#[test]
fn bare_super_in_static_field_lowers_for_other_names() {
    let source = concat!(
        "class Vehicle { static start() {} }\n",
        "class Car extends Vehicle {\n",
        "    static wheels = super;\n",
        "}\n",
    );
    let output = lower_es2015(source);
    assert!(
        output.contains("Reflect.get(") && output.contains(", \"\", "),
        "renamed bare-super static field should still lower to an empty-key `Reflect.get`.\nOutput:\n{output}"
    );
}

/// Negative / regression case: a valid `super.member` access is unaffected by
/// the recovery path — it keeps its real property name and is not collapsed to
/// the missing-member form.
#[test]
fn valid_super_member_access_is_unaffected() {
    let source = concat!(
        "class Base { greet() {} }\n",
        "class Sub extends Base {\n",
        "    hello() { super.greet(); }\n",
        "}\n",
    );
    let output = print_es2015(source);
    assert!(
        output.contains("super.greet()"),
        "valid `super.greet()` must be preserved.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("super.;"),
        "valid super access must not produce a missing-member `super.`.\nOutput:\n{output}"
    );
}
