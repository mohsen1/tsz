//! Tests for TS2339 on property access through an optional chain whose
//! receiver is exactly nullish.
//!
//! Closes #5938. The structural rule:
//!
//! > When the receiver of an optional chain (`?.`) has type `null`,
//! > `undefined`, or `null | undefined` (i.e. no non-nullish slice
//! > remains after splitting), the chain always short-circuits. tsc
//! > emits TS2339 ("Property 'X' does not exist on type 'never'.") at
//! > the property name, because the property access is unreachable.
//!
//! The fix lives in `handle_possibly_null_or_undefined_access`
//! (`crates/tsz-checker/src/types/property_access_type/nullish_access.rs`):
//! when `question_dot_token` is true and `property_type` is `None`
//! (no non-null union members), emit TS2339 with `TypeId::NEVER` at the
//! property name node.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn diags_strict(source: &str) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    check_source(source, "test.ts", options)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

fn has_ts2339_never(diags: &[(u32, String)], prop: &str) -> bool {
    diags
        .iter()
        .any(|(c, m)| *c == 2339 && m.contains(&format!("'{prop}'")) && m.contains("'never'"))
}

#[test]
fn null_literal_optional_chain_method() {
    let source = "const a = null?.valueOf();\n";
    let diags = diags_strict(source);
    assert!(
        has_ts2339_never(&diags, "valueOf"),
        "expected TS2339 'valueOf' / 'never' for `null?.valueOf()`, got: {diags:?}",
    );
}

#[test]
fn undefined_literal_optional_chain_method() {
    // Same as above but with `undefined` — name must not be hardcoded.
    let source = "const b = undefined?.toString();\n";
    let diags = diags_strict(source);
    assert!(
        has_ts2339_never(&diags, "toString"),
        "expected TS2339 'toString' / 'never' for `undefined?.toString()`, got: {diags:?}",
    );
}

#[test]
fn typed_null_only_optional_chain_property() {
    // Receiver typed as exactly `null` via assertion. The optional chain
    // still short-circuits because the non-nullish slice is `never`.
    let source = "const x: null = null;\nconst y = x?.foo;\n";
    let diags = diags_strict(source);
    assert!(
        has_ts2339_never(&diags, "foo"),
        "expected TS2339 'foo' / 'never' for `(x: null)?.foo`, got: {diags:?}",
    );
}

#[test]
fn null_or_undefined_union_optional_chain() {
    // Union of only nullish constituents — still never any non-nullish slice.
    let source = "const x: null | undefined = null;\nconst y = x?.bar;\n";
    let diags = diags_strict(source);
    assert!(
        has_ts2339_never(&diags, "bar"),
        "expected TS2339 'bar' / 'never' for `(x: null | undefined)?.bar`, got: {diags:?}",
    );
}

#[test]
fn element_access_on_literal_null() {
    // Indexed (element) access form: `null?.["valueOf"]`. The fix uses
    // `name_or_argument` so this position should also trigger TS2339.
    let source = "const z = null?.[\"valueOf\"];\n";
    let diags = diags_strict(source);
    assert!(
        diags
            .iter()
            .any(|(c, m)| *c == 2339 && m.contains("'never'")),
        "expected TS2339 / 'never' for `null?.[...]`, got: {diags:?}",
    );
}

// =========================================================================
// Regression guards — non-nullish-only receivers must NOT trigger TS2339.
// =========================================================================

#[test]
fn non_nullish_optional_chain_no_ts2339() {
    // `string | null` has a non-nullish slice (`string`), so `valueOf`
    // resolves on it and no TS2339 should fire.
    let source = "declare const s: string | null;\nconst r = s?.valueOf();\n";
    let diags = diags_strict(source);
    assert!(
        !diags.iter().any(|(c, _)| *c == 2339),
        "should not emit TS2339 when receiver has a non-nullish slice, got: {diags:?}",
    );
}

#[test]
fn plain_dot_on_null_keeps_18047() {
    // Regression: plain `.` (no question dot) on `null` literal should
    // still emit TS18047 ("The value cannot be used here"), not TS2339.
    let source = "const a = null;\nconst b = a.toString();\n";
    let diags = diags_strict(source);
    // Either TS18047 or TS2531 (object is possibly null) is acceptable —
    // the regression we care about is that the optional-chain fix didn't
    // change the non-optional path.
    let has_2339_never = diags
        .iter()
        .any(|(c, m)| *c == 2339 && m.contains("'never'"));
    assert!(
        !has_2339_never,
        "plain dot on null should not emit TS2339 / 'never' (that's the optional-chain rule), got: {diags:?}",
    );
}
