//! Regression tests for diagnostic display of unions whose top-level alias
//! names would be lost during interner flattening.
//!
//! When a user writes `T | null` and `T` is itself a union alias (e.g.,
//! `type T = "a" | "b" | undefined`), the interner flattens the result into
//! `"a" | "b" | undefined | null` for type-system correctness. tsc preserves
//! the as-written `T | null` form for diagnostic display via `UnionType.origin`.
//!
//! tsz captures the same information through the `display_union_origin` side
//! table on `TypeInterner`, populated by `get_type_from_union_type` and
//! consulted by the diagnostic printer. These tests lock the contract via the
//! highest-fidelity public surface available: full source → diagnostic text.

use tsz_checker::test_utils::check_source_diagnostics;

/// TS2859 ("Excessive complexity comparing types") must reference the
/// as-written alias name (`T1 | null`) rather than the flattened union body.
///
/// The repro is taken from `relationComplexityError.ts` (TS issue #55630).
#[test]
fn ts2859_message_preserves_top_level_alias_in_target_union() {
    let source = r#"
type Digits = '0' | '1' | '2' | '3' | '4' | '5' | '6' | '7';
type T1 = `${Digits}${Digits}${Digits}${Digits}` | undefined;
type T2 = { a: string } | { b: number };

function f2(x: T1 | null, y: T1 & T2) {
    x = y;
}
"#;
    let diags = check_source_diagnostics(source);
    let ts2859 = diags
        .iter()
        .find(|d| d.code == 2859)
        .unwrap_or_else(|| panic!("Expected TS2859, got: {diags:?}"));

    assert!(
        ts2859.message_text.contains("'T1 & T2'"),
        "Source half of TS2859 must read 'T1 & T2'. Got: {}",
        ts2859.message_text
    );
    assert!(
        ts2859.message_text.contains("'T1 | null'"),
        "Target half of TS2859 must read 'T1 | null' (the as-written alias \
         form), not the flattened union body. Got: {}",
        ts2859.message_text
    );
    assert!(
        !ts2859.message_text.contains("Digits"),
        "Target half must not leak T1's expanded body. Got: {}",
        ts2859.message_text
    );
}

/// Source-written string literal unions do not always display in declaration
/// order. This locks a tsc-compatible counterexample so union origin
/// preservation does not over-apply to all string literal unions.
#[test]
fn ts2322_renamed_string_literal_union_uses_tsc_display_order() {
    let source = r#"
type Status = "active" | "inactive" | "pending";
declare const s: "draft" | "active" | "inactive";
const x: Status = s;
"#;
    let diags = check_source_diagnostics(source);
    let ts2322 = diags
        .iter()
        .find(|d| d.code == 2322)
        .unwrap_or_else(|| panic!("Expected TS2322, got: {diags:?}"));
    assert!(
        ts2322
            .message_text
            .contains(r#""active" | "inactive" | "draft""#),
        "Source type must display in tsc order. Got: {}",
        ts2322.message_text
    );
    assert!(
        !ts2322
            .message_text
            .contains(r#""draft" | "active" | "inactive""#),
        "Source type must not preserve declaration order here. Got: {}",
        ts2322.message_text
    );
}

#[test]
fn ts2322_mixed_number_string_literal_union_uses_tsc_display_order() {
    let source = r#"
type Target = "b";
declare const s: "a" | 1;
const x: Target = s;
"#;
    let diags = check_source_diagnostics(source);
    let ts2322 = diags
        .iter()
        .find(|d| d.code == 2322)
        .unwrap_or_else(|| panic!("Expected TS2322, got: {diags:?}"));
    assert!(
        ts2322.message_text.contains(r#"1 | "a""#),
        "Source type must display mixed literal union in tsc order. Got: {}",
        ts2322.message_text
    );
    assert!(
        !ts2322.message_text.contains(r#""a" | 1"#),
        "Source type must not preserve mixed literal declaration order. Got: {}",
        ts2322.message_text
    );
}
