//! Regression tests: an overload-set arity-mismatch diagnostic (TS2554/TS2575)
//! must report the *expanded* argument count (a spread of an array literal
//! contributes one entry per element), not the raw number of argument
//! expressions. See `resolve_signatures.rs`'s final "no overload matched on
//! arity" fallback, which previously used `args.len()` (raw expression count)
//! instead of the already-correctly-expanded `arg_types.len()`.

use crate::test_utils::check_source_diagnostics;

fn diagnostics_with_code(
    diagnostics: &[crate::diagnostics::Diagnostic],
    code: u32,
) -> Vec<&crate::diagnostics::Diagnostic> {
    diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == code)
        .collect()
}

fn diagnostic_messages<'a>(diagnostics: &[&'a crate::diagnostics::Diagnostic]) -> Vec<&'a str> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message_text.as_str())
        .collect()
}

/// Regression: when every overload of a call fails on argument count, tsc's
/// TS2554 "got N" must count expanded arguments (a spread of an array literal
/// contributes one entry per element), not raw argument *expressions*.
/// `f2(1, 2, 3, 4, 5, ...[6, 7])` has 6 argument expressions but supplies 7
/// arguments. Oracle-verified (`typescript@7.0.2`): "Expected 0-6 arguments,
/// but got 7." tsz's overload-resolution fallback previously reported "got 6"
/// here (`args.len()`, the raw expression count) even though the identical
/// call against a single signature already reported the correct count.
#[test]
fn overload_all_arity_mismatch_counts_expanded_spread_arguments() {
    let diags = check_source_diagnostics(
        r#"
declare function f2(): void;
declare function f2(a: number, b: number): void;
declare function f2(a: number, b: number, c: number, d: number): void;
declare function f2(a: number, b: number, c: number, d: number, e: number, f: number): void;
f2(1, 2, 3, 4, 5, ...[6, 7]);
"#,
    );

    let ts2554 = diagnostics_with_code(&diags, 2554);
    assert_eq!(
        diagnostic_messages(&ts2554),
        vec!["Expected 0-6 arguments, but got 7."],
        "expanded spread elements must count toward the reported argument total"
    );
}

/// Sibling of the above at the `OverloadArgumentCountMismatch` ("no overload
/// expects N, but overloads do exist that expect M or K") reporting path: a
/// spread landing exactly in the gap between two overloads' exact arities
/// must also report the expanded count.
#[test]
fn overload_gap_arity_mismatch_counts_expanded_spread_arguments() {
    let diags = check_source_diagnostics(
        r#"
declare function g(x: number): void;
declare function g(x: number, y: number, z: number): void;
g(1, ...[2]);
"#,
    );

    let ts2575 = diagnostics_with_code(&diags, 2575);
    assert_eq!(
        diagnostic_messages(&ts2575),
        vec![
            "No overload expects 2 arguments, but overloads do exist that expect either 1 or 3 arguments."
        ],
        "expanded spread elements must count toward the reported gap total"
    );
}

/// Non-spread control: plain-literal too-many-arguments overload failure is
/// unaffected by the expanded-count fix (raw and expanded counts coincide
/// when there is no spread).
#[test]
fn overload_all_arity_mismatch_plain_arguments_unaffected() {
    let diags = check_source_diagnostics(
        r#"
declare function f2(): void;
declare function f2(a: number, b: number): void;
declare function f2(a: number, b: number, c: number, d: number): void;
declare function f2(a: number, b: number, c: number, d: number, e: number, f: number): void;
f2(1, 2, 3, 4, 5, 6, 7);
"#,
    );

    let ts2554 = diagnostics_with_code(&diags, 2554);
    assert_eq!(
        diagnostic_messages(&ts2554),
        vec!["Expected 0-6 arguments, but got 7."]
    );
}
