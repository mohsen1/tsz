//! TS2720 (`Class 'C' incorrectly implements class 'A'. Did you mean to
//! extend …`) must carry the nested member elaboration that named the nominal
//! break, exactly as `tsc` does. The early-exit branch for a class target with
//! private/protected members historically emitted a bare TS2720 with no
//! elaboration and — worse — fired unconditionally, over-reporting the
//! assignable `class C extends A implements A` case. It now routes through the
//! whole-type relation so the report fires only on a real failure and nests the
//! specific missing / separate-declaration / visibility line.
//!
//! See <https://github.com/tsz-org/tsz/issues/17216>. Ground truth is
//! `typescript@7.0.2 --strict`.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs, load_lib_files};
use tsz_common::diagnostics::Diagnostic;

fn diagnostics(source: &str) -> Vec<Diagnostic> {
    let libs = load_lib_files(&["es5.d.ts", "es2015.d.ts"]);
    check_source_with_libs(source, "test.ts", CheckerOptions::default(), &libs)
}

fn ts2720(source: &str) -> Diagnostic {
    let diags = diagnostics(source);
    diags
        .iter()
        .find(|d| d.code == 2720)
        .cloned()
        .unwrap_or_else(|| panic!("expected a TS2720, got: {diags:#?}"))
}

#[test]
fn missing_private_member_names_the_absent_property() {
    // A private member the implementing class lacks entirely: the nominal brand
    // can never be satisfied, so tsc nests the missing-property line.
    let diag = ts2720(
        r#"
class A {
    private x = 1;
    foo(): number { return 1; }
}
class C implements A {
    foo() { return 1; }
}
"#,
    );
    assert!(
        diag.message_text
            .contains("Property 'x' is missing in type 'C' but required in type 'A'."),
        "TS2720 must nest the missing-member line, got: {:?}",
        diag.message_text
    );
}

#[test]
fn separate_private_declarations_are_named() {
    // Both classes declare their own private `secret`: nominal, so tsc reports
    // "separate declarations of a private property". Binder names deliberately
    // differ from the missing-case test to prove the elaboration is structural.
    let diag = ts2720(
        r#"
class Base {
    private secret = 1;
    ping(): number { return 1; }
}
class Impl implements Base {
    private secret = 2;
    ping(): number { return 1; }
}
"#,
    );
    assert!(
        diag.message_text
            .contains("Types have separate declarations of a private property 'secret'."),
        "TS2720 must nest the separate-declaration line, got: {:?}",
        diag.message_text
    );
}

#[test]
fn separate_protected_declarations_use_the_derived_from_line() {
    // Two protected members with separate declarations: tsc uses the
    // protected-brand "is not a class derived from" line, not the private form.
    let diag = ts2720(
        r#"
class Widget {
    protected slot = 1;
    render(): number { return 1; }
}
class Panel implements Widget {
    protected slot = 2;
    render(): number { return 1; }
}
"#,
    );
    assert!(
        diag.message_text.contains(
            "Property 'slot' is protected but type 'Panel' is not a class derived from 'Widget'."
        ),
        "TS2720 must nest the protected-brand line, got: {:?}",
        diag.message_text
    );
}

#[test]
fn multiple_missing_members_use_the_plural_form() {
    let diag = ts2720(
        r#"
class Store {
    private a = 1;
    private b = 2;
    tick(): number { return 1; }
}
class Cache implements Store {
    tick() { return 1; }
}
"#,
    );
    assert!(
        diag.message_text
            .contains("Type 'Cache' is missing the following properties from type 'Store': a, b"),
        "TS2720 must nest the plural missing-properties line, got: {:?}",
        diag.message_text
    );
}

#[test]
fn public_member_shadowing_private_slot_names_the_visibility_break() {
    // The implementing class has a *public* member of the same name: tsc reports
    // the private-in-target-but-not-in-source visibility line.
    let diag = ts2720(
        r#"
class Owner {
    private token = 1;
    use(): number { return 1; }
}
class Guest implements Owner {
    token = 2;
    use() { return 1; }
}
"#,
    );
    assert!(
        diag.message_text
            .contains("Property 'token' is private in type 'Owner' but not in type 'Guest'."),
        "TS2720 must nest the visibility-break line, got: {:?}",
        diag.message_text
    );
}

#[test]
fn abstract_implementer_still_reports_with_elaboration() {
    // A private member can never be provided even by a subclass, so tsc does not
    // exempt an abstract implementing class here (unlike an ordinary missing
    // member, which abstract may defer).
    let diag = ts2720(
        r#"
class A {
    private x = 1;
    foo(): number { return 1; }
}
abstract class C implements A {
    foo() { return 1; }
}
"#,
    );
    assert!(
        diag.message_text
            .contains("Property 'x' is missing in type 'C' but required in type 'A'."),
        "abstract implementer must still get TS2720 + elaboration, got: {:?}",
        diag.message_text
    );
}

#[test]
fn extends_the_same_base_is_silent() {
    // `class C extends A implements A` inherits A's private brand, so C IS
    // assignable to A — tsc emits nothing. The old blind early-exit wrongly fired
    // a bare TS2720 here.
    let diags = diagnostics(
        r#"
class A {
    private x = 1;
    foo(): number { return 1; }
}
class C extends A implements A {
    foo() { return 1; }
}
"#,
    );
    assert!(
        diags.iter().all(|d| d.code != 2720),
        "extends-the-same-base must not report TS2720, got: {diags:#?}"
    );
}

#[test]
fn missing_member_wins_over_a_separate_private_declaration() {
    // One member is a separate private declaration, another is outright missing.
    // tsc's property walk reports the missing one first.
    let diag = ts2720(
        r#"
class A {
    private x = 1;
    private y = 2;
    foo(): number { return 1; }
}
class C implements A {
    private x = 9;
    foo() { return 1; }
}
"#,
    );
    assert!(
        diag.message_text
            .contains("Property 'y' is missing in type 'C' but required in type 'A'."),
        "missing member must win over separate-declaration, got: {:?}",
        diag.message_text
    );
}
