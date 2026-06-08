//! TS2687 ("All declarations of '{0}' must have identical modifiers.") for
//! duplicate property declarations in **classes**.
//!
//! `tsc` raises TS2687 from `checkVariableLikeDeclaration` whenever two or more
//! class member declarations resolve to the same name but disagree on the
//! optional (`?`) token or the `{readonly, private, protected, abstract, async,
//! static}` modifier mask. The diagnostic is only ever emitted on property
//! declarations (accessors/methods serve as the reference but are never
//! flagged). Targeting follows `tsc`: the first declaration in source order is
//! the reference; every later property whose flags differ is flagged, and the
//! reference property is flagged once if any other property disagrees.
//!
//! All expectations below were verified against `tsc` 6.0.2.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn count_ts2687(source: &str) -> usize {
    check_source(source, "test.ts", CheckerOptions::default())
        .iter()
        .filter(|d| d.code == 2687)
        .count()
}

#[test]
fn class_readonly_vs_mutable_reports_ts2687_on_both() {
    assert_eq!(
        count_ts2687("class C { readonly a: number = 1; a: number = 2; }"),
        2
    );
}

#[test]
fn class_optional_vs_required_reports_ts2687_on_both() {
    assert_eq!(count_ts2687("class C { b?: number; b: number = 2; }"), 2);
}

#[test]
fn class_private_vs_public_reports_ts2687_on_both() {
    assert_eq!(count_ts2687("class C { private a = 1; a = 2; }"), 2);
}

#[test]
fn class_protected_vs_public_reports_ts2687_on_both() {
    assert_eq!(
        count_ts2687("class C { protected a = 1; public a = 2; }"),
        2
    );
}

#[test]
fn class_abstract_vs_concrete_reports_ts2687_on_both() {
    assert_eq!(
        count_ts2687("abstract class C { abstract a: number; a: number = 2; }"),
        2
    );
}

#[test]
fn class_identical_modifiers_report_no_ts2687() {
    assert_eq!(
        count_ts2687("class C { readonly a: number = 1; readonly a: number = 2; }"),
        0
    );
    assert_eq!(count_ts2687("class C { a = 1; a = 2; }"), 0);
}

#[test]
fn class_public_keyword_vs_implicit_reports_no_ts2687() {
    // `public` is the absence of private/protected — same effective flags.
    assert_eq!(count_ts2687("class C { public a = 1; a = 2; }"), 0);
}

#[test]
fn class_override_and_declare_keywords_do_not_trigger_ts2687() {
    // `override`, `declare`, and the `accessor` keyword are outside the compared
    // modifier mask.
    assert_eq!(count_ts2687("class C { override a = 1; a = 2; }"), 0);
    assert_eq!(count_ts2687("class C { accessor a = 1; a = 2; }"), 0);
}

#[test]
fn class_declare_readonly_disagreement_still_reports_ts2687() {
    // `declare` is ignored, but the `readonly` disagreement still fires.
    assert_eq!(
        count_ts2687("class C { declare readonly a: number; declare a: number; }"),
        2
    );
}

#[test]
fn class_three_declarations_flag_reference_and_differing() {
    // [readonly, mutable, readonly]: reference is readonly; only the mutable one
    // differs -> reference + mutable flagged (2 total).
    assert_eq!(
        count_ts2687("class C { readonly a = 1; a = 2; readonly a = 3; }"),
        2
    );
    // [mutable, readonly, readonly]: both later declarations differ from the
    // mutable reference -> all three flagged.
    assert_eq!(
        count_ts2687("class C { a = 1; readonly a = 2; readonly a = 3; }"),
        3
    );
}

#[test]
fn class_static_and_instance_grouped_separately() {
    // Static `readonly a` vs static `a` disagree -> 2687 x2.
    assert_eq!(
        count_ts2687("class C { static readonly a = 1; static a = 2; }"),
        2
    );
    // A static member and an instance member with the same name are distinct
    // symbols -> no disagreement.
    assert_eq!(count_ts2687("class C { static readonly a = 1; a = 2; }"), 0);
}

#[test]
fn class_getter_before_property_flags_only_the_property() {
    // The getter is the reference (value declaration); the readonly property
    // differs from it -> only the property is flagged.
    assert_eq!(
        count_ts2687("class C { get a() { return 1; } readonly a = 2; }"),
        1
    );
}

#[test]
fn class_property_before_getter_reports_no_ts2687() {
    // The property is the reference; the only other declaration is a getter,
    // which is not a property declaration, so the reference probe finds nothing.
    assert_eq!(
        count_ts2687("class C { readonly a = 2; get a() { return 1; } }"),
        0
    );
}

#[test]
fn class_accessor_then_property_mix_matches_reference_model() {
    // [getter, mutable, readonly]: reference is the getter (no readonly); the
    // mutable property matches it, the readonly property differs -> only the
    // readonly property flagged.
    assert_eq!(
        count_ts2687("class C { get a() { return 3; } a = 1; readonly a = 2; }"),
        1
    );
}

#[test]
fn class_method_then_property_flags_only_the_property() {
    assert_eq!(count_ts2687("class C { m(): void {} readonly m = 2; }"), 1);
}

#[test]
fn class_property_then_method_reports_no_ts2687() {
    assert_eq!(count_ts2687("class C { readonly m = 2; m(): void {} }"), 0);
}

#[test]
fn class_method_overloads_report_no_ts2687() {
    assert_eq!(
        count_ts2687("class C { m(x: number): void; m(x: string): void; m(x: any): void {} }"),
        0
    );
}

#[test]
fn class_single_declaration_reports_no_ts2687() {
    assert_eq!(count_ts2687("class C { readonly a: number = 1; }"), 0);
}

#[test]
fn class_modifier_disagreement_follows_structure_not_name() {
    // Behaviour depends on the modifier shape, not the chosen identifier.
    assert_eq!(
        count_ts2687("class Renamed { readonly zebra = 1; zebra = 2; }"),
        2
    );
}

#[test]
fn class_expression_also_reports_ts2687() {
    // The same check runs for class expressions, not only declarations.
    assert_eq!(
        count_ts2687("const C = class { readonly a = 1; a = 2; };"),
        2
    );
}
