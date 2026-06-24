//! TS2791 — `Exponentiation cannot be performed on 'bigint' values unless the
//! 'target' option is set to 'es2016' or later`.
//!
//! tsc emits TS2791 only when the *result* type of `**`/`**=` is `bigint`.
//! `checkBinaryLikeExpression` resolves the arithmetic result by trying the
//! number branch before the bigint branch, treating `any`/`unknown`/`error`/
//! `never` as wildcards that satisfy both branches (the number branch wins for
//! a wildcard pair). These tests pin that branch precedence: a wildcard pair
//! resolves to `number` and must not fire, while a wildcard paired with a
//! genuine `bigint` resolves to `bigint` and must fire.

use crate::context::{CheckerOptions, ScriptTarget};
use crate::test_utils::{check_source, diagnostic_count};

const TS2791: u32 = 2791;

fn count_2791_at(target: ScriptTarget, source: &str) -> usize {
    let diags = check_source(
        source,
        "test.ts",
        CheckerOptions {
            target,
            ..CheckerOptions::default()
        },
    );
    diagnostic_count(&diags, TS2791)
}

fn count_2791_es2015(source: &str) -> usize {
    count_2791_at(ScriptTarget::ES2015, source)
}

// ---------------------------------------------------------------------------
// False-positive regressions: wildcard pairs resolve to `number`, never bigint.
// ---------------------------------------------------------------------------

#[test]
fn unresolved_name_operands_do_not_fire_ts2791() {
    // Both operands are `error` types (unresolved names). tsc reports TS2304
    // for the names but the arithmetic result is `number`, so no TS2791.
    assert_eq!(
        count_2791_es2015("const out = missingLhs ** missingRhs;"),
        0
    );
}

#[test]
fn unresolved_name_chain_does_not_fire_ts2791() {
    // Right-associative `a ** b ** c` — the inner `b ** c` is `error ** error`.
    assert_eq!(count_2791_es2015("const out = aRef ** bRef ** cRef;"), 0);
}

#[test]
fn never_pair_does_not_fire_ts2791() {
    let source = "declare const lo: never; declare const hi: never; const out = lo ** hi;";
    assert_eq!(count_2791_es2015(source), 0);
}

#[test]
fn any_pair_does_not_fire_ts2791() {
    let source = "declare const lhs: any; declare const rhs: any; const out = lhs ** rhs;";
    assert_eq!(count_2791_es2015(source), 0);
}

#[test]
fn unknown_pair_does_not_fire_ts2791() {
    let source = "declare const lhs: unknown; declare const rhs: unknown; const out = lhs ** rhs;";
    assert_eq!(count_2791_es2015(source), 0);
}

// ---------------------------------------------------------------------------
// False-negative regressions: a wildcard paired with a genuine bigint resolves
// to `bigint` and must fire.
// ---------------------------------------------------------------------------

#[test]
fn any_times_bigint_fires_ts2791() {
    let source = "declare const lhs: any; declare const rhs: bigint; const out = lhs ** rhs;";
    assert_eq!(count_2791_es2015(source), 1);
}

#[test]
fn bigint_times_any_fires_ts2791() {
    let source = "declare const lhs: bigint; declare const rhs: any; const out = lhs ** rhs;";
    assert_eq!(count_2791_es2015(source), 1);
}

#[test]
fn unknown_times_bigint_fires_ts2791() {
    let source = "declare const lhs: unknown; declare const rhs: bigint; const out = lhs ** rhs;";
    assert_eq!(count_2791_es2015(source), 1);
}

#[test]
fn never_times_bigint_fires_ts2791() {
    let source = "declare const lhs: never; declare const rhs: bigint; const out = lhs ** rhs;";
    assert_eq!(count_2791_es2015(source), 1);
}

#[test]
fn unresolved_name_times_bigint_fires_ts2791() {
    // `error ** bigint`: number branch fails (bigint is not number-like) but the
    // bigint branch matches, so the result is `bigint` and TS2791 fires.
    let source = "declare const rhs: bigint; const out = missingName ** rhs;";
    assert_eq!(count_2791_es2015(source), 1);
}

// ---------------------------------------------------------------------------
// Unchanged baseline behavior.
// ---------------------------------------------------------------------------

#[test]
fn bigint_pair_fires_ts2791_below_es2016() {
    let source = "declare const lhs: bigint; declare const rhs: bigint; const out = lhs ** rhs;";
    assert_eq!(count_2791_es2015(source), 1);
}

#[test]
fn bigint_literal_pair_fires_ts2791_below_es2016() {
    assert_eq!(count_2791_es2015("const out = 2n ** 3n;"), 1);
}

#[test]
fn bigint_pair_is_clean_at_es2016() {
    let source = "declare const lhs: bigint; declare const rhs: bigint; const out = lhs ** rhs;";
    assert_eq!(count_2791_at(ScriptTarget::ES2016, source), 0);
}

#[test]
fn bigint_times_number_does_not_fire_ts2791() {
    // Mixed bigint/number is an arithmetic error (TS2362/TS2363 family), never a
    // bigint result, so TS2791 must not fire.
    let source = "declare const lhs: bigint; declare const rhs: number; const out = lhs ** rhs;";
    assert_eq!(count_2791_es2015(source), 0);
}

#[test]
fn numeric_enum_times_bigint_does_not_fire_ts2791() {
    let source = "enum Unit { Base } declare const rhs: bigint; const out = Unit.Base ** rhs;";
    assert_eq!(count_2791_es2015(source), 0);
}

// ---------------------------------------------------------------------------
// Compound `**=` mirrors the binary `**` precedence.
// ---------------------------------------------------------------------------

#[test]
fn compound_never_pair_does_not_fire_ts2791() {
    let source = "declare let acc: never; declare const step: never; acc **= step;";
    assert_eq!(count_2791_es2015(source), 0);
}

#[test]
fn compound_any_times_bigint_fires_ts2791() {
    let source = "declare let acc: any; declare const step: bigint; acc **= step;";
    assert_eq!(count_2791_es2015(source), 1);
}

#[test]
fn compound_bigint_pair_fires_ts2791_below_es2016() {
    let source = "declare let acc: bigint; declare const step: bigint; acc **= step;";
    assert_eq!(count_2791_es2015(source), 1);
}

#[test]
fn compound_bigint_pair_is_clean_at_es2016() {
    let source = "declare let acc: bigint; declare const step: bigint; acc **= step;";
    assert_eq!(count_2791_at(ScriptTarget::ES2016, source), 0);
}
