//! Parity tests for tsc's `checkNonNullType` on operator operands.
//!
//! Structural rule: under `strictNullChecks`, tsc runs `checkNonNullType` on the
//! operand(s) of unary `+`/`-`/`~` and of binary arithmetic/relational/bitwise
//! operators. A nullable operand is reported as:
//!   * TS18047 / TS18048 / TS18049 ("'<x>' is possibly 'null'/'undefined'/...")
//!     when the operand is a named entity (identifier, property access, `this`);
//!   * TS2531 / TS2532 / TS2533 ("Object is possibly 'null'/'undefined'/...")
//!     when the operand is an unnamed expression (a call result, a parenthesized
//!     expression, etc.);
//!   * TS18050 ("The value '<x>' cannot be used here.") only for the literal
//!     `null` / `undefined` keyword.
//!
//! For unary `+`/`-`/`~` this check is UNCONDITIONAL — there is no arithmetic-operand
//! (TS2362) check for unary arithmetic, so the nullish diagnostic fires regardless of
//! the non-nullish remainder's kind (`number | undefined`, bare `null`/`undefined`,
//! `string | undefined`, `object | null`, `symbol | null`, a type parameter with a
//! nullable constraint, …).
//!
//! Owner: `error_reporter/operator_errors.rs` (`check_nullish_unary_operand`,
//! `emit_nullish_operand_error`).

use crate::test_utils::check_source_strict_codes as strict;

fn has(codes: &[u32], c: u32) -> bool {
    codes.contains(&c)
}

// =========================================================================
// Unary `+`/`-`/`~` on a *named* nullable operand of any non-null kind.
// =========================================================================

#[test]
fn unary_on_bare_null_named_emits_ts18047() {
    for op in ["+", "-", "~"] {
        let codes = strict(&format!("declare const x: null;\nconst _ = {op}x;\n"));
        assert!(
            has(&codes, 18047),
            "{op}x (x: null) -> TS18047; got {codes:?}"
        );
    }
}

#[test]
fn unary_on_bare_undefined_named_emits_ts18048() {
    for op in ["+", "-", "~"] {
        let codes = strict(&format!("declare const x: undefined;\nconst _ = {op}x;\n"));
        assert!(
            has(&codes, 18048),
            "{op}x (x: undefined) -> TS18048; got {codes:?}"
        );
    }
}

#[test]
fn unary_on_null_or_undefined_named_emits_ts18049() {
    for op in ["+", "-", "~"] {
        let codes = strict(&format!(
            "declare const x: number | null | undefined;\nconst _ = {op}x;\n"
        ));
        assert!(
            has(&codes, 18049),
            "{op}x (number|null|undefined) -> TS18049; got {codes:?}"
        );
    }
}

#[test]
fn unary_on_string_or_undefined_emits_ts18048_not_arithmetic() {
    // Non-arithmetic non-null remainder: tsc still reports the nullish operand and
    // there is NO unary TS2362 (unlike binary arithmetic).
    for op in ["+", "-", "~"] {
        let codes = strict(&format!(
            "declare const s: string | undefined;\nconst _ = {op}s;\n"
        ));
        assert!(
            has(&codes, 18048),
            "{op}s (string|undefined) -> TS18048; got {codes:?}"
        );
        assert!(
            !has(&codes, 2362),
            "unary {op} must not emit TS2362; got {codes:?}"
        );
    }
}

#[test]
fn unary_on_object_or_null_emits_ts18047() {
    for op in ["+", "-", "~"] {
        let codes = strict(&format!(
            "declare const o: object | null;\nconst _ = {op}o;\n"
        ));
        assert!(
            has(&codes, 18047),
            "{op}o (object|null) -> TS18047; got {codes:?}"
        );
    }
}

#[test]
fn unary_on_symbol_or_null_emits_both_ts18047_and_ts2469() {
    // The nullish check is independent of the ESSymbol (TS2469) check; both fire.
    for op in ["+", "-", "~"] {
        let codes = strict(&format!(
            "declare const y: symbol | null;\nconst _ = {op}y;\n"
        ));
        assert!(
            has(&codes, 18047),
            "{op}y (symbol|null) -> TS18047; got {codes:?}"
        );
        assert!(
            has(&codes, 2469),
            "{op}y (symbol|null) -> TS2469; got {codes:?}"
        );
    }
}

#[test]
fn unary_on_type_parameter_with_nullable_constraint_reports() {
    let codes = strict("function f<T extends string | undefined>(t: T) {\n  const _ = +t;\n}\n");
    assert!(
        has(&codes, 18048),
        "+t (T extends string|undefined) -> TS18048; got {codes:?}"
    );
    let codes = strict("function f<T extends number | null>(t: T) {\n  const _ = -t;\n}\n");
    assert!(
        has(&codes, 18047),
        "-t (T extends number|null) -> TS18047; got {codes:?}"
    );
}

