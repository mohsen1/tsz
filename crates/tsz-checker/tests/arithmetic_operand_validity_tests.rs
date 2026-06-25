//! Conformance matrix for the per-operand TS2362/TS2363 validity check on
//! arithmetic (`- * / % **`) and bitwise (`& | ^ << >> >>>`) operators.
//!
//! Structural rule: `tsc`'s `checkArithmeticOperandType` is invoked once per
//! operand and is *independent of the other operand* — each side must be
//! assignable to `number | bigint` after `checkNonNullType` (which turns an
//! unknown-under-strict-null operand into `error` and strips `null`/`undefined`).
//! Numeric values (`number`, numeric enums, `bigint`) and the wildcards
//! `any`/`unknown`/`error`/`never` are valid; `string`, `boolean`, object,
//! `symbol`, `void`, string literals and *string enums* are not.
//!
//! tsz previously only ran this check when one operand was already `any`/`error`,
//! and additionally treated every enum (including string enums) as valid, so it
//! silently accepted e.g. `stringEnum - number`, `string & never`, and the
//! non-unknown side of `unknown - string`. The fix makes the check run once,
//! up-front, for every operand pair.
//!
//! Anti-hardcoding: the rule is structural. Binder names (enum/alias/parameter)
//! vary across the matrix and the behavior holds; nothing keys off identifiers.

use tsz_checker::test_utils::check_source_strict_codes;

fn count(codes: &[u32], code: u32) -> usize {
    codes.iter().filter(|&&c| c == code).count()
}

/// `expr;` where the operands are declared above. Returns the diagnostic codes.
fn check_expr(decls: &str, expr: &str) -> Vec<u32> {
    let source = format!("export {{}};\n{decls}\nconst __r = {expr};\n");
    check_source_strict_codes(&source)
}

// ── String enums are not arithmetic operands (the headline regression) ──────

#[test]
fn string_enum_left_operand_reports_ts2362() {
    // `StrColor.Red - 1`: left is a string enum -> TS2362, right is fine.
    let codes = check_expr(
        "enum StrColor { Red = \"r\", Blue = \"b\" }",
        "StrColor.Red - 1",
    );
    assert_eq!(
        count(&codes, 2362),
        1,
        "string-enum left operand must report TS2362: {codes:?}"
    );
    assert_eq!(
        count(&codes, 2363),
        0,
        "numeric-literal right operand is valid: {codes:?}"
    );
}

#[test]
fn string_enum_right_operand_reports_ts2363() {
    let codes = check_expr(
        "enum Suit { Hearts = \"h\", Spades = \"s\" }",
        "10 * Suit.Hearts",
    );
    assert_eq!(
        count(&codes, 2363),
        1,
        "string-enum right operand must report TS2363: {codes:?}"
    );
    assert_eq!(
        count(&codes, 2362),
        0,
        "numeric-literal left operand is valid: {codes:?}"
    );
}

#[test]
fn both_string_enum_operands_report_both_sides() {
    let codes = check_expr("enum A { X = \"x\" }\nenum B { Y = \"y\" }", "A.X - B.Y");
    assert_eq!(
        count(&codes, 2362),
        1,
        "left string enum -> TS2362: {codes:?}"
    );
    assert_eq!(
        count(&codes, 2363),
        1,
        "right string enum -> TS2363: {codes:?}"
    );
}

#[test]
fn string_enum_with_any_other_operand_still_reports() {
    // The other operand being `any` must not suppress the invalid string enum:
    // tsc checks each operand independently.
    let lhs = check_expr("enum E { A = \"a\" }\ndeclare const x: any;", "E.A & x");
    assert_eq!(
        count(&lhs, 2362),
        1,
        "string-enum left + any right -> TS2362: {lhs:?}"
    );
    let rhs = check_expr("enum E { A = \"a\" }\ndeclare const x: any;", "x & E.A");
    assert_eq!(
        count(&rhs, 2363),
        1,
        "any left + string-enum right -> TS2363: {rhs:?}"
    );
}

// ── Numeric enums remain valid arithmetic operands (no false positive) ──────

#[test]
fn numeric_enum_operands_are_valid() {
    let codes = check_expr("enum Dir { Up = 1, Down = 2 }", "Dir.Up - Dir.Down");
    assert_eq!(
        count(&codes, 2362),
        0,
        "numeric enum is a valid operand: {codes:?}"
    );
    assert_eq!(
        count(&codes, 2363),
        0,
        "numeric enum is a valid operand: {codes:?}"
    );
}

#[test]
fn numeric_enum_with_number_is_valid() {
    let codes = check_expr("enum Lvl { Lo = 0, Hi = 9 }", "Lvl.Hi * 3");
    assert_eq!(
        count(&codes, 2362) + count(&codes, 2363),
        0,
        "numeric enum * number is valid: {codes:?}"
    );
}

// ── never-paired: the invalid operand is still reported ─────────────────────

