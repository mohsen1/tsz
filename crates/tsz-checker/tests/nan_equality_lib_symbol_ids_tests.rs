//! Locks in that `is_identifier_reference_to_global_nan` recognizes lib-merged
//! NaN symbols whose arena was rewritten to the local one during lib-symbol
//! merging. The arena-Arc-based `symbol_is_from_lib` misses those — they have
//! a local `binder.symbol_arenas[id]` Arc even though they originated from a
//! lib — so the NaN guard now consults `binder.lib_symbol_ids` directly as a
//! fallback.
//!
//! The fix is intentionally scoped to NaN detection; widening the same
//! fallback into the global `symbol_is_from_lib` helper broke variance
//! computation for `Pick<X, ...>` (see
//! `tsz_core::parallel::core::tests::test_check_files_parallel_generic_indexed_access_variance_preserves_ts2322`).
//!
//! Regression: `nanEquality.ts` — `x === NaN` failed to emit TS2845 because
//! the lib's `declare var NaN: number` resolved to a SymbolId whose arena ptr
//! did not match any `lib_context`'s arena. `is_identifier_reference_to_global_nan`
//! returned false and the downstream NaN-equality check at binary.rs ran with
//! `is_left_nan = is_right_nan = false`, silently dropping every TS2845.

use tsz_checker::test_utils::check_source_codes;

#[test]
fn nan_equality_emits_ts2845() {
    let source = r#"
declare const x: number;
if (x === NaN) {}
if (NaN === x) {}
if (x !== NaN) {}
"#;
    let codes = check_source_codes(source);
    let count_2845 = codes.iter().filter(|&&c| c == 2845).count();
    assert_eq!(
        count_2845, 3,
        "expected TS2845 for each NaN equality; got {codes:?}",
    );
}

#[test]
fn nan_equality_with_user_local_nan_does_not_emit_ts2845() {
    // User-defined `NaN` parameter shadows the global. tsc treats
    // `value === NaN` as a normal number equality, NOT TS2845. Our fix
    // must preserve this — `lib_symbol_ids` should not contain user
    // parameter symbols, so the existing `symbol_is_from_lib` returns
    // false for the parameter and the NaN-equality path is skipped.
    let source = r#"
function t1(value: number, NaN: number) {
    return value === NaN;
}
function t2(NaN: number) {
    return NaN === NaN;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2845),
        "user-shadowed NaN parameter should not trigger TS2845; got {codes:?}",
    );
}

#[test]
fn nan_inequality_emits_ts2845_with_true_message() {
    // `x !== NaN` and `x != NaN` are always true (NaN is unequal to
    // everything). The diagnostic message reflects this.
    let source = r#"
declare const x: number;
const a = x !== NaN;
const b = x != NaN;
"#;
    let codes = check_source_codes(source);
    let count_2845 = codes.iter().filter(|&&c| c == 2845).count();
    assert_eq!(
        count_2845, 2,
        "expected TS2845 for both `!==` and `!=` against NaN; got {codes:?}",
    );
}
