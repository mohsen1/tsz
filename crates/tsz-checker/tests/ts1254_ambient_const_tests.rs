//! Tests for TS1254: ambient const initializer validation.
//! Boolean literals (true/false) should be accepted as valid ambient const initializers.

use tsz_checker::context::CheckerOptions;

fn get_codes(source: &str) -> Vec<u32> {
    tsz_checker::test_utils::check_source(source, "test.d.ts", CheckerOptions::default())
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn ts1254_not_emitted_for_true_literal() {
    let codes = get_codes("export declare const x = true;");
    assert!(
        !codes.contains(&1254),
        "TS1254 should NOT fire for `true` literal in ambient const, got: {codes:?}"
    );
}

#[test]
fn ts1254_not_emitted_for_false_literal() {
    let codes = get_codes("export declare const x = false;");
    assert!(
        !codes.contains(&1254),
        "TS1254 should NOT fire for `false` literal in ambient const, got: {codes:?}"
    );
}

#[test]
fn ts1254_not_emitted_for_string_literal() {
    let codes = get_codes(r#"export declare const x = "hello";"#);
    assert!(
        !codes.contains(&1254),
        "TS1254 should NOT fire for string literal in ambient const, got: {codes:?}"
    );
}

#[test]
fn ts1254_not_emitted_for_numeric_literal() {
    let codes = get_codes("export declare const x = 42;");
    assert!(
        !codes.contains(&1254),
        "TS1254 should NOT fire for numeric literal in ambient const, got: {codes:?}"
    );
}

// --- Simple-literal enum references (parity with tsc's `isSimpleLiteralEnumReference`) ---
//
// An ambient const initializer is a valid "literal enum reference" when it is a
// property access, or a string/numeric-literal element access, whose *object*
// is an entity-name expression AND whose resulting type is an enum-member
// (enum-literal) type. Binder names are varied so no identifier is load-bearing.

#[test]
fn ts1254_not_emitted_for_direct_enum_member_reference() {
    let codes = get_codes("enum Color { Red } export declare const x = Color.Red;");
    assert!(
        !codes.contains(&1254),
        "a direct enum-member reference is a valid literal enum reference, got: {codes:?}"
    );
}

#[test]
fn ts1254_not_emitted_for_nested_namespace_enum_member_reference() {
    // The object of the property access is a qualified name (`Palette.Shade`),
    // not a bare identifier. tsc accepts it; tsz previously rejected it with a
    // spurious TS1254 because it only resolved a plain-identifier object.
    let codes = get_codes(
        "declare namespace Palette { export enum Shade { Dark } } \
         export declare const y = Palette.Shade.Dark;",
    );
    assert!(
        !codes.contains(&1254),
        "a nested-namespace enum-member reference is valid, got: {codes:?}"
    );
}

#[test]
fn ts1254_not_emitted_for_string_literal_index_enum_reference() {
    let codes = get_codes(r#"enum Fruit { Apple } export declare const z = Fruit["Apple"];"#);
    assert!(
        !codes.contains(&1254),
        "a string-literal element access resolving to an enum member is valid, got: {codes:?}"
    );
}

#[test]
fn ts1254_not_emitted_for_const_enum_member_reference() {
    let codes =
        get_codes("declare const enum Weekday { Mon } export declare const w = Weekday.Mon;");
    assert!(
        !codes.contains(&1254),
        "a const-enum member reference is valid, got: {codes:?}"
    );
}

#[test]
fn ts1254_emitted_for_numeric_reverse_mapping_index() {
    // `Suit[0]` is a reverse mapping whose type is `string`, not an enum
    // literal, so tsc rejects it with TS1254 — tsz previously accepted it
    // because the check was purely syntactic (any numeric-literal index passed).
    let codes = get_codes("enum Suit { Hearts } export declare const q = Suit[0];");
    assert!(
        codes.contains(&1254),
        "a numeric reverse-mapping index is NOT a literal enum reference, got: {codes:?}"
    );
}

#[test]
fn ts1254_emitted_for_non_member_tail_off_enum_member() {
    // `Dir.North.toString` is a method reference, not an enum literal.
    let codes = get_codes("enum Dir { North } export declare const r = Dir.North.toString;");
    assert!(
        codes.contains(&1254),
        "a non-member tail off an enum member is not a literal enum reference, got: {codes:?}"
    );
}

#[test]
fn ts1254_emitted_for_non_enum_namespace_value_reference() {
    // A property access whose type is a plain value (here `number`), not an
    // enum literal, must still be rejected.
    let codes = get_codes(
        "declare namespace Cfg { export const flag: number; } \
         export declare const s = Cfg.flag;",
    );
    assert!(
        codes.contains(&1254),
        "a non-enum namespace value reference is not a literal enum reference, got: {codes:?}"
    );
}
