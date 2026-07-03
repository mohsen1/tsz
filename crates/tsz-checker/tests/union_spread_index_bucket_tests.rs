//! Union object-spread index bucket parity.

use tsz_checker::test_utils::{
    HasDiagnosticCode, check_source_diagnostics, diagnostic_count, diagnostics_where,
};

fn missing_index_diagnostic_count<T: HasDiagnosticCode>(diagnostics: &[T]) -> usize {
    diagnostics_where(diagnostics, |code| matches!(code, 2538 | 7053)).len()
}

#[test]
fn union_object_spread_preserves_symbol_index_signature_when_all_arms_have_one() {
    let source = r#"
declare const sym: symbol;
type Left = { kind: "left"; [key: symbol]: boolean };
type Right = { kind: "right"; [key: symbol]: boolean };
declare const source: Left | Right;

const spread = { ...source };
const keyed: boolean = spread[sym];
const key: keyof typeof spread = sym;
"#;

    let diagnostics = check_source_diagnostics(source);
    let relevant = diagnostics_where(&diagnostics, |code| matches!(code, 2322 | 2538 | 7053));
    assert!(
        relevant.is_empty(),
        "Expected union object spread to preserve symbol index signatures present in every arm, got {diagnostics:?}"
    );
}

#[test]
fn union_object_spread_keeps_string_and_symbol_indexes_separate() {
    let source = r#"
declare const sym: symbol;
type Left = { tag: number; [key: string]: number; [key: symbol]: boolean };
type Right = { other: number; [key: string]: number; [key: symbol]: boolean };
declare const source: Left | Right;

const spread = { ...source };
const named: number = spread["name"];
const keyed: boolean = spread[sym];
const wrongString: boolean = spread["name"];
const wrongSymbol: number = spread[sym];
"#;

    let diagnostics = check_source_diagnostics(source);
    assert_eq!(
        missing_index_diagnostic_count(&diagnostics),
        0,
        "Expected union object spread to keep string and symbol index signatures separate, got {diagnostics:?}"
    );
    assert_eq!(
        diagnostic_count(&diagnostics, 2322),
        2,
        "Expected exactly two TS2322s for swapped string/symbol union-spread assignments, got {diagnostics:?}"
    );
}

#[test]
fn union_object_spread_preserves_number_index_signature_when_all_arms_have_one() {
    let source = r#"
type Left = { kind: "left"; [key: number]: string };
type Right = { kind: "right"; [key: number]: string };
declare const source: Left | Right;

const spread = { ...source };
const keyed: string = spread[0];
const key: keyof typeof spread = 0;
"#;

    let diagnostics = check_source_diagnostics(source);
    let relevant = diagnostics_where(&diagnostics, |code| matches!(code, 2322 | 2538 | 7053));
    assert!(
        relevant.is_empty(),
        "Expected union object spread to preserve number index signatures present in every arm, got {diagnostics:?}"
    );
}

#[test]
fn union_object_spread_rejects_symbol_index_when_one_arm_lacks_it() {
    let source = r#"
declare const sym: symbol;
type Left = { kind: "left"; [key: symbol]: boolean };
type Right = { kind: "right"; value: number };
declare const source: Left | Right;

const spread = { ...source };
const keyed: boolean = spread[sym];
const key: keyof typeof spread = sym;
"#;

    let diagnostics = check_source_diagnostics(source);
    assert!(
        missing_index_diagnostic_count(&diagnostics) >= 1,
        "Expected union object spread to reject symbol indexing when one arm lacks the symbol index, got {diagnostics:?}"
    );
    assert!(
        diagnostic_count(&diagnostics, 2322) >= 1,
        "Expected keyof assignment to reject `symbol` when one union arm lacks the symbol index, got {diagnostics:?}"
    );
}

