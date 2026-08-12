//! TS2687 ("All declarations of '{0}' must have identical modifiers.") for
//! duplicate property declarations in object type literals and interfaces.
//!
//! `tsc` raises TS2687 whenever two or more property declarations resolve to
//! the same member name but disagree on the `readonly` or optional (`?`)
//! modifier. The diagnostic is independent of the same-type (TS2717) check:
//! it fires even when the declared types match (so TS2717 is absent).
//! Targeting follows `tsc`: the first declaration is the reference; every
//! later declaration whose flags differ from it is flagged, and the
//! reference itself is flagged once if any later declaration differs.
//!
//! TS2687 is NOT independent of TS2300: computed names that resolve to the
//! same value still report TS2300 (duplicate identifier) alongside TS2687,
//! oracle-confirmed against `typescript@7.0.2` (re-verified for #17203).

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn count_ts2687(source: &str) -> usize {
    check_source(source, "test.ts", CheckerOptions::default())
        .iter()
        .filter(|d| d.code == 2687)
        .count()
}

#[test]
fn type_literal_readonly_vs_mutable_reports_ts2687_on_both() {
    // readonly disagrees -> flag both declarations.
    assert_eq!(
        count_ts2687("type X = { readonly a: number; a: number };"),
        2
    );
}

#[test]
fn type_literal_optional_vs_required_reports_ts2687_on_both() {
    assert_eq!(count_ts2687("type X = { a?: number; a: number };"), 2);
}

#[test]
fn type_literal_identical_readonly_reports_no_ts2687() {
    assert_eq!(
        count_ts2687("type X = { readonly a: number; readonly a: number };"),
        0
    );
}

#[test]
fn type_literal_identical_mutable_reports_no_ts2687() {
    assert_eq!(count_ts2687("type X = { a: number; a: number };"), 0);
}

#[test]
fn type_literal_three_declarations_flag_reference_and_differing() {
    // [readonly, mutable, readonly]: reference is readonly, only the mutable
    // one differs -> reference + the mutable declaration are flagged (2 total).
    assert_eq!(
        count_ts2687("type X = { readonly a: number; a: number; readonly a: number };"),
        2
    );
    // [mutable, readonly, readonly]: both later declarations differ from the
    // mutable reference -> all three are flagged.
    assert_eq!(
        count_ts2687("type X = { a: number; readonly a: number; readonly a: number };"),
        3
    );
}

#[test]
fn type_literal_readonly_optional_combination_reports_ts2687() {
    // readonly matches but optional differs -> still a modifier disagreement.
    assert_eq!(
        count_ts2687("type X = { readonly a?: number; readonly a: number };"),
        2
    );
}

#[test]
fn type_literal_method_overloads_report_no_ts2687() {
    // Method signatures with the same name are overloads, not duplicates.
    assert_eq!(count_ts2687("type X = { a(): void; a(): void };"), 0);
}

#[test]
fn type_literal_single_declaration_reports_no_ts2687() {
    assert_eq!(count_ts2687("type X = { readonly a: number };"), 0);
}

#[test]
fn interface_readonly_vs_mutable_reports_ts2687_on_both() {
    assert_eq!(
        count_ts2687("interface I { readonly d: string; d: string; }"),
        2
    );
}

#[test]
fn interface_optional_vs_required_reports_ts2687_on_both() {
    assert_eq!(count_ts2687("interface I { d?: string; d: string; }"), 2);
}

#[test]
fn interface_identical_modifiers_report_no_ts2687() {
    assert_eq!(
        count_ts2687("interface I { readonly d: string; readonly d: string; }"),
        0
    );
}

#[test]
fn modifier_disagreement_follows_structure_not_name() {
    // Behaviour must depend on the modifier shape, not the chosen identifier.
    assert_eq!(
        count_ts2687("type Renamed = { readonly zebra: number; zebra: number };"),
        2
    );
}

#[test]
fn computed_names_resolving_to_same_value_report_ts2687_and_ts2300() {
    // `[c0]` and `[c1]` both resolve to "a"; oracle-confirmed (typescript@7.0.2,
    // re-verified for #17203) tsc reports BOTH TS2300 (duplicate identifier)
    // and TS2687 (readonly disagreement) for this shape — it does not suppress
    // TS2300 for computed names resolving to the same value. This test
    // previously pinned zero TS2300s, which was a stale expectation, not a
    // regression: tsz's TS2300 x2 / TS2687 x2 output matches tsc exactly.
    let source = "const c0 = \"a\";\nconst c1 = \"a\";\n\
        type X = { readonly [c0]: number; [c1]: number };";
    let diagnostics = check_source(source, "test.ts", CheckerOptions::default());
    assert_eq!(diagnostics.iter().filter(|d| d.code == 2687).count(), 2);
    assert_eq!(diagnostics.iter().filter(|d| d.code == 2300).count(), 2);
}
