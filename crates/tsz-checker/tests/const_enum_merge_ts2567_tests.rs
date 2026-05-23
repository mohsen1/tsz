//! TS2567 "Enum declarations can only merge with namespace or other enum
//! declarations" for `const enum` / non-const `enum` merges of the same name.
//!
//! Structural rule: when a symbol's enum declarations disagree on const-ness,
//! tsc keeps the *first* declaration's const-ness as the merged symbol's kind
//! and splits every later declaration that disagrees into its own symbol,
//! reporting TS2567 at the enum name of both the surviving (primary)
//! declarations bound before the conflict and the conflicting declaration.
//! Only the primary group actually merges, so TS2432 ("only one declaration
//! can omit an initializer") and member-duplicate TS2300 apply to it alone.

use tsz_checker::test_utils::check_source_codes as get_error_codes;

fn count(codes: &[u32], code: u32) -> usize {
    codes.iter().filter(|&&c| c == code).count()
}

#[test]
fn enum_then_const_enum_with_initializers_reports_two_ts2567() {
    let codes = get_error_codes(
        r#"
enum E { A = 1 }
const enum E { B = 2 }
"#,
    );
    assert_eq!(
        count(&codes, 2567),
        2,
        "const/non-const enum merge must report TS2567 on both declarations, got: {codes:?}"
    );
}

#[test]
fn const_enum_then_enum_order_swapped_reports_two_ts2567() {
    // Position of the const declaration does not matter.
    let codes = get_error_codes(
        r#"
const enum Color { A = 1 }
enum Color { B = 2 }
"#,
    );
    assert_eq!(
        count(&codes, 2567),
        2,
        "swapped-order const/non-const merge must report TS2567 twice, got: {codes:?}"
    );
}

#[test]
fn enum_then_const_enum_without_initializers_reports_ts2567_not_ts2432() {
    // The split-off declarations each form a single-declaration symbol, so the
    // "only one declaration can omit an initializer" rule (TS2432) does not
    // fire — tsc emits only the two TS2567.
    let codes = get_error_codes(
        r#"
enum Status { A }
const enum Status { B }
"#,
    );
    assert_eq!(
        count(&codes, 2567),
        2,
        "uninitialized const/non-const merge must report TS2567 twice, got: {codes:?}"
    );
    assert_eq!(
        count(&codes, 2432),
        0,
        "split-off enum declarations must not trigger TS2432, got: {codes:?}"
    );
}

#[test]
fn two_non_const_enums_plus_one_const_reports_ts2567_for_all_three() {
    // Primary group = the two leading non-const enums; the trailing const enum
    // conflicts. TS2567 lands on all three names; TS2432 fires once within the
    // (uninitialized) primary group.
    let codes = get_error_codes(
        r#"
enum E { X }
enum E { Y }
const enum E { Z }
"#,
    );
    assert_eq!(
        count(&codes, 2567),
        3,
        "all enum declarations in a const-mismatch merge get TS2567, got: {codes:?}"
    );
    assert_eq!(
        count(&codes, 2432),
        1,
        "TS2432 applies once within the primary merge group, got: {codes:?}"
    );
}

#[test]
fn const_then_non_const_then_const_merges_primary_group() {
    // First declaration is const, so the primary group is the two const enums;
    // the middle non-const enum is the only conflict. tsc: TS2567 on decls 1
    // and 2 only, and TS2432 within the const primary group (decls 1 and 3).
    let codes = get_error_codes(
        r#"
const enum E { A }
enum E { B }
const enum E { C }
"#,
    );
    assert_eq!(
        count(&codes, 2567),
        2,
        "only the first-const primary plus the conflicting non-const enum get TS2567, got: {codes:?}"
    );
    assert_eq!(
        count(&codes, 2432),
        1,
        "TS2432 applies to the const primary group, got: {codes:?}"
    );
}

#[test]
fn two_non_const_enums_merge_cleanly_no_ts2567() {
    // Control: same const-ness merges, only the ordinary TS2432 applies.
    let codes = get_error_codes(
        r#"
enum E { X }
enum E { Y }
"#,
    );
    assert_eq!(
        count(&codes, 2567),
        0,
        "same-const-ness enum merge must not report TS2567, got: {codes:?}"
    );
    assert_eq!(
        count(&codes, 2432),
        1,
        "uninitialized non-const merge keeps its TS2432, got: {codes:?}"
    );
}

#[test]
fn two_const_enums_merge_cleanly_no_ts2567() {
    // Control: both const, both initialized — a fully valid merge.
    let codes = get_error_codes(
        r#"
const enum E { A = 1 }
const enum E { B = 2 }
"#,
    );
    assert_eq!(
        count(&codes, 2567),
        0,
        "two const enums merge cleanly, got: {codes:?}"
    );
    assert_eq!(
        count(&codes, 2432),
        0,
        "initialized const merge has no TS2432, got: {codes:?}"
    );
}

#[test]
fn non_const_enum_merges_with_namespace_no_ts2567() {
    // Control: a non-const enum may merge with a namespace.
    let codes = get_error_codes(
        r#"
enum E { A = 1 }
namespace E { export const x = 1; }
"#,
    );
    assert_eq!(
        count(&codes, 2567),
        0,
        "non-const enum + namespace is a valid merge, got: {codes:?}"
    );
}
