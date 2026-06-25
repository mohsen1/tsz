//! End-to-end recovery for a *missing required type operand* on a type
//! operator (`unique`/`readonly`).
//!
//! When the operand is absent (`type U = unique ;`), the parser already emits
//! TS1110 `Type expected` (covered by the parser-side unit tests). tsc does not
//! *also* fire the operand-shape grammar errors — TS1005 `'symbol' expected`
//! for `unique`, TS1354 (array/tuple) for `readonly` — on that missing node.
//! This suite pins the checker half of the behavior: the grammar errors are
//! suppressed for a missing operand but still fire for a *present* wrong operand.
//!
//! (TS1110 itself is a parse diagnostic and is asserted in
//! `tsz-parser`'s `missing_required_constituent_tests`; the checker test
//! harness here returns only checker diagnostics.)
//!
//! Owners: the missing-recovery guards in
//! `crates/tsz-checker/src/types/unique_symbol_arena.rs` and
//! `crates/tsz-checker/src/types/type_checking/type_alias_missing_name_coverage.rs`.

use tsz_checker::test_utils::check_source_diagnostics;

const SYMBOL_EXPECTED: u32 = 1005;
const READONLY_ONLY_ON_ARRAY_TUPLE: u32 = 1354;

fn codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn unique_without_operand_does_not_report_symbol_expected() {
    let c = codes("type U = unique ;");
    assert!(
        !c.contains(&SYMBOL_EXPECTED),
        "a missing `unique` operand must not report TS1005 (TS1110 already fired); got {c:?}"
    );
}

#[test]
fn readonly_without_operand_does_not_report_array_tuple_error() {
    // A type-alias body routes through the alias missing-name sweep.
    let alias = codes("type U = readonly ;");
    assert!(
        !alias.contains(&READONLY_ONLY_ON_ARRAY_TUPLE),
        "a missing `readonly` operand must not report TS1354 (TS1110 already fired); got {alias:?}"
    );
    // An annotation position routes through `get_type_from_type_operator`.
    let annot = codes("let v: readonly ;");
    assert!(
        !annot.contains(&READONLY_ONLY_ON_ARRAY_TUPLE),
        "a missing `readonly` operand in an annotation must not report TS1354; got {annot:?}"
    );
}

#[test]
fn unique_over_real_non_symbol_still_reports_symbol_expected() {
    // The missing-operand guard must not weaken the genuine grammar error:
    // a *present* non-symbol operand is still TS1005. Vary the operand keyword.
    for operand in ["number", "string", "boolean"] {
        let c = codes(&format!("declare const bad: unique {operand};"));
        assert!(
            c.contains(&SYMBOL_EXPECTED),
            "`unique {operand}` must still report TS1005; got {c:?}"
        );
    }
}

#[test]
fn readonly_over_real_non_array_still_reports_array_tuple_error() {
    for operand in ["string", "number", "Foo"] {
        let c = codes(&format!("type U = readonly {operand};"));
        assert!(
            c.contains(&READONLY_ONLY_ON_ARRAY_TUPLE),
            "`readonly {operand}` must still report TS1354; got {c:?}"
        );
    }
}

#[test]
fn well_formed_type_operators_report_no_grammar_error() {
    for source in [
        "type K = keyof number;",
        "type R = readonly string[];",
        "type R2 = readonly [string, number];",
        "declare const s: unique symbol;",
    ] {
        let c = codes(source);
        assert!(
            !c.contains(&SYMBOL_EXPECTED) && !c.contains(&READONLY_ONLY_ON_ARRAY_TUPLE),
            "`{source}` should not report an operand-shape grammar error; got {c:?}"
        );
    }
}
