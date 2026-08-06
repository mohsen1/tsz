//! Regression tests for issue #16637: an object-literal computed `symbol` key
//! validated against a target's `[k: string]` index instead of its
//! `[k: symbol]` index when both are present on the same shape.
//!
//! `tsc`'s `getApplicableIndexInfo` routes a `symbol`-keyed computed property
//! through the target's `[k: symbol]` index exclusively — a `[k: string]` (or
//! `[k: number]`) index never applies to a symbol key, even when the target
//! also carries one.
//!
//! The unit harness runs with no lib (`CheckerOptions::default()`), so
//! `Symbol()` itself does not resolve (`TS2583`) — every case uses
//! `declare const s: unique symbol;` instead, matching the rest of this
//! crate's symbol-index test suite.

use tsz_checker::diagnostics::diagnostic_codes;
use tsz_checker::test_utils::check_source_diagnostics;

fn diagnostic_codes_for_ts(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

fn assert_clean(source: &str) {
    let codes = diagnostic_codes_for_ts(source);
    assert!(codes.is_empty(), "expected no diagnostics, got {codes:?}");
}

fn assert_only_ts2418(source: &str) {
    let codes = diagnostic_codes_for_ts(source);
    assert_eq!(
        codes,
        vec![diagnostic_codes::TYPE_OF_COMPUTED_PROPERTYS_VALUE_IS_WHICH_IS_NOT_ASSIGNABLE_TO_TYPE],
        "expected exactly one TS2418, got {codes:?}"
    );
}

// tsc: clean — `[sym]: "x"` is checked against the `[k: symbol]: string`
// index, not the `[k: string]: number` index. Was a false positive TS2418.
#[test]
fn symbol_key_matching_value_against_symbol_index_stays_clean_with_string_index_present() {
    assert_clean(
        r#"
declare const sym: unique symbol;
interface I { [k: string]: number; [k: symbol]: string; }
const i: I = { a: 1, [sym]: "x" };
"#,
    );
}

// tsc: TS2418 `number` not assignable to `string` — the symbol index's value
// type, not the string index's. Was a false negative (silently clean).
#[test]
fn symbol_key_mismatched_value_reports_ts2418_against_symbol_index_value_type() {
    assert_only_ts2418(
        r#"
declare const sym: unique symbol;
interface I { [k: string]: number; [k: symbol]: string; }
const i: I = { [sym]: 1 };
"#,
    );
}

// tsc: clean — a symbol key uncovered by any index (string-only target) is
// not excess and not a value mismatch; tsc has nothing to check it against.
// Was a false positive TS2418 (matched via the string index).
#[test]
fn symbol_key_uncovered_by_string_only_index_stays_clean() {
    assert_clean(
        r#"
declare const sym: unique symbol;
interface I { [k: string]: number; }
const i: I = { [sym]: "x" };
"#,
    );
}

// Renamed-binder control: the fix must not key off any literal identifier
// text ("sym"/"I"), only the symbol-ness of the computed key.
#[test]
fn symbol_key_renamed_binders_same_behavior() {
    assert_clean(
        r#"
declare const mySymbol: unique symbol;
interface Target { [index: string]: number; [index: symbol]: string; }
const target: Target = { field: 1, [mySymbol]: "ok" };
"#,
    );
    assert_only_ts2418(
        r#"
declare const mySymbol: unique symbol;
interface Target { [index: string]: number; [index: symbol]: string; }
const target: Target = { [mySymbol]: 42 };
"#,
    );
}

// Negative control: an ordinary string-keyed property must still route
// through the string index exactly as before (not affected by the
// `is_symbol_named` guard added ahead of the `STRING` short-circuit). A
// plain (non-computed) named-property mismatch keeps tsc's ordinary TS2322,
// not the computed-property TS2418.
#[test]
fn ordinary_string_key_still_matches_string_index_with_symbol_index_present() {
    assert_clean(
        r#"
interface I { [k: string]: number; [k: symbol]: string; }
const i: I = { plain: 1 };
"#,
    );
    let codes = diagnostic_codes_for_ts(
        r#"
interface I { [k: string]: number; [k: symbol]: string; }
const i: I = { plain: "wrong" };
"#,
    );
    assert_eq!(
        codes,
        vec![diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE],
        "expected exactly one TS2322 for a plain named-property mismatch, got {codes:?}"
    );
}

// Negative control: a numeric key must still route through the number index,
// unaffected by the symbol-key guard (kept free of a `symbol_index` sibling
// so it does not also exercise an unrelated, pre-existing numeric-literal
// index-signature gap).
#[test]
fn numeric_key_still_matches_number_index() {
    assert_clean(
        r#"
interface I { [k: number]: string; [k: string]: string; }
const i: I = { 0: "ok" };
"#,
    );
}

// Positive control: a declared `unique symbol` NAMED member keeps its own
// named-member path (unaffected — this is not an index-signature lookup at
// all, the source and target key resolve to the same synthetic atom).
#[test]
fn unique_symbol_named_member_unaffected_by_symbol_index_guard() {
    assert_clean(
        r#"
declare const S: unique symbol;
interface I { [S]: number; [k: string]: string; }
const i: I = { [S]: 1 };
"#,
    );
    assert_only_ts2418(
        r#"
declare const S: unique symbol;
interface I { [S]: number; [k: string]: string; }
const i: I = { [S]: "bad" };
"#,
    );
}
