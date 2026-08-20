//! Regression tests for #16371: `declare` on a private/private-identifier
//! class *member* (as opposed to the enclosing class or file being ambient)
//! suppresses the `noImplicitAny` member family (TS7006/TS7008/TS7010) for
//! methods and properties, but not for accessors.
//!
//! `declare` is not a legal modifier on a *method* inside a non-ambient
//! class — `tsc` reports TS1031 for it, which tsz does not yet implement
//! (a separate, pre-existing gap; these tests assert only the
//! `noImplicitAny` family, not TS1031) — but is legal on a *property* (an
//! ambient property declaration). Either way, `tsc` treats the member as
//! hidden from the ambient declaration surface for the `noImplicitAny`
//! family when it is also `private` or named with a private identifier.
//! Neither condition suppresses alone: `declare m()` (no private-ness) keeps
//! TS7010, and `private m()` without `declare` keeps TS7010 too. A
//! private-*identifier* property (`#x`) additionally routes through the
//! dedicated TS18019 grammar check rather than TS7008.
//!
//! Oracle-verified against pinned `typescript@7.0.2`
//! (`--noEmit --strict --target es2022 --module esnext --pretty false`).

use crate::context::CheckerOptions;
use crate::test_utils::check_source;

fn codes(source: &str) -> Vec<u32> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    )
    .iter()
    .map(|d| d.code)
    .collect()
}

// -- Methods (TS7010) -------------------------------------------------------

#[test]
fn declare_private_identifier_method_suppresses_ts7010() {
    assert_eq!(codes("class A { declare #m(); }"), Vec::<u32>::new());
}

#[test]
fn declare_private_modifier_method_suppresses_ts7010() {
    assert_eq!(codes("class A { declare private m(); }"), Vec::<u32>::new());
}

#[test]
fn declare_static_private_identifier_method_suppresses_ts7010() {
    assert_eq!(codes("class A { declare static #m(); }"), Vec::<u32>::new());
}

#[test]
fn declare_ordinary_name_method_keeps_ts7010() {
    assert_eq!(codes("class A { declare m(); }"), vec![7010]);
}

#[test]
fn declare_protected_method_keeps_ts7010() {
    assert_eq!(codes("class A { declare protected m(); }"), vec![7010]);
}

#[test]
fn private_identifier_method_without_declare_stays_clean() {
    // Negative control: no `declare` at all keeps behaving as before this fix
    // (a private-identifier method body infers its return type, so no TS7010
    // fires anyway; this pins that this fix does not touch that path).
    assert_eq!(codes("class A { #m() { return 1; } }"), Vec::<u32>::new());
}

// -- Method parameters (TS7006) ---------------------------------------------
//
// The same suppression must reach parameter checking, which — for methods —
// is computed twice: once in `check_method_declaration_with_request`
// (`ambient_signature_checks.rs`) and independently in `get_type_of_function`
// (`function_type.rs`), which has its own `is_ambient_private` predicate.
// Both needed the private-identifier case added; only the `private` modifier
// was previously wired in `function_type.rs`.

#[test]
fn declare_private_identifier_method_suppresses_ts7006_param() {
    assert_eq!(codes("class A { declare #m(x); }"), Vec::<u32>::new());
}

#[test]
fn declare_private_modifier_method_suppresses_ts7006_param() {
    assert_eq!(
        codes("class A { declare private m(x); }"),
        Vec::<u32>::new()
    );
}

#[test]
fn declare_class_private_identifier_method_param_stays_clean() {
    // Adjacent case caught by the same `function_type.rs` gap: a *whole*
    // ambient class with a private-identifier method parameter was also
    // wrongly reporting TS7006 before this fix (the `is_ambient_private`
    // predicate never matched a private-identifier name, only `private`).
    assert_eq!(codes("declare class A { #m(x); }"), Vec::<u32>::new());
}

// -- Properties (TS7008) -----------------------------------------------------

#[test]
fn declare_private_identifier_property_suppresses_ts7008() {
    // `tsc` reports TS18019 ('declare' cannot be used with a private
    // identifier) for the name itself — a property, unlike a method, is a
    // legal position for `declare`. TS7008 must not join it.
    assert_eq!(codes("class A { declare #x; }"), vec![18019]);
}

#[test]
fn declare_private_modifier_property_suppresses_ts7008() {
    assert_eq!(codes("class A { declare private x; }"), Vec::<u32>::new());
}

#[test]
fn declare_static_private_identifier_property_suppresses_ts7008() {
    assert_eq!(codes("class A { declare static #x; }"), vec![18019]);
}

#[test]
fn declare_ordinary_name_property_keeps_ts7008() {
    assert_eq!(codes("class A { declare x; }"), vec![7008]);
}

// -- Accessors: this suppression must NOT apply (TS7032/TS7033) -------------
//
// A member's own `declare` does not hide an accessor from the ambient
// surface the way it does a method or property — only the enclosing class
// or file being genuinely ambient does. `function_type.rs`'s
// `is_ambient_private` deliberately keeps its narrower `private`-modifier-only
// check for accessors so these stay unsuppressed.

#[test]
fn declare_private_identifier_getter_keeps_ts7033() {
    assert_eq!(codes("class A { declare get #m(); }"), vec![7033]);
}

#[test]
fn declare_private_modifier_getter_keeps_ts7033() {
    assert_eq!(codes("class A { declare private get m(); }"), vec![7033]);
}

#[test]
fn declare_private_identifier_setter_keeps_ts7032_and_ts7006() {
    assert_eq!(codes("class A { declare set #m(v); }"), vec![7006, 7032]);
}

#[test]
fn declare_private_modifier_setter_keeps_ts7032_and_ts7006() {
    assert_eq!(
        codes("class A { declare private set m(v); }"),
        vec![7006, 7032]
    );
}

// -- Whole class already-ambient: unaffected by this change -----------------

#[test]
fn declare_class_private_identifier_method_stays_clean() {
    assert_eq!(codes("declare class A { #m(); }"), Vec::<u32>::new());
}

#[test]
fn declare_class_private_identifier_property_stays_clean() {
    assert_eq!(codes("declare class A { #x; }"), Vec::<u32>::new());
}

#[test]
fn declare_class_private_identifier_setter_stays_clean() {
    // Whole-class ambient accessors DO suppress (unlike the member-own-declare
    // case above): a genuinely ambient class hides every private member,
    // accessors included. Routed through
    // `member_hidden_from_ambient_declaration_surface` in
    // `accessor_checker.rs`, not the `function_type.rs` predicate this PR
    // touches.
    assert_eq!(codes("declare class A { set #m(v); }"), Vec::<u32>::new());
}