// =========================================================================
// Unnamed nullish operand -> TS2531/2532/2533 (NOT TS18050).
// =========================================================================

#[test]
fn unary_on_unnamed_call_result_emits_object_possibly_code() {
    let codes = strict("declare function g(): number | undefined;\nconst _ = +g();\n");
    assert!(
        has(&codes, 2532),
        "+g() (number|undefined) -> TS2532; got {codes:?}"
    );
    assert!(
        !has(&codes, 18050),
        "must not use literal-value TS18050; got {codes:?}"
    );

    let codes = strict("declare function g(): number | null;\nconst _ = -g();\n");
    assert!(
        has(&codes, 2531),
        "-g() (number|null) -> TS2531; got {codes:?}"
    );
}

#[test]
fn binary_on_unnamed_call_result_emits_object_possibly_code() {
    // Same `checkNonNullType` rule for binary arithmetic / relational / bitwise.
    let codes = strict("declare function g(): number | undefined;\nconst _ = g() - 1;\n");
    assert!(has(&codes, 2532), "g() - 1 -> TS2532; got {codes:?}");
    assert!(
        !has(&codes, 18050),
        "binary unnamed operand must not be TS18050; got {codes:?}"
    );

    let codes = strict("declare function g(): number | undefined;\nconst _ = g() < 1;\n");
    assert!(has(&codes, 2532), "g() < 1 -> TS2532; got {codes:?}");

    let codes = strict("declare function g(): number | undefined;\nconst _ = g() & 1;\n");
    assert!(has(&codes, 2532), "g() & 1 -> TS2532; got {codes:?}");
}

#[test]
fn binary_unnamed_nonarithmetic_emits_object_possibly_and_ts2362() {
    // Binary arithmetic DOES additionally run the arithmetic-operand check (TS2362).
    let codes = strict("declare function g(): string | undefined;\nconst _ = g() - 1;\n");
    assert!(
        has(&codes, 2532),
        "g():string|undefined - 1 -> TS2532; got {codes:?}"
    );
    assert!(
        has(&codes, 2362),
        "g():string|undefined - 1 -> TS2362; got {codes:?}"
    );
    assert!(!has(&codes, 18050), "got {codes:?}");
}

// =========================================================================
// Literal `null`/`undefined` keyword still -> TS18050 (unchanged path).
// =========================================================================

#[test]
fn unary_on_literal_null_keyword_keeps_ts18050() {
    let codes = strict("const _ = +null;\n");
    assert!(
        has(&codes, 18050),
        "+null literal keyword -> TS18050; got {codes:?}"
    );
    assert!(
        !has(&codes, 18047),
        "literal keeps TS18050 not TS18047; got {codes:?}"
    );
}

// =========================================================================
// Negatives: no false positives.
// =========================================================================

#[test]
fn unary_on_void_no_nullish_error() {
    // `void` is not in tsc's Nullable flag set.
    for op in ["+", "-", "~"] {
        let codes = strict(&format!("declare const v: void;\nconst _ = {op}v;\n"));
        assert!(!has(&codes, 18047) && !has(&codes, 18048) && !has(&codes, 18049));
        assert!(!has(&codes, 2531) && !has(&codes, 2532) && !has(&codes, 2533));
    }
}

#[test]
fn unary_on_any_no_nullish_error() {
    for op in ["+", "-", "~"] {
        let codes = strict(&format!("declare const a: any;\nconst _ = {op}a;\n"));
        assert!(
            !has(&codes, 18047) && !has(&codes, 18048) && !has(&codes, 18049),
            "{codes:?}"
        );
    }
}

#[test]
fn unary_on_plain_number_no_nullish_error() {
    for op in ["+", "-", "~"] {
        let codes = strict(&format!("declare const n: number;\nconst _ = {op}n;\n"));
        assert!(!has(&codes, 18047) && !has(&codes, 18048), "{codes:?}");
    }
}

#[test]
fn unary_after_narrowing_no_nullish_error() {
    let codes = strict(
        "declare const a: number | undefined;\nif (a !== undefined) {\n  const _ = -a;\n}\n",
    );
    assert!(
        !has(&codes, 18048),
        "narrowed operand -> no TS18048; got {codes:?}"
    );
}

#[test]
fn intersection_with_empty_object_strips_nullability_no_error() {
    let codes = strict("declare const x: (number | null) & {};\nconst _ = +x;\n");
    assert!(
        !has(&codes, 18047),
        "(number|null) & {{}} is non-null; got {codes:?}"
    );
}
