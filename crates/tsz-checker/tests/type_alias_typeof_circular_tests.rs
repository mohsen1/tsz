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

#[test]
fn test_no_ts2456_when_typeof_target_references_alias_inside_type_literal() {
    // Repro from `unionTypeWithRecursiveSubtypeReduction3.ts`:
    //
    //   declare var a27: { prop: number } | { prop: T27 };
    //   type T27 = typeof a27;
    //
    // The reference to `T27` is wrapped inside a TYPE_LITERAL property type
    // (`{ prop: T27 }`). tsc treats TYPE_LITERAL property types as lazily
    // resolved during typeof-target type construction, so the cycle is
    // structurally deferred and does NOT trigger TS2456.
    let src = r#"
        declare var a27: { prop: number } | { prop: T27 };
        type T27 = typeof a27;
    "#;
    let codes = get_error_codes(src);
    assert!(
        !codes.contains(&2456),
        "Expected no TS2456 when alias reference is inside a TYPE_LITERAL property, got: {codes:?}"
    );
}

#[test]
fn test_no_ts2456_when_typeof_target_references_alias_inside_function_type() {
    // `() => T` is structurally deferred — tsc does not eagerly compute
    // signature types when constructing the variable's typeof target.
    let src = r#"
        declare var f: () => T;
        type T = typeof f;
    "#;
    let codes = get_error_codes(src);
    assert!(
        !codes.contains(&2456),
        "Expected no TS2456 when alias reference is inside a FUNCTION_TYPE, got: {codes:?}"
    );
}

#[test]
fn test_no_ts2456_when_typeof_target_references_alias_inside_constructor_type() {
    // `new () => T` is structurally deferred for the same reason as
    // FUNCTION_TYPE.
    let src = r#"
        declare var c: new () => T;
        type T = typeof c;
    "#;
    let codes = get_error_codes(src);
    assert!(
        !codes.contains(&2456),
        "Expected no TS2456 when alias reference is inside a CONSTRUCTOR_TYPE, got: {codes:?}"
    );
}
