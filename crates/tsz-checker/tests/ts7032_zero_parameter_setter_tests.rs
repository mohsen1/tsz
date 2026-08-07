//! `TS7032` for a `set` accessor with **no parameter at all** — the
//! zero-parameter half of the already-wired "setter lacks a parameter type
//! annotation" family. Issue #16809.
//!
//! `tsc` reports `TS7032` here alongside the (unrelated) grammar error
//! `TS1049` ("An accessor property setter must have exactly one parameter."):
//! a zero-parameter setter still "lacks a parameter type annotation" under
//! `noImplicitAny`, the same way a present-but-unannotated one does.
//!
//! `check_source_strict` (used below, matching the sibling test files in this
//! family) only surfaces checker diagnostics, so `TS1049` — a parser grammar
//! diagnostic — never appears in these expectations; it is unaffected by this
//! change and out of scope here.
//!
//! Three checker-owned emission sites compute this decision inside a `for`
//! loop over the setter's parameters, so a zero-parameter setter's loop body
//! never ran and `TS7032` stayed silent:
//! - `checkers/accessor_checker.rs::check_setter_parameter` (class member,
//!   ambient class member).
//! - `checkers/accessor_checker.rs::check_type_member_accessor_implicit_any`
//!   (interface member, type-literal member).
//! - `types/computation/object_literal/accessor_element.rs` (object literal
//!   accessor element).
//!
//! Every expectation was recorded from `typescript@7.0.2` under `--noEmit
//! --strict --lib es2022 --target es2022`.

use tsz_checker::test_utils::check_source_strict;

fn sites(source: &str) -> Vec<String> {
    let mut out: Vec<String> = check_source_strict(source)
        .iter()
        .map(|d| format!("TS{}@{}", d.code, d.start))
        .collect();
    out.sort();
    out
}

fn assert_sites(source: &str, expected: &[&str]) {
    let actual = sites(source);
    let expected: Vec<String> = expected.iter().map(|s| (*s).to_string()).collect();
    assert_eq!(actual, expected, "source: {source}");
}

// ---------------------------------------------------------------------------
// The six divergent rows from #16809: every container a `set` accessor can
// be written in.
// ---------------------------------------------------------------------------

#[test]
fn class_zero_parameter_setter_reports_ts7032() {
    assert_sites("class C { set y() {} }", &["TS7032@14"]);
}

#[test]
fn object_literal_zero_parameter_setter_reports_ts7032() {
    assert_sites("const o = { set z() {} };", &["TS7032@16"]);
}

#[test]
fn ambient_class_zero_parameter_setter_reports_ts7032() {
    assert_sites("declare class C { set w(); }", &["TS7032@22"]);
}

#[test]
fn ambient_class_unpaired_get_and_zero_parameter_set_reports_ts7032() {
    // Neither accessor supplies the property a type, so the pair still
    // blames the setter — same rule as the present-but-unannotated pair,
    // just with no parameter to blame TS7006 on.
    assert_sites("declare class C { get p(); set p(); }", &["TS7032@31"]);
}

#[test]
fn interface_zero_parameter_setter_reports_ts7032() {
    assert_sites("interface I { set a(); }", &["TS7032@18"]);
}

#[test]
fn type_literal_zero_parameter_setter_reports_ts7032() {
    assert_sites("type T = { set c(); };", &["TS7032@15"]);
}

// ---------------------------------------------------------------------------
// Controls from #16809: shapes that must stay exactly as they already were.
// ---------------------------------------------------------------------------

#[test]
fn class_paired_getter_with_annotation_suppresses_zero_parameter_setter() {
    assert_sites("class C { get q(): number {return 1;} set q() {} }", &[]);
}

#[test]
fn class_paired_getter_with_inferred_body_suppresses_zero_parameter_setter() {
    assert_sites("class C { get r() {return 1;} set r() {} }", &[]);
}

#[test]
fn interface_paired_annotated_getter_suppresses_zero_parameter_setter() {
    assert_sites("interface I { get b(): number; set b(); }", &[]);
}

#[test]
fn class_present_but_unannotated_setter_is_unaffected() {
    // The already-wired half of the family: present-but-unannotated still
    // reports both TS7006 (on the parameter) and TS7032 (on the setter name).
    assert_sites("class C { set d(v) {} }", &["TS7006@16", "TS7032@14"]);
}

#[test]
fn class_annotated_zero_arity_free_setter_is_clean() {
    assert_sites("class C { set e(v: number) {} }", &[]);
}

// ---------------------------------------------------------------------------
// Adjacent cases: renamed binders, and TS7006 must not fire when there is no
// parameter to blame it on.
// ---------------------------------------------------------------------------

#[test]
fn renamed_binders_report_identically() {
    assert_sites("class Zqx { set wobble() {} }", &["TS7032@16"]);
    assert_sites("interface Zqx { set wobble(); }", &["TS7032@20"]);
    assert_sites("type Zqx = { set wobble(); };", &["TS7032@17"]);
}

#[test]
fn zero_parameter_setter_never_reports_ts7006() {
    // There is no parameter node to blame TS7006 on — only TS7032 fires.
    for source in [
        "class C { set y() {} }",
        "const o = { set z() {} };",
        "declare class C { set w(); }",
        "interface I { set a(); }",
        "type T = { set c(); };",
    ] {
        let codes: Vec<u32> = check_source_strict(source).iter().map(|d| d.code).collect();
        assert!(
            !codes.contains(&7006),
            "TS7006 must not fire on a zero-parameter setter: {source}"
        );
    }
}

#[test]
fn the_family_is_governed_by_no_implicit_any() {
    use tsz_checker::test_utils::check_source_non_strict_codes;
    assert!(
        check_source_non_strict_codes("class C { set y() {} }").is_empty(),
        "TS7032 must be governed by noImplicitAny"
    );
    assert!(
        check_source_non_strict_codes("interface I { set a(); }").is_empty(),
        "TS7032 must be governed by noImplicitAny"
    );
    assert!(
        check_source_non_strict_codes("const o = { set z() {} };").is_empty(),
        "TS7032 must be governed by noImplicitAny"
    );
}
