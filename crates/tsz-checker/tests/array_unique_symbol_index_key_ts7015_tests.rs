//! Issue #16948-adjacent: array/tuple index-key evaluation for `unique
//! symbol` keys.
//!
//! Structural rule: `Array<T>[K]` where `K` is a `unique symbol` (a
//! well-known symbol like `Symbol.isConcatSpreadable`, or a user `declare
//! const s: unique symbol`) is not a numeric/string index contributor. tsc
//! reports TS7015 ("index expression is not of type 'number'") under
//! `noImplicitAny`/strict, matching the existing bare-`symbol` case; it
//! never treats the key as if it matched the array's own numeric index
//! signature. tsz's `ArrayKeyVisitor`
//! (crates/tsz-solver/src/evaluation/evaluate_rules/index_access_keys.rs)
//! had no `visit_unique_symbol` override, so unhandled key shapes fell
//! through `default_output()` -> `None` -> the element-type fallback in
//! `evaluate()`, wrongly checking the assigned value against the array's
//! element type instead of raising TS7015 — producing a spurious TS2322
//! whenever the RHS didn't happen to match the element type. Oracled
//! against `tsc` 7.0.2.

use tsz_checker::CheckerOptions;
use tsz_checker::diagnostics::diagnostic_codes;
use tsz_checker::test_utils::{
    check_source_diagnostics, check_source_with_libs_code_messages, load_default_lib_files,
};

fn diagnostic_codes_for_ts(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

#[test]
fn well_known_symbol_array_index_write_reports_ts7015_not_ts2322() {
    let libs = load_default_lib_files();
    if libs.is_empty() {
        return;
    }
    let diagnostics = check_source_with_libs_code_messages(
        r#"
let a = ['c', 'd'];
a[Symbol.isConcatSpreadable] = false;
"#,
        "test.ts",
        CheckerOptions::default(),
        &libs,
    );
    let codes: Vec<u32> = diagnostics.iter().map(|(code, _)| *code).collect();
    assert!(
        codes.contains(&diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_INDEX_EXPRESSION_IS_NOT_OF_TYPE_NUMBE),
        "expected TS7015 for a well-known-symbol array index write, got {diagnostics:?}",
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "well-known-symbol array index write must not fall back to the element type (spurious TS2322), got {diagnostics:?}",
    );
}

#[test]
fn unique_symbol_const_array_index_write_reports_ts7015_not_ts2322() {
    let codes = diagnostic_codes_for_ts(
        r#"
declare const s: unique symbol;
let a = ['c', 'd'];
a[s] = false;
"#,
    );
    assert!(
        codes.contains(&diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_INDEX_EXPRESSION_IS_NOT_OF_TYPE_NUMBE),
        "expected TS7015 for a `unique symbol` const array index write, got {codes:?}",
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "`unique symbol` array index write must not fall back to the element type (spurious TS2322), got {codes:?}",
    );
}

// Renamed binder — proves the fix is structural, not keyed on `s`'s spelling.
#[test]
fn unique_symbol_const_array_index_write_reports_ts7015_renamed_binder() {
    let codes = diagnostic_codes_for_ts(
        r#"
declare const uniqueKey: unique symbol;
let letters = ['c', 'd'];
letters[uniqueKey] = false;
"#,
    );
    assert!(
        codes.contains(&diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_INDEX_EXPRESSION_IS_NOT_OF_TYPE_NUMBE),
        "expected TS7015 regardless of the unique-symbol binder's name, got {codes:?}",
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "renamed unique-symbol array index write must not report TS2322, got {codes:?}",
    );
}

// Negative control: plain (non-unique) `symbol` on an array already worked
// before this fix (`visit_intrinsic`'s `Symbol` arm) — regression guard.
#[test]
fn plain_symbol_array_index_write_still_reports_ts7015() {
    let codes = diagnostic_codes_for_ts(
        r#"
let s: symbol = Symbol();
let a = ['c', 'd'];
a[s] = false;
"#,
    );
    assert!(
        codes.contains(&diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_INDEX_EXPRESSION_IS_NOT_OF_TYPE_NUMBE),
        "expected TS7015 for a plain-symbol array index write (regression guard), got {codes:?}",
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "plain-symbol array index write must not report TS2322, got {codes:?}",
    );
}

// Negative control: numeric and string keys must still resolve to the
// element type via the array's real numeric index signature — the fix must
// not touch this path.
#[test]
fn numeric_and_string_array_index_write_unaffected() {
    let codes = diagnostic_codes_for_ts(
        r#"
let a = ['c', 'd'];
a[0] = 'x';
a['1'] = 'y';
"#,
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "numeric/string array index writes with matching element type must stay clean, got {codes:?}",
    );
    assert!(
        !codes.contains(&diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_INDEX_EXPRESSION_IS_NOT_OF_TYPE_NUMBE),
        "numeric/string array index writes must not report TS7015, got {codes:?}",
    );
}

#[test]
fn numeric_array_index_write_mismatch_still_reports_ts2322() {
    // Negative control: a genuine element-type mismatch through the real
    // numeric index signature must still report TS2322 — the fix narrows the
    // unique-symbol fallback only, not the legitimate numeric-key path.
    let codes = diagnostic_codes_for_ts(
        r#"
let a = ['c', 'd'];
a[0] = 42;
"#,
    );
    assert!(
        codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "expected TS2322 for a genuine numeric-index element-type mismatch, got {codes:?}",
    );
}

// Read access (not just write) must also route through TS7015, not silently
// resolve to the element type.
#[test]
fn unique_symbol_const_array_index_read_reports_ts7015() {
    let codes = diagnostic_codes_for_ts(
        r#"
declare const s: unique symbol;
let a = ['c', 'd'];
const v = a[s];
"#,
    );
    assert!(
        codes.contains(&diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_INDEX_EXPRESSION_IS_NOT_OF_TYPE_NUMBE),
        "expected TS7015 for a `unique symbol` array index read, got {codes:?}",
    );
}

// Unique-symbol index on a tuple was already correct (`TupleKeyVisitor`
// defaults to `UNDEFINED`, not the fallback bug) — regression guard so the
// two visitors don't drift.
#[test]
fn unique_symbol_const_tuple_index_write_reports_ts7015() {
    let codes = diagnostic_codes_for_ts(
        r#"
declare const s: unique symbol;
let t: [string, string] = ['c', 'd'];
t[s] = false;
"#,
    );
    assert!(
        codes.contains(&diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_INDEX_EXPRESSION_IS_NOT_OF_TYPE_NUMBE),
        "expected TS7015 for a `unique symbol` tuple index write, got {codes:?}",
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "unique-symbol tuple index write must not report TS2322, got {codes:?}",
    );
}
