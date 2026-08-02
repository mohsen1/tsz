//! A parameter binding pattern declares value bindings that stay in scope for
//! the *type* positions of the same signature.
//!
//! Because a signature has no body, the only way to reference one is a `typeof`
//! type query in the signature's own return type. `tsc` resolves such a query
//! to the binding, which has two consequences tsz used to get wrong:
//!
//! * the query must not fall through to global name resolution (`TS2304`);
//! * `TS2842` ("is an unused renaming of") describes an *unused* renaming, so a
//!   `typeof` reference to the renamed binding must silence it.
//!
//! Oracled against `tsc` 7.0.2 with `--strict false --target es2015`.

use tsz_checker::test_utils::check_source_non_strict_codes;

const TS2304_CANNOT_FIND_NAME: u32 = 2304;
const TS2693_TYPE_ONLY_VALUE: u32 = 2693;
const TS2842_UNUSED_RENAMING: u32 = 2842;

// ---------------------------------------------------------------------------
// TS2842 fires only when the renaming is unused
// ---------------------------------------------------------------------------

#[test]
fn unreferenced_rename_in_function_type_still_reports_ts2842() {
    let codes = check_source_non_strict_codes(
        "type O = { a?: string; b: number };\ntype F = ({ a: renamed }: O) => void;",
    );
    assert!(
        codes.contains(&TS2842_UNUSED_RENAMING),
        "an unreferenced renaming is still TS2842; got {codes:?}"
    );
}

#[test]
fn rename_referenced_by_return_type_query_reports_nothing() {
    let codes = check_source_non_strict_codes(
        "type O = { a?: string; b: number };\ntype F = ({ a: renamed }: O) => typeof renamed;",
    );
    assert!(
        codes.is_empty(),
        "`typeof renamed` uses the renaming, so neither TS2842 nor TS2304 may fire; got {codes:?}"
    );
}

#[test]
fn rename_referenced_from_a_constructor_type_reports_nothing() {
    let codes = check_source_non_strict_codes(
        "type O = { a?: string; b: number };\ntype G = new ({ a: renamed }: O) => typeof renamed;",
    );
    assert!(
        codes.is_empty(),
        "a construct signature scopes its bindings the same way; got {codes:?}"
    );
}

#[test]
fn rename_referenced_from_a_bodyless_function_declaration_reports_nothing() {
    let codes = check_source_non_strict_codes(
        "type O = { a?: string; b: number };\ndeclare function f({ a: renamed }: O): typeof renamed;",
    );
    assert!(
        codes.is_empty(),
        "a bodyless declaration scopes its bindings the same way; got {codes:?}"
    );
}

#[test]
fn a_nested_rename_referenced_by_a_type_query_reports_nothing() {
    let codes = check_source_non_strict_codes(
        "type F = ({ outer: { inner: renamed } }: any) => typeof renamed;",
    );
    assert!(
        codes.is_empty(),
        "the binding is declared by a nested pattern but scoped to the same signature; got {codes:?}"
    );
}