#[test]
fn union_object_spread_rejects_number_index_when_one_arm_lacks_it() {
    let source = r#"
type Left = { kind: "left"; [key: number]: string };
type Right = { kind: "right"; value: boolean };
declare const source: Left | Right;

const spread = { ...source };
const keyed: string = spread[0];
const key: keyof typeof spread = 0;
"#;

    let diagnostics = check_source_diagnostics(source);
    assert!(
        missing_index_diagnostic_count(&diagnostics) >= 1,
        "Expected union object spread to reject number indexing when one arm lacks the number index, got {diagnostics:?}"
    );
    assert!(
        diagnostic_count(&diagnostics, 2322) >= 1,
        "Expected keyof assignment to reject `number` when one union arm lacks the number index, got {diagnostics:?}"
    );
}

#[test]
fn union_object_spread_with_explicit_property_drops_symbol_index_signature() {
    let source = r#"
declare const sym: symbol;
type Left = { kind: "left"; [key: symbol]: boolean };
type Right = { kind: "right"; [key: symbol]: boolean };
declare const source: Left | Right;

const spread = { ...source, own: 1 };
const keyed: boolean = spread[sym];
const key: keyof typeof spread = sym;
"#;

    let diagnostics = check_source_diagnostics(source);
    assert!(
        missing_index_diagnostic_count(&diagnostics) >= 1,
        "Expected explicit property after union spread to drop the symbol index signature, got {diagnostics:?}"
    );
    assert!(
        diagnostic_count(&diagnostics, 2322) >= 1,
        "Expected keyof assignment to reject `symbol` after explicit property drops union-spread symbol indexes, got {diagnostics:?}"
    );
}

#[test]
fn union_object_spread_unions_symbol_index_value_types() {
    let source = r#"
declare const sym: symbol;
type Left = { kind: "left"; [key: symbol]: boolean };
type Right = { kind: "right"; [key: symbol]: number };
declare const source: Left | Right;

const spread = { ...source };
const keyed: boolean | number = spread[sym];
const key: keyof typeof spread = sym;
const wrong: boolean = spread[sym];
"#;

    let diagnostics = check_source_diagnostics(source);
    assert_eq!(
        missing_index_diagnostic_count(&diagnostics),
        0,
        "Expected union object spread to preserve symbol index access across differing value types, got {diagnostics:?}"
    );
    assert_eq!(
        diagnostic_count(&diagnostics, 2322),
        1,
        "Expected exactly one TS2322 for assigning boolean | number to boolean, got {diagnostics:?}"
    );
}

#[test]
fn union_object_spread_symbol_bucket_survives_when_string_bucket_is_missing() {
    let source = r#"
declare const sym: symbol;
type Left = { kind: "left"; [key: string]: number | "left" | "right"; [key: symbol]: boolean };
type Right = { kind: "right"; [key: symbol]: boolean };
declare const source: Left | Right;

const spread = { ...source };
const symbolValue: boolean = spread[sym];
const symbolKey: keyof typeof spread = sym;
const missingStringValue = spread["name"];
const missingStringKey: keyof typeof spread = "name";
"#;

    let diagnostics = check_source_diagnostics(source);
    let symbol_diagnostics = diagnostics_where(&diagnostics, |code| matches!(code, 2538));
    assert!(
        symbol_diagnostics.is_empty(),
        "Expected symbol bucket to survive even when string bucket is missing from one union arm, got {diagnostics:?}"
    );
    assert!(
        !diagnostics_where(&diagnostics, |code| matches!(code, 7053)).is_empty(),
        "Expected string indexing to fail when one union arm lacks the string bucket, got {diagnostics:?}"
    );
    assert!(
        diagnostic_count(&diagnostics, 2322) >= 1,
        "Expected string keyof assignment to fail when one union arm lacks the string bucket, got {diagnostics:?}"
    );
}

