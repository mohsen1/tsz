//! Tests for TS18048/TS18047: unary arithmetic operators on possibly-null/undefined operands.
//!
//! Structural rule: under `strictNullChecks`, applying a unary arithmetic operator
//! (`-`, `~`, `+`) to an operand whose type contains `null` or `undefined` as a union
//! member must emit TS18048 ("possibly 'undefined'") or TS18047 ("possibly 'null'"),
//! matching the same nullish-operand check that binary arithmetic already runs.
//!
//! Fixes issue #9745.

use crate::test_utils::check_source_strict_codes;

// =========================================================================
// Repro cases: number | undefined → TS18048
// =========================================================================

#[test]
fn unary_minus_on_possibly_undefined_number_emits_ts18048() {
    let codes = check_source_strict_codes("declare const a: number | undefined;\nconst m = -a;\n");
    assert!(
        codes.contains(&18048),
        "unary - on `number | undefined` must emit TS18048; got: {codes:?}"
    );
}

#[test]
fn unary_tilde_on_possibly_undefined_number_emits_ts18048() {
    let codes = check_source_strict_codes("declare const x: number | undefined;\nconst t = ~x;\n");
    assert!(
        codes.contains(&18048),
        "unary ~ on `number | undefined` must emit TS18048; got: {codes:?}"
    );
}

#[test]
fn unary_plus_on_possibly_undefined_number_emits_ts18048() {
    let codes = check_source_strict_codes("declare const v: number | undefined;\nconst p = +v;\n");
    assert!(
        codes.contains(&18048),
        "unary + on `number | undefined` must emit TS18048; got: {codes:?}"
    );
}

// =========================================================================
// number | null → TS18047
// =========================================================================

#[test]
fn unary_minus_on_possibly_null_number_emits_ts18047() {
    let codes = check_source_strict_codes("declare const b: number | null;\nconst n = -b;\n");
    assert!(
        codes.contains(&18047),
        "unary - on `number | null` must emit TS18047; got: {codes:?}"
    );
}

#[test]
fn unary_tilde_on_possibly_null_number_emits_ts18047() {
    let codes = check_source_strict_codes("declare const q: number | null;\nconst t = ~q;\n");
    assert!(
        codes.contains(&18047),
        "unary ~ on `number | null` must emit TS18047; got: {codes:?}"
    );
}

#[test]
fn unary_plus_on_possibly_null_number_emits_ts18047() {
    let codes = check_source_strict_codes("declare const r: number | null;\nconst p = +r;\n");
    assert!(
        codes.contains(&18047),
        "unary + on `number | null` must emit TS18047; got: {codes:?}"
    );
}

// =========================================================================
// Name-variation: the rule must not be tied to any specific variable name
// =========================================================================

#[test]
fn unary_minus_different_variable_names_emit_ts18048() {
    let source_k = "declare const k: number | undefined;\nconst _ = -k;\n";
    let source_result = "declare const result: number | undefined;\nconst _ = -result;\n";
    let codes_k = check_source_strict_codes(source_k);
    let codes_r = check_source_strict_codes(source_result);
    assert!(
        codes_k.contains(&18048),
        "unary - on `k: number | undefined` must emit TS18048; got: {codes_k:?}"
    );
    assert!(
        codes_r.contains(&18048),
        "unary - on `result: number | undefined` must emit TS18048; got: {codes_r:?}"
    );
}

// =========================================================================
// Union member order: null | number and undefined | number are equivalent
// =========================================================================

#[test]
fn unary_minus_null_union_reversed_order_emits_ts18047() {
    let codes = check_source_strict_codes("declare const c: null | number;\nconst _ = -c;\n");
    assert!(
        codes.contains(&18047),
        "unary - on `null | number` (reversed) must emit TS18047; got: {codes:?}"
    );
}

#[test]
fn unary_minus_undefined_union_reversed_order_emits_ts18048() {
    let codes = check_source_strict_codes("declare const c: undefined | number;\nconst _ = -c;\n");
    assert!(
        codes.contains(&18048),
        "unary - on `undefined | number` (reversed) must emit TS18048; got: {codes:?}"
    );
}

// =========================================================================
// Negative cases: no false positives
// =========================================================================

#[test]
fn unary_minus_on_plain_number_no_error() {
    let codes = check_source_strict_codes("declare const n: number;\nconst _ = -n;\n");
    assert!(
        !codes.contains(&18048) && !codes.contains(&18047),
        "unary - on plain `number` must not emit TS18048/18047; got: {codes:?}"
    );
}

#[test]
fn unary_minus_after_narrowing_no_error() {
    let source = "\
declare const a: number | undefined;\n\
if (a !== undefined) {\n\
    const _ = -a;\n\
}\n";
    let codes = check_source_strict_codes(source);
    assert!(
        !codes.contains(&18048),
        "unary - after narrowing away undefined must not emit TS18048; got: {codes:?}"
    );
}

#[test]
fn unary_tilde_after_null_narrowing_no_error() {
    let source = "\
declare const b: number | null;\n\
if (b !== null) {\n\
    const _ = ~b;\n\
}\n";
    let codes = check_source_strict_codes(source);
    assert!(
        !codes.contains(&18047),
        "unary ~ after narrowing away null must not emit TS18047; got: {codes:?}"
    );
}

// =========================================================================
// Control: binary arithmetic on nullish operands already emits TS18048
// =========================================================================

#[test]
fn binary_arithmetic_on_possibly_undefined_still_emits_ts18048() {
    let codes =
        check_source_strict_codes("declare const a: number | undefined;\nconst _ = a + 1;\n");
    assert!(
        codes.contains(&18048),
        "binary + on `number | undefined` must still emit TS18048; got: {codes:?}"
    );
}

// =========================================================================
// Without strictNullChecks: no nullish errors
// =========================================================================

#[test]
fn unary_minus_possibly_undefined_without_strict_no_error() {
    use crate::context::CheckerOptions;
    use crate::test_utils::check_with_options;

    let codes: Vec<u32> = check_with_options(
        "declare const a: number | undefined;\nconst _ = -a;\n",
        CheckerOptions {
            strict_null_checks: false,
            ..Default::default()
        },
    )
    .into_iter()
    .map(|d| d.code)
    .collect();

    assert!(
        !codes.contains(&18048),
        "without strictNullChecks, unary - on `number | undefined` must NOT emit TS18048; got: {codes:?}"
    );
}
