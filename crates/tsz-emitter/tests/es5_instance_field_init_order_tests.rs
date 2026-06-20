//! Regression tests for ES5 class lowering: instance field initializers
//! (public property assignments, private-field `WeakMap` stores, and
//! auto-accessor stores) must run in **source declaration order**, exactly as
//! `tsc` emits them.
//!
//! Previously the ES5 constructor lowering emitted every private-field /
//! auto-accessor store as one block *before* the public property assignments,
//! reordering observable initializer side effects. For `class C { a = 1; #p =
//! 2; b = 3; }` `tsc` emits `this.a = 1; _C_p.set(this, 2); this.b = 3;` while
//! tsz emitted `_C_p.set(this, 2); this.a = 1; this.b = 3;`.
//!
//! The constructor emission order `tsc` uses (and these tests lock) is:
//! private brand `add(this)` → parameter-property assignments → instance field
//! initializers in textual order → original constructor body.
//!
//! Binder names are deliberately varied (no `C`/`a`/`#p` reliance) so the
//! assertions track the structural declaration order, not any spelling.

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_lower_print as emit;

fn es5() -> PrintOptions {
    PrintOptions {
        target: ScriptTarget::ES5,
        module: ModuleKind::ESNext,
        ..Default::default()
    }
}

/// Assert that each needle appears in `haystack` and that they occur in the
/// given order.
#[track_caller]
fn assert_order(haystack: &str, needles: &[&str]) {
    let mut last = 0usize;
    for needle in needles {
        match haystack[last..].find(needle) {
            Some(rel) => last += rel + needle.len(),
            None => panic!(
                "expected to find {needle:?} after offset {last} in order {needles:?}\nOutput:\n{haystack}"
            ),
        }
    }
}

#[test]
fn public_private_public_interleave_keeps_source_order() {
    // a (public) ; #secret (private) ; omega (public)
    let src = "class Widget { alpha = 1; #secret = 2; omega = 3; }";
    let out = emit(src, es5());
    assert_order(
        &out,
        &[
            "function Widget()",
            "this.alpha = 1;",
            "_Widget_secret.set(this, 2);",
            "this.omega = 3;",
        ],
    );
}

#[test]
fn private_public_private_interleave_keeps_source_order() {
    let src = "class Box { #first = 1; middle = 2; #last = 3; }";
    let out = emit(src, es5());
    assert_order(
        &out,
        &[
            "_Box_first.set(this, 1);",
            "this.middle = 2;",
            "_Box_last.set(this, 3);",
        ],
    );
}

#[test]
fn private_brand_then_fields_in_source_order() {
    // Private method forces a WeakSet brand `add(this)`, which `tsc` emits
    // before any field initializer; the public/private fields then interleave.
    let src = "class Service { handle = 1; #token = 2; #run() {} ready = 3; }";
    let out = emit(src, es5());
    assert_order(
        &out,
        &[
            "_Service_instances.add(this);",
            "this.handle = 1;",
            "_Service_token.set(this, 2);",
            "this.ready = 3;",
        ],
    );
}

#[test]
fn parameter_properties_precede_field_initializers() {
    let src = "class Account { balance = 1; #pin = 2; constructor(public owner: string) {} }";
    let out = emit(src, es5());
    assert_order(
        &out,
        &[
            "function Account(owner)",
            "this.owner = owner;",
            "this.balance = 1;",
            "_Account_pin.set(this, 2);",
        ],
    );
}

#[test]
fn auto_accessor_interleaves_with_public_and_private() {
    let src = "class Model { name = 1; accessor count = 2; #hidden = 3; }";
    let out = emit(src, es5());
    assert_order(
        &out,
        &[
            "this.name = 1;",
            "_Model_count_accessor_storage.set(this, 2);",
            "_Model_hidden.set(this, 3);",
        ],
    );
}

#[test]
fn derived_class_interleaves_after_super_capture() {
    let src = "class Base {} class Derived extends Base { head = 1; #mid = 2; tail = 3; constructor() { super(); } }";
    let out = emit(src, es5());
    assert_order(
        &out,
        &[
            "_super.call(this)",
            "_this.head = 1;",
            "_Derived_mid.set(_this, 2);",
            "_this.tail = 3;",
        ],
    );
}

#[test]
fn field_initializers_precede_original_constructor_body() {
    let src = "class Timer { tick = 1; #count = 2; constructor() { this.started = true; } }";
    let out = emit(src, es5());
    assert_order(
        &out,
        &[
            "this.tick = 1;",
            "_Timer_count.set(this, 2);",
            "this.started = true;",
        ],
    );
}

#[test]
fn public_only_fields_unchanged() {
    // Control: classes without private members keep their plain source order.
    let src = "class Plain { one = 1; two = 2; three = 3; }";
    let out = emit(src, es5());
    assert_order(&out, &["this.one = 1;", "this.two = 2;", "this.three = 3;"]);
}
