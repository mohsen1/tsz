//! Regression coverage for TS2413 across interface heritage when an index
//! signature's value type carries an implicitly-`any` (unannotated) property
//! member.
//!
//! An unannotated property signature (`a;`) is implicitly `any` in TypeScript;
//! the `noImplicitAny` TS7008 diagnostic is raised separately from the missing
//! annotation. Lowering such a member to the `error` sentinel poisoned the
//! containing object type, so the `number`-vs-`string` index-signature
//! compatibility check (TS2413) treated the derived interface's inherited
//! string index as "contains an error" and silently dropped it — suppressing an
//! error that `tsc` reports.
//!
//! Oracle (typescript@7.0.2, `--noEmit`): the derived interface reports
//! `TS2413: 'number' index type '{}' is not assignable to 'string' index type
//! '{ a: any; }'.`
//!
//! Binder names are varied across the matrix per the anti-hardcoding gate so
//! the coverage pins the structural rule rather than a single repro's spelling.

use tsz_checker::test_utils::{check_source_non_strict, check_source_strict_codes};

fn non_strict_codes(source: &str) -> Vec<u32> {
    check_source_non_strict(source)
        .iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn inherited_string_and_number_index_conflict_with_implicit_any_member_reports_ts2413() {
    // `Lhs` owns the `string` index whose value has an implicitly-`any` member;
    // `Rhs` owns a `number` index whose value (`{}`) is not assignable to it.
    // `Joined` inherits both and must report TS2413 — the exact
    // `inheritedStringIndexersFromDifferentBaseTypes2` conformance shape.
    let source = r#"
interface Lhs { [s: string]: { a; }; }
interface Rhs { [s: number]: { a; b; }; }
interface Compatible extends Lhs, Rhs {} // ok: { a; b; } is assignable to { a; }
interface Empty { [s: number]: {}; }
interface Joined extends Lhs, Empty {} // TS2413: {} not assignable to { a; }
interface Overridden extends Lhs, Empty { [s: number]: { a; }; } // ok: overrides Empty's number index
"#;
    assert_eq!(non_strict_codes(source), vec![2413]);
}

#[test]
fn implicit_any_member_matches_explicit_any_member_for_ts2413() {
    // The implicitly-`any` member (`p;`) and an explicit `any` member
    // (`p: any`) must behave identically for the index-compatibility check.
    let implicit = r#"
interface Src { [k: string]: { p; }; }
interface Bad { [k: number]: {}; }
interface Derived extends Src, Bad {}
"#;
    let explicit = r#"
interface Src { [k: string]: { p: any }; }
interface Bad { [k: number]: {}; }
interface Derived extends Src, Bad {}
"#;
    assert_eq!(non_strict_codes(implicit), vec![2413]);
    assert_eq!(non_strict_codes(explicit), vec![2413]);
}

#[test]
fn number_index_value_assignable_to_string_index_value_is_clean() {
    // When the `number` index value IS assignable to the `string` index value,
    // no TS2413 fires — even with an implicitly-`any` member in the target.
    let source = r#"
interface Wide { [k: string]: { m; }; }
interface Narrow { [k: number]: { m; n; }; }
interface Ok extends Wide, Narrow {}
"#;
    assert_eq!(non_strict_codes(source), Vec::<u32>::new());
}

#[test]
fn direct_declaration_conflict_still_reports_ts2413() {
    // The own-declaration path (no inheritance) must remain unaffected.
    let source = r#"
interface Direct {
    [k: string]: { field };
    [k: number]: { other };
}
"#;
    assert_eq!(non_strict_codes(source), vec![2413]);
}

#[test]
fn implicit_any_property_type_is_any_not_error() {
    // A value satisfying the implicitly-`any` property must assign cleanly:
    // the property type is `any`, not an error sentinel.
    let source = r#"
interface Holder { value; }
const ok: Holder = { value: 42 };
"#;
    assert_eq!(non_strict_codes(source), Vec::<u32>::new());
}

#[test]
fn implicit_any_property_still_reports_ts7008_under_no_implicit_any() {
    // Fixing the lowered type to `any` must not swallow the `noImplicitAny`
    // diagnostic, which is raised from the missing-annotation node.
    let codes = check_source_strict_codes("interface Holder { value; }");
    assert!(
        codes.contains(&7008),
        "expected TS7008 for the implicit-any member, got: {codes:?}"
    );
}
