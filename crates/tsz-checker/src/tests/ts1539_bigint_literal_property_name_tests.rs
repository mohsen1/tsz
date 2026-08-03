//! Regression tests for TS1539 — a literal (non-computed) `bigint` name
//! cannot be used as a property name.
//!
//! Verified against the pinned `typescript@7.0.2` oracle: this fires
//! unconditionally on property-shaped members (interface/type-literal
//! property signatures, class property declarations, object-literal
//! property assignments) regardless of `readonly`, `static`, `declare`, or
//! optionality, and never fires on the method-shaped equivalent (methods,
//! `get`/`set` accessors) in any of those four containers.

use crate::test_utils::check_source_diagnostics;

fn diag_codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn ts1539_interface_property_signature() {
    let codes = diag_codes("interface I { 123n: string; }");
    assert!(codes.contains(&1539), "Got: {codes:?}");
}

#[test]
fn ts1539_interface_optional_property_signature() {
    let codes = diag_codes("interface I { 123n?: string; }");
    assert!(codes.contains(&1539), "Got: {codes:?}");
}

#[test]
fn ts1539_interface_readonly_property_signature() {
    let codes = diag_codes("interface I { readonly 123n: string; }");
    assert!(codes.contains(&1539), "Got: {codes:?}");
}

#[test]
fn ts1539_interface_method_signature_is_not_affected() {
    let codes = diag_codes("interface I { 123n(): void; }");
    assert!(!codes.contains(&1539), "Got: {codes:?}");
}

#[test]
fn ts1539_type_literal_property_signature() {
    let codes = diag_codes("type T = { 123n: string };");
    assert!(codes.contains(&1539), "Got: {codes:?}");
}

#[test]
fn ts1539_type_literal_method_signature_is_not_affected() {
    let codes = diag_codes("type T = { 123n(): void };");
    assert!(!codes.contains(&1539), "Got: {codes:?}");
}

#[test]
fn ts1539_class_instance_property() {
    let codes = diag_codes("class C { 123n = 1; }");
    assert!(codes.contains(&1539), "Got: {codes:?}");
}

#[test]
fn ts1539_class_static_property() {
    let codes = diag_codes("class C { static 123n = 1; }");
    assert!(codes.contains(&1539), "Got: {codes:?}");
}

#[test]
fn ts1539_class_readonly_property() {
    let codes = diag_codes("class C { readonly 123n = 1; }");
    assert!(codes.contains(&1539), "Got: {codes:?}");
}

#[test]
fn ts1539_ambient_class_property() {
    let codes = diag_codes("declare class C { 123n: number; static 123n: number; }");
    let count = codes.iter().filter(|&&c| c == 1539).count();
    assert_eq!(count, 2, "Got: {codes:?}");
}

#[test]
fn ts1539_class_method_is_not_affected() {
    let codes = diag_codes("class C { 123n() {} }");
    assert!(!codes.contains(&1539), "Got: {codes:?}");
}

#[test]
fn ts1539_class_accessor_is_not_affected() {
    let codes = diag_codes("class C { get 123n() { return 1; } set 123n(v: number) {} }");
    assert!(!codes.contains(&1539), "Got: {codes:?}");
}

#[test]
fn ts1539_ambient_class_method_is_not_affected() {
    let codes = diag_codes("declare class C { 123n(): void; }");
    assert!(!codes.contains(&1539), "Got: {codes:?}");
}

#[test]
fn ts1539_object_literal_property_assignment() {
    let codes = diag_codes("const o = { 123n: 1 };");
    assert!(codes.contains(&1539), "Got: {codes:?}");
}

#[test]
fn ts1539_object_literal_method_is_not_affected() {
    let codes = diag_codes("const o = { 123n() {} };");
    assert!(!codes.contains(&1539), "Got: {codes:?}");
}

#[test]
fn ts1539_string_and_numeric_property_names_are_not_affected() {
    let codes = diag_codes(
        r#"
interface I { 123: string; "abc": string; }
class C { 123 = 1; "abc" = 2; }
const o = { 123: 1, "abc": 2 };
"#,
    );
    assert!(!codes.contains(&1539), "Got: {codes:?}");
}

#[test]
fn ts1539_renamed_binders_do_not_change_the_result() {
    let codes = diag_codes(
        r#"
interface AnotherName { 999n: boolean; }
class YetAnotherName { 999n = true; }
"#,
    );
    let count = codes.iter().filter(|&&c| c == 1539).count();
    assert_eq!(count, 2, "Got: {codes:?}");
}
