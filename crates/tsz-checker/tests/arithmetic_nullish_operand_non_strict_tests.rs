//! Under `strictNullChecks: false`, a bare `null`/`undefined` operand is
//! assignable to `number` and `string`, so `+` with it is a well-typed
//! addition/concatenation — `tsc` reports nothing. tsz spuriously reported
//! TS2365 ("Operator '+' cannot be applied to types 'undefined' and 'number'").
//!
//! The witness family includes the immediately-invoked function expression
//! whose uncovered optional parameter is typed `undefined`: `((k?) => k + 1)()`
//! (conformance corpus `contextuallyTypedIife.ts`, `@strict: false`). Under
//! `strictNullChecks: true` the same operand instead yields TS18048 ("possibly
//! undefined"), which must NOT be suppressed — and a mixed `number + bigint`
//! (no nullish operand) must still report TS2365 regardless of mode.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_multi_file;

const TS2365: u32 = 2365;
const TS18048: u32 = 18048;

fn non_strict() -> CheckerOptions {
    CheckerOptions {
        strict_null_checks: false,
        no_implicit_any: false,
        ..CheckerOptions::default()
    }
}

fn strict() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        strict_null_checks: true,
        no_implicit_any: true,
        strict_function_types: true,
        ..CheckerOptions::default()
    }
}

fn codes(source: &str, options: CheckerOptions) -> Vec<u32> {
    check_multi_file(&[("test.ts", source)], "test.ts", options)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

/// `undefined + number` under non-strict: assignable to `number`, no TS2365.
#[test]
fn undefined_plus_number_non_strict_is_clean() {
    let diags = codes("declare let x: undefined;\nlet y = x + 1;\n", non_strict());
    assert!(
        !diags.contains(&TS2365),
        "undefined + number under strictNullChecks-off must not report TS2365; got {diags:?}"
    );
}

/// `null + number` under non-strict: same allowance as `undefined`.
#[test]
fn null_plus_number_non_strict_is_clean() {
    let diags = codes("declare let z: null;\nlet w = z + 1;\n", non_strict());
    assert!(
        !diags.contains(&TS2365),
        "null + number under strictNullChecks-off must not report TS2365; got {diags:?}"
    );
}

/// String concatenation with a nullish operand is also valid under non-strict.
#[test]
fn undefined_plus_string_non_strict_is_clean() {
    let diags = codes(
        "declare let x: undefined;\nlet y = x + \"a\";\n",
        non_strict(),
    );
    assert!(
        !diags.contains(&TS2365),
        "undefined + string under strictNullChecks-off must not report TS2365; got {diags:?}"
    );
}

/// IIFE witness: the uncovered optional param `k` is typed `undefined`, and
/// `k + 1` is clean under non-strict (`contextuallyTypedIife.ts`).
#[test]
fn iife_uncovered_optional_param_plus_is_clean_non_strict() {
    let diags = codes(
        "((k?) => k + 1)();\n((l, o?) => l + o)(12);\n",
        non_strict(),
    );
    assert!(
        !diags.contains(&TS2365),
        "uncovered optional IIFE param arithmetic must be clean under non-strict; got {diags:?}"
    );
}

/// Regression guard: under `strictNullChecks: true` the nullish operand is NOT
/// assignable to `number`, so the diagnostic must still fire (TS18048) — the
/// non-strict allowance must not leak into strict mode.
#[test]
fn undefined_plus_number_strict_still_flags() {
    let diags = codes("declare let x: undefined;\nlet y = x + 1;\n", strict());
    assert!(
        diags.contains(&TS18048),
        "undefined + number under strictNullChecks must still report TS18048; got {diags:?}"
    );
}

/// A nullish operand borrows its kind from the *other* operand, so with **two**
/// nullish operands there is nothing to borrow from and tsc still reports
/// TS2365 (`plusOperatorWithAnyOtherType.ts`: `null + undefined`, `null + null`,
/// `undefined + undefined`). The non-strict allowance must not over-suppress.
#[test]
fn both_nullish_operands_still_report_ts2365_non_strict() {
    let diags = codes(
        "var a = null + undefined;\nvar b = null + null;\nvar c = undefined + undefined;\n",
        non_strict(),
    );
    assert!(
        diags.contains(&TS2365),
        "two nullish operands have no numeric/string side and must still report TS2365; got {diags:?}"
    );
}

/// Regression guard: a genuinely invalid `+` with **no** nullish operand —
/// mixed `number + bigint` — must still report TS2365 under non-strict. The
/// suppression is scoped to nullish operands only.
#[test]
fn mixed_number_bigint_still_reports_ts2365_non_strict() {
    let diags = codes(
        "declare let n: number;\ndeclare let b: bigint;\nlet r = n + b;\n",
        non_strict(),
    );
    assert!(
        diags.contains(&TS2365),
        "mixed number + bigint must still report TS2365 (no nullish operand); got {diags:?}"
    );
}
