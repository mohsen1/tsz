//! TS18036: a legacy (`experimentalDecorators`) class decorator cannot be
//! combined with a static private-identifier member.
//!
//! Structural rule: tsc's `checkClassDeclaration` reports TS18036 on the
//! class's *first* decorator whenever `legacyDecorators` (i.e.
//! `experimentalDecorators`) is on and `some(node.members, p =>
//! hasStaticModifier(p) && isPrivateIdentifierClassElementDeclaration(p))` —
//! a static `PropertyDeclaration`/`MethodDeclaration`/get-or-set accessor
//! named with a `#`-identifier. Only `ClassDeclaration` is checked; tsc's
//! sibling `checkClassExpression` never calls this predicate, and ES (TC39)
//! decorators never gate on it at all.
//!
//! Oracle: `typescript@6.0.2` (`tsc` on `PATH`), `--noEmit --target es2022`,
//! `--experimentalDecorators` where noted.
//!
//! Binder names are varied throughout so no assertion can pass by matching a
//! particular identifier spelling.

use tsz_checker::test_utils::{check_source_codes, check_source_codes_experimental_decorators};

/// A static private field under a decorated class: tsc TS18036 at the
/// decorator.
#[test]
fn ts18036_static_private_field_reports() {
    let codes = check_source_codes_experimental_decorators(
        r#"
@dec
class A {
    static #x = 1;
}
function dec(target: any) { return target; }
"#,
    );
    assert!(
        codes.contains(&18036),
        "expected TS18036 for a decorated class with a static private field, got: {codes:?}"
    );
}

/// Same shape, a static private method instead of a field.
#[test]
fn ts18036_static_private_method_reports() {
    let codes = check_source_codes_experimental_decorators(
        r#"
@dec
class A {
    static #x() { return 1; }
}
function dec(target: any) { return target; }
"#,
    );
    assert!(
        codes.contains(&18036),
        "expected TS18036 for a decorated class with a static private method, got: {codes:?}"
    );
}

/// A static private `get` accessor.
#[test]
fn ts18036_static_private_getter_reports() {
    let codes = check_source_codes_experimental_decorators(
        r#"
@dec
class A {
    static get #x() { return 1; }
}
function dec(target: any) { return target; }
"#,
    );
    assert!(
        codes.contains(&18036),
        "expected TS18036 for a decorated class with a static private getter, got: {codes:?}"
    );
}

/// A static private `set` accessor.
#[test]
fn ts18036_static_private_setter_reports() {
    let codes = check_source_codes_experimental_decorators(
        r#"
@dec
class A {
    static set #x(v: number) {}
}
function dec(target: any) { return target; }
"#,
    );
    assert!(
        codes.contains(&18036),
        "expected TS18036 for a decorated class with a static private setter, got: {codes:?}"
    );
}

/// Renamed binders (class/decorator/member identifiers) — the fix must rest
/// on the structural shape (static + private-identifier name), not spelling.
#[test]
fn ts18036_renamed_binders_reports() {
    let codes = check_source_codes_experimental_decorators(
        r#"
@myDecorator
class Widget {
    static #secretCache = 1;
}
function myDecorator(target: any) { return target; }
"#,
    );
    assert!(
        codes.contains(&18036),
        "expected TS18036 to be spelling-independent, got: {codes:?}"
    );
}

/// Two decorators: tsc anchors TS18036 on the *first* one in source order
/// and reports exactly once, not once per decorator.
#[test]
fn ts18036_multiple_decorators_reports_once() {
    let codes = check_source_codes_experimental_decorators(
        r#"
@dec2
@dec
class A {
    static #x = 1;
}
function dec(target: any) { return target; }
function dec2(target: any) { return target; }
"#,
    );
    let count = codes.iter().filter(|&&c| c == 18036).count();
    assert_eq!(
        count, 1,
        "expected exactly one TS18036 for a doubly-decorated class, got: {codes:?}"
    );
}

/// Negative: an *instance* private field (no `static`) never triggers
/// TS18036, decorated or not.
#[test]
fn ts18036_instance_private_field_clean() {
    let codes = check_source_codes_experimental_decorators(
        r#"
@dec
class A {
    #x = 1;
}
function dec(target: any) { return target; }
"#,
    );
    assert!(
        !codes.contains(&18036),
        "instance private fields must not trigger TS18036, got: {codes:?}"
    );
}

/// Negative: a static *public* field never triggers TS18036.
#[test]
fn ts18036_static_public_field_clean() {
    let codes = check_source_codes_experimental_decorators(
        r#"
@dec
class A {
    static x = 1;
}
function dec(target: any) { return target; }
"#,
    );
    assert!(
        !codes.contains(&18036),
        "static public fields must not trigger TS18036, got: {codes:?}"
    );
}

/// Negative: a static private field with no class decorator at all.
#[test]
fn ts18036_no_decorator_clean() {
    let codes = check_source_codes_experimental_decorators(
        r#"
class A {
    static #x = 1;
}
"#,
    );
    assert!(
        !codes.contains(&18036),
        "an undecorated class must not trigger TS18036, got: {codes:?}"
    );
}

/// Negative: ES (TC39 stage-3) decorators never gate on this predicate —
/// `legacyDecorators` must be true, so plain `check_source_codes` (no
/// `experimentalDecorators`) on the same shape must stay clean of TS18036.
#[test]
fn ts18036_es_decorator_clean() {
    let codes = check_source_codes(
        r#"
function dec(target: any, ctx: ClassDecoratorContext) { return target; }
@dec
class A {
    static #x = 1;
}
"#,
    );
    assert!(
        !codes.contains(&18036),
        "ES decorators (non-legacy) must not trigger TS18036, got: {codes:?}"
    );
}