#[test]
fn never_paired_invalid_operand_is_reported() {
    // `string - never`: left invalid -> TS2362; `never` is assignable to
    // number|bigint so the right side is fine. Previously the evaluator
    // short-circuited on `never` and emitted nothing.
    let left = check_expr("declare const s: string;\ndeclare const n: never;", "s - n");
    assert_eq!(
        count(&left, 2362),
        1,
        "string - never must report TS2362 on the string: {left:?}"
    );
    assert_eq!(
        count(&left, 2363),
        0,
        "never right operand is valid: {left:?}"
    );

    let right = check_expr("declare const s: string;\ndeclare const n: never;", "n - s");
    assert_eq!(
        count(&right, 2363),
        1,
        "never - string must report TS2363 on the string: {right:?}"
    );
    assert_eq!(
        count(&right, 2362),
        0,
        "never left operand is valid: {right:?}"
    );
}

// ── unknown-paired: the other operand is still checked alongside TS18046 ─────

#[test]
fn unknown_paired_operand_still_checks_other_side() {
    // `unknown - string`: unknown -> TS18046 (and is treated as valid), but the
    // string operand must still report TS2363. Previously the unknown branch
    // short-circuited before the other operand was checked.
    let codes = check_expr(
        "declare const u: unknown;\ndeclare const s: string;",
        "u - s",
    );
    assert_eq!(
        count(&codes, 18046),
        1,
        "unknown operand reports TS18046: {codes:?}"
    );
    assert_eq!(
        count(&codes, 2363),
        1,
        "the string operand is still invalid -> TS2363: {codes:?}"
    );
    assert_eq!(
        count(&codes, 2362),
        0,
        "unknown side is not an arithmetic-operand error: {codes:?}"
    );
}

// ── Other invalid primitives across the operator families ───────────────────

#[test]
fn invalid_primitive_operands_report_per_side() {
    for op in ["-", "*", "/", "%", "**", "&", "|", "^", "<<", ">>", ">>>"] {
        let codes = check_expr(
            "declare const s: string;\ndeclare const n: number;",
            &format!("s {op} n"),
        );
        assert_eq!(
            count(&codes, 2362),
            1,
            "`string {op} number` must report TS2362: {codes:?}"
        );
        assert_eq!(
            count(&codes, 2363),
            0,
            "number right operand is valid for `{op}`: {codes:?}"
        );
    }
}

#[test]
fn object_and_void_operands_are_invalid() {
    let obj = check_expr(
        "declare const o: { x: number };\ndeclare const n: number;",
        "o * n",
    );
    assert_eq!(count(&obj, 2362), 1, "object operand -> TS2362: {obj:?}");

    let void = check_expr("declare const v: void;\ndeclare const n: number;", "v - n");
    assert_eq!(count(&void, 2362), 1, "void operand -> TS2362: {void:?}");
}

// ── Boolean bitwise stays TS2447, not the per-operand error ─────────────────

#[test]
fn boolean_bitwise_reports_ts2447_not_per_operand() {
    let codes = check_expr(
        "declare const a: boolean;\ndeclare const b: boolean;",
        "a & b",
    );
    assert_eq!(
        count(&codes, 2447),
        1,
        "boolean & boolean -> TS2447: {codes:?}"
    );
    assert_eq!(
        count(&codes, 2362) + count(&codes, 2363),
        0,
        "no per-operand error when TS2447 fires: {codes:?}"
    );
}

#[test]
fn boolean_with_any_bitwise_reports_per_operand_not_ts2447() {
    // `boolean & any`: `any` is not `BooleanLike`, so tsc does NOT emit the
    // boolean-operator suggestion; the boolean operand is reported via TS2362.
    let left = check_expr("declare const a: boolean;\ndeclare const b: any;", "a & b");
    assert_eq!(
        count(&left, 2362),
        1,
        "boolean & any -> TS2362 on the boolean: {left:?}"
    );
    assert_eq!(
        count(&left, 2447),
        0,
        "TS2447 must not fire when one side is any: {left:?}"
    );

    let right = check_expr("declare const a: boolean;\ndeclare const b: any;", "b & a");
    assert_eq!(
        count(&right, 2363),
        1,
        "any & boolean -> TS2363 on the boolean: {right:?}"
    );
}

// ── Valid numeric/bigint pairs produce no per-operand error ─────────────────

#[test]
fn valid_numeric_and_bigint_pairs_have_no_operand_error() {
    let num = check_expr(
        "declare const a: number;\ndeclare const b: number;",
        "a - b",
    );
    assert_eq!(
        count(&num, 2362) + count(&num, 2363),
        0,
        "number - number is clean: {num:?}"
    );

    let big = check_expr(
        "declare const a: bigint;\ndeclare const b: bigint;",
        "a * b",
    );
    assert_eq!(
        count(&big, 2362) + count(&big, 2363),
        0,
        "bigint * bigint is clean: {big:?}"
    );
}

// ── Anti-hardcoding: rename every binder; behavior is structural ────────────

#[test]
fn string_enum_operand_rule_is_binder_name_independent() {
    let a = check_expr("enum Palette { Crimson = \"c\" }", "Palette.Crimson - 2");
    let b = check_expr("enum Mood { Calm = \"q\" }", "Mood.Calm - 2");
    assert_eq!(count(&a, 2362), 1, "{a:?}");
    assert_eq!(count(&b, 2362), 1, "{b:?}");
    assert_eq!(
        a, b,
        "diagnostic codes must not depend on binder names: {a:?} vs {b:?}"
    );
}