#[test]
fn union_object_spread_string_bucket_survives_when_symbol_bucket_is_missing() {
    let source = r#"
declare const sym: symbol;
type Left = { kind: "left"; [key: string]: number | "left" | "right"; [key: symbol]: boolean };
type Right = { kind: "right"; [key: string]: number | "left" | "right" };
declare const source: Left | Right;

const spread = { ...source };
const stringValue: number | "left" | "right" = spread["name"];
const stringKey: keyof typeof spread = "anything";
const missingSymbolValue: boolean = spread[sym];
const missingSymbolKey: keyof typeof spread = sym;
"#;

    let diagnostics = check_source_diagnostics(source);
    assert!(
        missing_index_diagnostic_count(&diagnostics) >= 1,
        "Expected symbol indexing to fail when one union arm lacks the symbol bucket, got {diagnostics:?}"
    );
    assert!(
        diagnostic_count(&diagnostics, 2322) >= 1,
        "Expected symbol keyof assignment to fail when one union arm lacks the symbol bucket, got {diagnostics:?}"
    );
}

#[test]
fn union_spread_followed_by_indexed_spread_drops_spread_indexes() {
    let source = r#"
declare const sym: symbol;
type Left = { kind: "left"; [key: symbol]: boolean };
type Right = { kind: "right"; [key: symbol]: boolean };
declare const unionSource: Left | Right;
declare const numericSource: { [key: number]: string };

const spread = { ...unionSource, ...numericSource };
const symbolValue: boolean = spread[sym];
const numberValue: string = spread[0];
const symbolKey: keyof typeof spread = sym;
const numberKey: keyof typeof spread = 0;
"#;

    let diagnostics = check_source_diagnostics(source);
    assert!(
        missing_index_diagnostic_count(&diagnostics) >= 2,
        "Expected second spread to drop spread index signatures, got {diagnostics:?}"
    );
    assert!(
        diagnostic_count(&diagnostics, 2322) >= 2,
        "Expected keyof assignments to reject dropped spread indexes, got {diagnostics:?}"
    );
}

#[test]
fn indexed_spread_followed_by_union_spread_drops_spread_indexes() {
    let source = r#"
declare const sym: symbol;
type Left = { kind: "left"; [key: symbol]: boolean };
type Right = { kind: "right"; [key: symbol]: boolean };
declare const unionSource: Left | Right;
declare const numericSource: { [key: number]: string };

const spread = { ...numericSource, ...unionSource };
const symbolValue: boolean = spread[sym];
const numberValue: string = spread[0];
const symbolKey: keyof typeof spread = sym;
const numberKey: keyof typeof spread = 0;
"#;

    let diagnostics = check_source_diagnostics(source);
    assert!(
        missing_index_diagnostic_count(&diagnostics) >= 2,
        "Expected first spread index signatures to be dropped after a second spread, got {diagnostics:?}"
    );
    assert!(
        diagnostic_count(&diagnostics, 2322) >= 2,
        "Expected keyof assignments to reject dropped spread indexes, got {diagnostics:?}"
    );
}

#[test]
fn two_union_spreads_drop_spread_indexes() {
    let source = r#"
declare const sym: symbol;
type Left = { kind: "left"; [key: symbol]: boolean };
type Right = { kind: "right"; [key: symbol]: boolean };
type NumericLeft = { a: "a"; [key: number]: string };
type NumericRight = { b: "b"; [key: number]: string };
declare const symbolUnion: Left | Right;
declare const numberUnion: NumericLeft | NumericRight;

const spread = { ...symbolUnion, ...numberUnion };
const symbolValue: boolean = spread[sym];
const numberValue: string = spread[0];
const symbolKey: keyof typeof spread = sym;
const numberKey: keyof typeof spread = 0;
"#;

    let diagnostics = check_source_diagnostics(source);
    assert!(
        missing_index_diagnostic_count(&diagnostics) >= 2,
        "Expected multiple union spreads to drop spread index signatures, got {diagnostics:?}"
    );
    assert!(
        diagnostic_count(&diagnostics, 2322) >= 2,
        "Expected keyof assignments to reject dropped spread indexes from multiple union spreads, got {diagnostics:?}"
    );
}
