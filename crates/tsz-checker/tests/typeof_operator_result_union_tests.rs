//! Tests for the result type of the runtime `typeof` operator.
//!
//! Structural rule: a `typeof expr` expression (the prefix unary operator, in
//! value position) has the well-known string-literal union type
//! `"string" | "number" | "bigint" | "boolean" | "symbol" | "undefined" | "object" | "function"`,
//! not plain `string`. This is independent of the operand's type. As a result,
//! assigning it to that full union is allowed, but assigning it to a single
//! member like `"string"` must emit TS2322.
//!
//! Fixes issue #9733.

use crate::test_utils::{check_source_codes, check_source_strict_messages};

const FULL_UNION: &str =
    "\"string\"|\"number\"|\"bigint\"|\"boolean\"|\"symbol\"|\"undefined\"|\"object\"|\"function\"";

// =========================================================================
// Reported repro
// =========================================================================

#[test]
fn typeof_result_assignable_to_full_well_known_union() {
    let src =
        format!("declare const o: unknown;\nconst t = typeof o;\nconst x: {FULL_UNION} = t;\n");
    let codes = check_source_codes(&src);
    assert!(
        !codes.contains(&2322),
        "typeof result must be assignable to its well-known union; got: {codes:?}"
    );
}

#[test]
fn typeof_result_not_assignable_to_single_string_member() {
    let src = "declare const o: unknown;\nconst t = typeof o;\nconst y: \"string\" = t;\n";
    let messages = check_source_strict_messages(src);
    let ts2322: Vec<&str> = messages
        .iter()
        .filter(|(code, _)| *code == 2322)
        .map(|(_, m)| m.as_str())
        .collect();
    assert_eq!(
        ts2322.len(),
        1,
        "exactly one TS2322 expected when assigning typeof result to a single member; got: {messages:?}"
    );
    // The reported source type must render as the literal union, never plain `string`.
    let msg = ts2322[0];
    assert!(
        msg.contains("\"string\"") && msg.contains("\"function\""),
        "TS2322 message should display the typeof literal union, got: {msg}"
    );
}

#[test]
fn typeof_result_assigned_to_non_string_target_reports_string_source() {
    let src = "declare const o: unknown;\nconst z: number = typeof o;\n";
    let messages = check_source_strict_messages(src);
    let ts2322: Vec<&str> = messages
        .iter()
        .filter(|(code, _)| *code == 2322)
        .map(|(_, m)| m.as_str())
        .collect();
    assert_eq!(
        ts2322.len(),
        1,
        "exactly one TS2322 expected when assigning typeof result to number; got: {messages:?}"
    );
    let msg = ts2322[0];
    assert!(
        msg.contains("Type 'string' is not assignable to type 'number'"),
        "non-string targets should display typeof result as `string`, got: {msg}"
    );
    assert!(
        !msg.contains("\"function\""),
        "non-string target diagnostics should not display the typeof literal union, got: {msg}"
    );
}

// =========================================================================
// Independent of operand shape (rule is about the operator, not the operand)
// =========================================================================

#[test]
fn typeof_result_union_regardless_of_operand_type() {
    // number operand, object operand, string-literal operand, function operand.
    let src = format!(
        "declare const n: number;\n\
         declare const obj: {{ a: 1 }};\n\
         declare const fn: () => void;\n\
         const a: {FULL_UNION} = typeof n;\n\
         const b: {FULL_UNION} = typeof obj;\n\
         const c: {FULL_UNION} = typeof \"hi\";\n\
         const d: {FULL_UNION} = typeof fn;\n"
    );
    let codes = check_source_codes(&src);
    assert!(
        !codes.contains(&2322),
        "typeof result must be the union for any operand shape; got: {codes:?}"
    );
}

#[test]
fn typeof_result_single_member_rejected_for_any_operand() {
    let src = "declare const obj: { a: 1 };\nconst b: \"object\" = typeof obj;\n";
    let codes = check_source_codes(src);
    assert!(
        codes.contains(&2322),
        "typeof result is the full union, not the narrowed literal, so a single-member \
         target must error even when the operand is an object; got: {codes:?}"
    );
}

// =========================================================================
// Negative controls: existing behavior must be preserved
// =========================================================================

#[test]
fn typeof_narrowing_still_works() {
    // Result-type change must not disturb `typeof` type guards.
    let src = "function f(x: string | number) {\n\
               if (typeof x === \"string\") { x.toUpperCase(); }\n\
               else { x.toFixed(2); }\n\
               }\n";
    let codes = check_source_codes(src);
    assert!(
        !codes.contains(&2322) && !codes.contains(&2339) && !codes.contains(&2345),
        "typeof narrowing must remain sound; got: {codes:?}"
    );
}

#[test]
fn typeof_embedded_in_template_is_string() {
    // `${typeof x}` is still a string-coercible spot; assigning to string is fine.
    let src = "declare const z: unknown;\nconst s: string = `kind:${typeof z}`;\n";
    let codes = check_source_codes(src);
    assert!(
        !codes.contains(&2322),
        "template embedding of typeof result must stay assignable to string; got: {codes:?}"
    );
}

#[test]
fn typeof_comparison_with_non_member_still_reports_ts2367() {
    // The overlap check shares the same canonical union; capital-O "Object" is
    // not a possible typeof result, so the comparison has no overlap.
    let src = "declare const x: unknown;\nconst bad = typeof x === \"Object\";\n";
    let codes = check_source_codes(src);
    assert!(
        codes.contains(&2367),
        "comparison against a non-typeof string must still emit TS2367; got: {codes:?}"
    );
}

#[test]
fn typeof_comparison_with_valid_member_is_clean() {
    let src = "declare const x: unknown;\nconst good = typeof x === \"object\";\n";
    let codes = check_source_codes(src);
    assert!(
        !codes.contains(&2367),
        "comparison against a valid typeof result must not emit TS2367; got: {codes:?}"
    );
}
