//! Regression tests for TS1539 — a literal (non-computed) `bigint` name
//! cannot be used as a property name.
//!
//! Verified against the pinned `typescript@7.0.2` oracle: this fires on
//! property-shaped members (interface/type-literal property signatures,
//! class property declarations, object-literal property assignments)
//! regardless of `readonly`, `static`, `declare`, or optionality, and never
//! fires on the method-shaped equivalent (methods, `get`/`set` accessors) in
//! any of those four containers. It is suppressed program-wide whenever any
//! file in the compilation has a real syntax error, matching `tsc` (verified
//! directly against the oracle: a genuine parse error in one file suppresses
//! semantic diagnostics in a completely unrelated file of the same
//! compilation, not just its own file).

use crate::test_utils::{check_source_codes_with_parse_health, check_source_diagnostics};

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

// TS1539 must be suppressed program-wide whenever the file has a real syntax
// error, matching every sibling property-name-shape check in this checker
// (TS2464's computed-property-name validation, TS1170's type-literal
// computed-property check). Verified directly against the pinned oracle: a
// genuine parse error in one file suppresses semantic diagnostics in an
// entirely unrelated file of the same compilation. These use
// `check_source_codes_with_parse_health` (real parser-diagnostic wiring)
// rather than `check_source_diagnostics`, which never sets `has_parse_errors`
// and so could never observe this suppression either way.

#[test]
fn ts1539_suppressed_by_a_real_syntax_error_in_the_same_file_interface() {
    let codes = check_source_codes_with_parse_health(
        r#"
interface I { 123n: string; }
const broken = ;
"#,
    );
    assert!(!codes.contains(&1539), "Got: {codes:?}");
}

#[test]
fn ts1539_suppressed_by_a_real_syntax_error_in_the_same_file_class() {
    let codes = check_source_codes_with_parse_health(
        r#"
class C { 123n = 1; }
const broken = ;
"#,
    );
    assert!(!codes.contains(&1539), "Got: {codes:?}");
}

#[test]
fn ts1539_suppressed_by_a_real_syntax_error_in_the_same_file_object_literal() {
    let codes = check_source_codes_with_parse_health(
        r#"
const o = { 123n: 1 };
const broken = ;
"#,
    );
    assert!(!codes.contains(&1539), "Got: {codes:?}");
}

#[test]
fn ts1539_still_fires_via_the_parse_health_harness_without_a_syntax_error() {
    // Negative control for the three tests above: confirms the harness
    // itself does not suppress TS1539 unconditionally — only a real parse
    // error does.
    let codes = check_source_codes_with_parse_health("interface I { 123n: string; }");
    assert!(codes.contains(&1539), "Got: {codes:?}");
}
