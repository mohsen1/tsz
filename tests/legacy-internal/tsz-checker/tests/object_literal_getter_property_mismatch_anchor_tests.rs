//! A mismatched `get` accessor member in a fresh object literal is checked
//! directly against the target property type and anchored at the accessor's
//! own name — tsc's `elaborateElementwise` does not walk into the getter
//! body; it compares the checked property type (`getTypeOfSymbol`, the
//! getter's inferred/declared return type) at the accessor position, with no
//! object-literal head and no "Types of property X are incompatible" chain.
//!
//! Every expectation here was oracled against BOTH the pinned typescript
//! 7.0.2 (`scripts/conformance/oracle.sh --strict --lib es2022 --target
//! es2022`) and a local tsc 6.0.2 build; the two agree byte-for-byte on this
//! family. Binder names vary across cases so the behavior is structural, not
//! keyed to a spelling.

use crate::test_utils::check_source_diagnostics;

fn code_messages(source: &str) -> Vec<(u32, String)> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

/// Plain (non-union) interface target: single-line TS2322, no head/chain.
#[test]
fn plain_target_reports_bare_ts2322_at_accessor() {
    let diags = code_messages(
        r#"
interface S { val: number }
const a1: S = { get val() { return true; } };
"#,
    );
    assert_eq!(
        diags,
        vec![(
            2322,
            "Type 'boolean' is not assignable to type 'number'.".to_string()
        )]
    );
}

/// Union target: the accessor's return type is compared directly against the
/// union, not against one discriminant-matched arm's property re-wrapped in
/// an object-literal head.
#[test]
fn union_target_reports_bare_ts2322_at_accessor() {
    let diags = code_messages(
        r#"
type M = { tag: "y"; v: number } | { tag: "x"; v: string };
const b1: M = { tag: "y", get v() { return true; } };
"#,
    );
    assert_eq!(
        diags,
        vec![(
            2322,
            "Type 'boolean' is not assignable to type 'string | number'.".to_string()
        )]
    );
}

/// Negative control: a getter whose inferred return type matches the target
/// property produces no diagnostic at all.
#[test]
fn matching_getter_return_type_is_clean() {
    let diags = code_messages(
        r#"
interface S { val: number }
const c1: S = { get val() { return 1; } };
"#,
    );
    assert!(diags.is_empty(), "expected no diagnostics, got: {diags:?}");
}

/// Renamed binders: the routing/anchor behavior is structural, not keyed to
/// `val`/`S`.
#[test]
fn renamed_binders_report_bare_ts2322_at_accessor() {
    let diags = code_messages(
        r#"
interface Widget { count: string }
const d1: Widget = { get count() { return 5; } };
"#,
    );
    assert_eq!(
        diags,
        vec![(
            2322,
            "Type 'number' is not assignable to type 'string'.".to_string()
        )]
    );
}

/// A getter with an explicit return-type annotation: the checked property
/// type is the declared annotation, not a re-inferred body type, but the
/// anchor/no-chain behavior is identical.
#[test]
fn explicit_return_type_annotation_reports_bare_ts2322_at_accessor() {
    let diags = code_messages(
        r#"
interface S { val: number }
const e1: S = { get val(): boolean { return true; } };
"#,
    );
    assert_eq!(
        diags,
        vec![(
            2322,
            "Type 'boolean' is not assignable to type 'number'.".to_string()
        )]
    );
}
