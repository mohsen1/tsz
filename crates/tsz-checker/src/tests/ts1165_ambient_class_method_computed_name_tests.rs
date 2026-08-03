//! Regression tests for TS1165 — a computed method name in an ambient class
//! must refer to an expression whose type is a literal type or a 'unique
//! symbol' type.
//!
//! TS1165 is the method-signature sibling of TS1166 (class property
//! declarations), TS1169 (interfaces), and TS1170 (type literals): all four
//! route through the shared `check_computed_property_requires_literal`
//! helper, but TS1165 previously had no call site anywhere in the checker.
//! Verified against the pinned `typescript@7.0.2` oracle: unlike TS1166,
//! this arm only fires inside an ambient context (never for a non-ambient
//! class method) and only for method declarations (never for accessors).

use crate::test_utils::check_source_diagnostics;

fn diag_codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn ts1165_declare_class_method() {
    let codes = diag_codes(
        r#"
declare const x: string;
declare class C {
    [`a${x}`](): void;
}
"#,
    );
    assert!(
        codes.contains(&1165),
        "Expected TS1165 for non-literal computed method name in a declare class. Got: {codes:?}"
    );
}

#[test]
fn ts1165_declare_abstract_class_method() {
    let codes = diag_codes(
        r#"
declare const x: string;
declare abstract class C {
    [`a${x}`](): void;
}
"#,
    );
    assert!(
        codes.contains(&1165),
        "Expected TS1165 for non-literal computed method name in a declare abstract class. Got: {codes:?}"
    );
}

#[test]
fn ts1165_declare_class_static_method() {
    let codes = diag_codes(
        r#"
declare const x: string;
declare class C {
    static [`a${x}`](): void;
}
"#,
    );
    assert!(
        codes.contains(&1165),
        "Expected TS1165 for non-literal computed static method name in a declare class. Got: {codes:?}"
    );
}

#[test]
fn ts1165_class_nested_in_declare_namespace() {
    let codes = diag_codes(
        r#"
declare const x: string;
declare namespace N {
    class C {
        [`a${x}`](): void;
    }
}
"#,
    );
    assert!(
        codes.contains(&1165),
        "Expected TS1165 for a class method nested inside a declare namespace. Got: {codes:?}"
    );
}

/// Anti-hardcoding cover: renamed binder and container names must not change
/// whether the check fires.
#[test]
fn ts1165_renamed_binder_and_container() {
    let codes = diag_codes(
        r#"
declare const suffix: string;
declare class Widget {
    [`prefix_${suffix}`](): void;
}
"#,
    );
    assert!(
        codes.contains(&1165),
        "TS1165 must fire regardless of identifier spelling. Got: {codes:?}"
    );
}

/// Negative control: the identical shape in a non-ambient (concrete) class
/// must NOT trigger TS1165 — the check is ambient-only, unlike TS1166.
#[test]
fn ts1165_absent_for_non_ambient_class_method() {
    let codes = diag_codes(
        r#"
declare const x: string;
class C {
    [`a${x}`](): void {}
}
"#,
    );
    assert!(
        !codes.contains(&1165),
        "TS1165 must not fire for a non-ambient class method. Got: {codes:?}"
    );
}

/// Negative control: accessors are exempt from this arm entirely, even
/// inside an ambient class (verified against the tsc@7.0.2 oracle).
#[test]
fn ts1165_absent_for_ambient_class_accessor() {
    let codes = diag_codes(
        r#"
declare const x: string;
declare class C {
    get [`a${x}`](): string;
}
"#,
    );
    assert!(
        !codes.contains(&1165),
        "TS1165 must not fire for an ambient class accessor. Got: {codes:?}"
    );
}

/// Negative control: a `unique symbol`-typed operand is a valid ambient
/// computed name and must not trigger TS1165.
#[test]
fn ts1165_absent_for_unique_symbol_operand() {
    let codes = diag_codes(
        r#"
declare const sym: unique symbol;
declare class C {
    [sym](): void;
}
"#,
    );
    assert!(
        !codes.contains(&1165),
        "TS1165 must not fire for a unique-symbol-typed computed method name. Got: {codes:?}"
    );
}

/// Fallback/adjacent case: the property-member arm in the same ambient class
/// must keep reporting TS1166, not TS1165 — the two arms are distinct and
/// this fix must not blur them.
#[test]
fn ts1166_still_used_for_ambient_class_property() {
    let codes = diag_codes(
        r#"
declare const x: string;
declare class C {
    [`a${x}`]: number;
}
"#,
    );
    assert!(
        codes.contains(&1166),
        "Expected TS1166 for the property arm in a declare class. Got: {codes:?}"
    );
    assert!(
        !codes.contains(&1165),
        "TS1165 must not fire for a property declaration. Got: {codes:?}"
    );
}
