//! Regression tests for issue #13510: a mapped type whose keys are numeric
//! *string* literals (e.g. `"0" | "1" | "2"`) must keep those keys as string
//! literals, so `keyof` reproduces `"0" | "1" | "2"` rather than the numeric
//! `0 | 1 | 2`.
//!
//! Structural rule: when a mapped type `{ [K in U]: V }` is instantiated, a
//! property materialized from a string-literal key whose text is numeric is
//! string-named — independent of whether the mapping is homomorphic. Without
//! this, indexing `M[P]` with `P extends U` spuriously reported TS2536 because
//! the `"0" | "1" | "2"`-constrained index could not index a `0 | 1 | 2`
//! key space. Owner: mapped-type instantiation / `keyof` in the solver.
//!
//! The hotscript witness is `DigitCompareTable[D1][D2]`; the matrix below also
//! exercises the single-level form, the `Record<...>` form, varied binder
//! names, the bare-numeric-key control (must stay numeric), and a negative
//! case that must still report a genuinely-missing key.

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;

fn check_es5(source: &str) -> Vec<Diagnostic> {
    let lib_files = tsz_checker::test_utils::load_lib_files(&["es5.d.ts"]);
    assert!(!lib_files.is_empty(), "es5.d.ts lib file not loaded");
    tsz_checker::test_utils::check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
        &lib_files,
    )
}

fn ts2536(diags: &[Diagnostic]) -> Vec<&Diagnostic> {
    diags.iter().filter(|d| d.code == 2536).collect()
}

#[test]
fn nested_digit_compare_table_no_ts2536() {
    // The hotscript witness, reduced: a mapped table whose value is itself a
    // mapped table, both keyed by a numeric-string union, indexed two levels
    // deep by constrained type parameters.
    let diags = check_es5(
        r#"
type Digit = "0" | "1" | "2";
type DigitCompareTable = {
  [D1 in Digit]: {
    [D2 in Digit]: boolean;
  };
};
type Compare<A extends Digit, B extends Digit> = DigitCompareTable[A][B];
"#,
    );
    assert!(
        ts2536(&diags).is_empty(),
        "nested numeric-string mapped table indexed by constrained params must not emit TS2536: {diags:?}"
    );
}

#[test]
fn single_level_numeric_string_mapped_index_no_ts2536() {
    let diags = check_es5(
        r#"
type Keys = "0" | "1" | "2";
type Table = { [K in Keys]: number };
type At<P extends Keys> = Table[P];
"#,
    );
    assert!(
        ts2536(&diags).is_empty(),
        "single-level numeric-string mapped index must not emit TS2536: {diags:?}"
    );
}

#[test]
fn record_with_numeric_string_keys_index_no_ts2536() {
    // `Record<K, V>` is the homomorphic `{ [P in K]: V }`; numeric-string keys
    // must survive it too.
    let diags = check_es5(
        r#"
type Keys = "0" | "1";
type R = Record<Keys, string>;
type At<Q extends Keys> = R[Q];
"#,
    );
    assert!(
        ts2536(&diags).is_empty(),
        "Record<numeric-string-union, V> indexed by a constrained param must not emit TS2536: {diags:?}"
    );
}

#[test]
fn keyof_numeric_string_mapped_is_string_literal() {
    // `keyof` of the instantiated mapped type must be the string literals, so a
    // string-literal index is a valid key and assignment through it type-checks.
    let diags = check_es5(
        r#"
type Keys = "0" | "1";
type Table = { [K in Keys]: number };
const ok: Table["0"] = 1;
const k: keyof Table = "1";
"#,
    );
    assert!(
        ts2536(&diags).is_empty(),
        "string-literal index into a numeric-string mapped type must be a valid key: {diags:?}"
    );
}

#[test]
fn bare_numeric_literal_keys_stay_numeric() {
    // Control: bare numeric-literal keys (`0 | 1`, not `"0" | "1"`) must remain
    // number-named. Indexing by a parameter constrained to the same numeric
    // union stays valid and emits no TS2536.
    let diags = check_es5(
        r#"
type NumKeys = 0 | 1;
type Table = { [K in NumKeys]: string };
type At<P extends NumKeys> = Table[P];
"#,
    );
    assert!(
        ts2536(&diags).is_empty(),
        "numeric-literal mapped keys indexed by a matching numeric param must not emit TS2536: {diags:?}"
    );
}

#[test]
fn missing_numeric_string_key_still_reported() {
    // The fix must not silence a genuinely-out-of-range key: indexing by a
    // parameter constrained to a wider union than the table's keys must still
    // be rejected (tsc reports TS2536 here).
    let diags = check_es5(
        r#"
type TableKeys = "0" | "1";
type WideKeys = "0" | "1" | "2";
type Table = { [K in TableKeys]: number };
type At<P extends WideKeys> = Table[P];
"#,
    );
    assert!(
        !ts2536(&diags).is_empty(),
        "indexing a numeric-string mapped table by a wider key union must still emit TS2536: {diags:?}"
    );
}