#[test]
fn only_the_referenced_rename_of_a_pair_is_exempt() {
    // `used` is referenced, `unused` is not: exactly one TS2842.
    let codes = check_source_non_strict_codes("type F = ({ a: unused, b: used }) => typeof used;");
    assert_eq!(
        codes
            .iter()
            .filter(|&&c| c == TS2842_UNUSED_RENAMING)
            .count(),
        1,
        "the used renaming is exempt and the unused one is not; got {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// The binding resolves, so no TS2304
// ---------------------------------------------------------------------------

#[test]
fn shorthand_binding_referenced_by_return_type_query_is_not_unresolved() {
    let codes = check_source_non_strict_codes(
        "type O = { a?: string; b: number };\ntype F = ({ a }: O) => typeof a;",
    );
    assert!(
        !codes.contains(&TS2304_CANNOT_FIND_NAME),
        "`{{ a }}` declares `a`, so `typeof a` resolves; got {codes:?}"
    );
}

#[test]
fn array_pattern_binding_referenced_by_return_type_query_is_not_unresolved() {
    let codes = check_source_non_strict_codes(
        "type F = ([first, second]: [number, string]) => typeof first;",
    );
    assert!(
        !codes.contains(&TS2304_CANNOT_FIND_NAME),
        "an array pattern declares its elements too; got {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Negative cases: the scope must not over-reach
// ---------------------------------------------------------------------------

#[test]
fn a_name_no_parameter_declares_is_still_unresolved() {
    let codes = check_source_non_strict_codes(
        "type O = { a?: string; b: number };\ntype F = ({ a: renamed }: O) => typeof missing;",
    );
    assert!(
        codes.contains(&TS2304_CANNOT_FIND_NAME),
        "`missing` is declared by nothing and must still report TS2304; got {codes:?}"
    );
}

#[test]
fn a_sibling_signatures_binding_does_not_leak() {
    let codes = check_source_non_strict_codes(
        "type A = ({ a: mine }: any) => typeof mine;\ntype B = (x: number) => typeof mine;",
    );
    assert!(
        codes.contains(&TS2304_CANNOT_FIND_NAME),
        "`mine` belongs to A's signature and must not resolve inside B; got {codes:?}"
    );
}

#[test]
fn an_inner_signatures_binding_does_not_leak_outward() {
    // `inner` is declared by the parameter of the *nested* function type, so it
    // is out of scope for the outer signature's own return type.
    let codes = check_source_non_strict_codes(
        "type F = (cb: ({ a: inner }: any) => void) => typeof inner;",
    );
    assert!(
        codes.contains(&TS2304_CANNOT_FIND_NAME),
        "an inner signature's binding is not in the outer signature's scope; got {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// The binding can shadow a primitive-type keyword: no TS2693 (issue #16242)
//
// `error_cannot_find_name_at` has an early-return branch for names that spell
// a primitive type keyword (`string`, `number`, ...) that reported TS2693
// before ever reaching the `signature_parameter_declares_binding` guard the
// TS2304 path already had — so a query that resolved fine for `typeof mine`
// still reported TS2693 for the byte-identical shape `typeof string`.
// ---------------------------------------------------------------------------

#[test]
fn primitive_named_rename_referenced_by_return_type_query_reports_nothing() {
    let codes = check_source_non_strict_codes("type F = ({ a: string }) => typeof string;");
    assert!(
        codes.is_empty(),
        "`string` is a real binding here, not the primitive keyword; got {codes:?}"
    );
}

#[test]
fn primitive_named_rename_referenced_reports_only_ts2842_for_the_other_unused_one() {
    let codes =
        check_source_non_strict_codes("type F = ({ a: string, b: number }) => typeof number;");
    assert_eq!(
        codes,
        vec![TS2842_UNUSED_RENAMING],
        "`number` is referenced (no TS2693/TS2842 for it); `string` is unused (TS2842 only, no TS2693); got {codes:?}"
    );
}

#[test]
fn primitive_named_rename_in_a_method_signature_reports_nothing() {
    let codes = check_source_non_strict_codes("interface I { m({ a: string }): typeof string; }");
    assert!(
        codes.is_empty(),
        "a method signature scopes its bindings the same way a function type does; got {codes:?}"
    );
}

#[test]
fn primitive_keyword_as_a_plain_value_still_reports_ts2693() {
    // Negative control: no signature binding is in play at all.
    let codes = check_source_non_strict_codes("let x = string;");
    assert!(
        codes.contains(&TS2693_TYPE_ONLY_VALUE),
        "a bare value use of the `string` keyword must still report TS2693; got {codes:?}"
    );
}

#[test]
fn primitive_named_binding_does_not_leak_into_a_sibling_signature() {
    let codes = check_source_non_strict_codes(
        "type A = ({ a: string }: any) => typeof string;\ntype B = (x: number) => typeof string;",
    );
    assert!(
        codes.contains(&TS2693_TYPE_ONLY_VALUE),
        "`string` belongs to A's signature and must not resolve inside B; got {codes:?}"
    );
}
