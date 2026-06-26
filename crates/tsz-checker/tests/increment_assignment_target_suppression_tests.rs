//! Tests for the increment/decrement (`++`/`--`) assignment-target validity ordering.
//!
//! Structural rule: when the operand of `++`/`--` is an invalid assignment target,
//! `tsc` evaluates it through `checkExpression`, which reports the const-variable
//! (TS2588) or readonly-*named*-property (TS2540) error and yields `errorType` for
//! that operand. The subsequent `checkArithmeticOperandType` then sees a wildcard and
//! is satisfied, so TS2356 ("An arithmetic operand must be of type 'any', 'number',
//! 'bigint' or an enum type") is *suppressed*. A readonly *index signature* (TS2542)
//! keeps the real element type, so it does NOT suppress TS2356 (tsc emits both).
//!
//! Separately, the nullish "possibly undefined/null" diagnostic does not suppress the
//! reference-expression check (TS2357 / TS2777): a nullable-but-numeric operand that is
//! not a valid reference still reports the lvalue/optional-chain error.
//!
//! Binder names are varied across cases so no fix can key off a specific identifier.

use crate::test_utils::check_source_strict_codes;

// =========================================================================
// const variable + non-arithmetic operand → TS2588 only (TS2356 suppressed)
// =========================================================================

#[test]
fn const_symbol_postfix_increment_emits_only_ts2588() {
    let codes = check_source_strict_codes("const sym: symbol = Symbol();\nsym++;\n");
    assert!(
        codes.contains(&2588),
        "`const sym: symbol; sym++` must emit TS2588; got: {codes:?}"
    );
    assert!(
        !codes.contains(&2356),
        "const operand error must suppress TS2356; got: {codes:?}"
    );
}

#[test]
fn const_string_postfix_increment_emits_only_ts2588() {
    let codes = check_source_strict_codes("const label: string = \"a\";\nlabel--;\n");
    assert!(codes.contains(&2588), "expected TS2588; got: {codes:?}");
    assert!(
        !codes.contains(&2356),
        "expected TS2356 suppressed; got: {codes:?}"
    );
}

#[test]
fn const_boolean_prefix_increment_emits_only_ts2588() {
    let codes = check_source_strict_codes("const flag: boolean = true;\n++flag;\n");
    assert!(codes.contains(&2588), "expected TS2588; got: {codes:?}");
    assert!(
        !codes.contains(&2356),
        "expected TS2356 suppressed; got: {codes:?}"
    );
}

#[test]
fn const_symbol_prefix_decrement_emits_only_ts2588() {
    let codes = check_source_strict_codes("const token: symbol = Symbol();\n--token;\n");
    assert!(codes.contains(&2588), "expected TS2588; got: {codes:?}");
    assert!(
        !codes.contains(&2356),
        "expected TS2356 suppressed; got: {codes:?}"
    );
}

// =========================================================================
// readonly NAMED property + non-arithmetic operand → TS2540 only
// =========================================================================

#[test]
fn readonly_symbol_property_postfix_increment_emits_only_ts2540() {
    let codes =
        check_source_strict_codes("declare const box: { readonly key: symbol };\nbox.key++;\n");
    assert!(
        codes.contains(&2540),
        "readonly named property `++` must emit TS2540; got: {codes:?}"
    );
    assert!(
        !codes.contains(&2356),
        "readonly named-property error must suppress TS2356; got: {codes:?}"
    );
}

#[test]
fn readonly_string_property_prefix_increment_emits_only_ts2540() {
    let codes =
        check_source_strict_codes("declare const rec: { readonly name: string };\n++rec.name;\n");
    assert!(codes.contains(&2540), "expected TS2540; got: {codes:?}");
    assert!(
        !codes.contains(&2356),
        "expected TS2356 suppressed; got: {codes:?}"
    );
}

#[test]
fn readonly_class_field_increment_via_this_emits_only_ts2540() {
    let codes = check_source_strict_codes(
        "class Widget {\n  readonly handle: symbol = Symbol();\n  bump() { this.handle++; }\n}\n",
    );
    assert!(codes.contains(&2540), "expected TS2540; got: {codes:?}");
    assert!(
        !codes.contains(&2356),
        "expected TS2356 suppressed; got: {codes:?}"
    );
}

// =========================================================================
// Preserved behavior (regression guards)
// =========================================================================

#[test]
fn const_number_increment_still_emits_ts2588() {
    // A valid arithmetic operand that is const: TS2588 with no TS2356.
    let codes = check_source_strict_codes("const count: number = 1;\ncount++;\n");
    assert!(codes.contains(&2588), "expected TS2588; got: {codes:?}");
    assert!(
        !codes.contains(&2356),
        "valid arithmetic operand has no TS2356; got: {codes:?}"
    );
}

#[test]
fn mutable_symbol_increment_still_emits_ts2356_without_ts2588() {
    // A mutable (non-const) non-arithmetic operand: TS2356, no TS2588.
    let codes = check_source_strict_codes("let marker: symbol = Symbol();\nmarker++;\n");
    assert!(
        codes.contains(&2356),
        "mutable symbol `++` must still emit TS2356; got: {codes:?}"
    );
    assert!(
        !codes.contains(&2588),
        "a mutable operand is not const, so no TS2588; got: {codes:?}"
    );
}

#[test]
fn readonly_index_signature_non_arithmetic_emits_both_ts2356_and_ts2542() {
    // A readonly *index signature* keeps the real element type, so tsc emits BOTH
    // TS2356 (non-arithmetic element) and TS2542 (index only permits reading).
    let codes = check_source_strict_codes(
        "declare const map: { readonly [k: string]: symbol };\nmap[\"a\"]++;\n",
    );
    assert!(
        codes.contains(&2542),
        "readonly index write must emit TS2542; got: {codes:?}"
    );
    assert!(
        codes.contains(&2356),
        "readonly index signature must NOT suppress TS2356; got: {codes:?}"
    );
}

// =========================================================================
// Nullish does not suppress the reference-expression check (TS2357 / TS2777)
// =========================================================================

#[test]
fn optional_chain_increment_emits_ts2777() {
    // `o?.k++` is a possibly-undefined numeric operand AND an optional-chain target:
    // tsc reports the nullish diagnostic AND TS2777, which the nullish error must not
    // suppress.
    let codes =
        check_source_strict_codes("declare const cfg: { value?: number };\ncfg?.value++;\n");
    assert!(
        codes.contains(&2777),
        "optional-chain `++` must emit TS2777 even with a nullish operand; got: {codes:?}"
    );
}
