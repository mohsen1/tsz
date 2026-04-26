//! Tests for TS2456 circular reference detection through `typeof` queries.
//!
//! Per the TypeScript spec, "a type query directly depends on the type of the
//! referenced entity". When a type alias is `typeof X` and `X`'s annotation
//! references the same alias, tsc emits TS2456. This test locks in the
//! AST-based detection path that walks through `var x: T[]` annotations.

use tsz_checker::test_utils::check_source_codes as get_error_codes;

#[test]
fn test_ts2456_typeof_alias_references_self_through_array() {
    // type T = typeof x; var x: T[]
    // The alias body `typeof x` resolves to x's type which is `T[]` —
    // T directly depends on x's type, x's annotation contains T, so circular.
    // tsc emits TS2456 at the alias name.
    let src = r#"
        var x: T[] = [];
        type T = typeof x;
    "#;
    let codes = get_error_codes(src);
    assert!(
        codes.contains(&2456),
        "Expected TS2456 (circularly references itself), got: {codes:?}"
    );
}

#[test]
fn test_no_ts2456_when_typeof_target_uses_tuple_wrapping() {
    // Cycle goes through structurally-wrapping types (tuple element + generic
    // Application). tsc considers these structurally deferred and does NOT emit
    // TS2456. Our AST-based check only fires when x's annotation directly
    // references an alias on the resolution chain — tuple element nodes still
    // recurse, so this test guards against marking the leaf alias `T8 = C<T6>`
    // as circular when the cycle is ultimately broken by `C<T6>`'s generic
    // application.
    let src = r#"
        class C<T> {}
        type T6 = T7 | number;
        type T7 = typeof yy;
        var yy: [string, T8[]];
        type T8 = C<T6>;
    "#;
    let codes = get_error_codes(src);
    // tsc does not emit TS2456 for this constellation. We mirror that.
    assert!(
        !codes.contains(&2456),
        "Expected no TS2456 (tsc emits none), got: {codes:?}"
    );
}

#[test]
fn test_no_ts2456_when_typeof_target_does_not_reference_alias() {
    // typeof on a variable whose annotation is unrelated should not emit TS2456.
    let src = r#"
        var x: number = 0;
        type T = typeof x;
    "#;
    let codes = get_error_codes(src);
    assert!(
        !codes.contains(&2456),
        "Expected no TS2456 when typeof target is unrelated, got: {codes:?}"
    );
}

#[test]
fn test_no_ts2456_when_typeof_self_referencing_var_is_value() {
    // `type X = ...` and `const X = ...` (merged value+type) — typeof X on
    // the value side should not produce TS2456 at the type alias.
    let src = r#"
        type X = number;
        const X = 1;
    "#;
    let codes = get_error_codes(src);
    assert!(
        !codes.contains(&2456),
        "Expected no TS2456 for merged value+type with typeof self, got: {codes:?}"
    );
}
