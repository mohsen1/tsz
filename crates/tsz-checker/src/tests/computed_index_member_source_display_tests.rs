//! Source-spelled display for a computed-name method/accessor whose wide
//! (non-literal) key folds into an index-signature bucket.
//!
//! Structural rule, oracled against `typescript@7.0.2` (`--strict`): a fresh
//! object literal whose member is a method, getter, or setter under a wide
//! `string`/`number`/`symbol` computed key displays that member
//! property-style from its own source spelling — `{ [ws]: () => number; }`
//! for a method, the return type for a getter, the parameter type for a
//! setter — exactly as the property-assignment form of the same key already
//! does. The generic `[x: kind]: V` clause appears only under the #16721
//! fold rule: at least one wide key in the homogeneous group is not an
//! entity-name reference.
//!
//! tsz rendered every method/accessor-bearing literal through the structural
//! formatter, whose synthesized index signature carries no source name, so
//! the message showed `{ [x: string]: () => number; }` (#16662). The member
//! type is captured at object-literal computation time
//! (`computed_index_member_display_types`) and re-spelled by
//! `computed_index_member_source_display`; function inference never re-runs
//! at display time.
//!
//! Binder names vary across cases so no identifier string is load-bearing.

use crate::test_utils::check_source_strict_messages;

fn message_for(source: &str, code: u32) -> Option<String> {
    check_source_strict_messages(source)
        .into_iter()
        .find(|(c, _)| *c == code)
        .map(|(_, message)| message)
}

// ---------------------------------------------------------------------------
// Entity-name wide keys re-spell the member, property-style.
// ---------------------------------------------------------------------------

#[test]
fn a_wide_string_keyed_method_displays_its_source_key_and_function_type() {
    let source = r#"
declare const routeName: string;
interface StrTable { [slot: string]: string }
const table: StrTable = { [routeName]() { return 1; } };
"#;
    let message = message_for(source, 2322).expect("TS2322 for the index mismatch");
    assert!(
        message.contains("{ [routeName]: () => number; }"),
        "method should display property-style from its source key: {message}"
    );
}

#[test]
fn a_wide_string_keyed_getter_displays_its_return_type() {
    let source = r#"
declare const fieldKey: string;
interface StrTable { [slot: string]: string }
const table: StrTable = { get [fieldKey]() { return 5; } };
"#;
    let message = message_for(source, 2322).expect("TS2322 for the index mismatch");
    assert!(
        message.contains("{ [fieldKey]: number; }"),
        "getter should display by its return type: {message}"
    );
}

#[test]
fn a_wide_string_keyed_setter_displays_its_parameter_type() {
    let source = r#"
declare const sinkKey: string;
interface StrTable { [slot: string]: string }
const table: StrTable = { set [sinkKey](next: number) {} };
"#;
    let message = message_for(source, 2322).expect("TS2322 for the index mismatch");
    assert!(
        message.contains("{ [sinkKey]: number; }"),
        "setter should display by its parameter type: {message}"
    );
}

#[test]
fn a_wide_number_keyed_method_displays_its_source_key() {
    let source = r#"
declare const rowIndex: number;
interface NumTable { [slot: number]: string }
const table: NumTable = { [rowIndex]() { return 2; } };
"#;
    let message = message_for(source, 2322).expect("TS2322 for the index mismatch");
    assert!(
        message.contains("{ [rowIndex]: () => number; }"),
        "number-keyed method should display its source key: {message}"
    );
}

#[test]
fn a_wide_symbol_keyed_method_displays_its_source_key() {
    let source = r#"
declare const tagSym: symbol;
interface SymTable { [slot: symbol]: string }
const table: SymTable = { [tagSym]() { return 3; } };
"#;
    let message = message_for(source, 2322).expect("TS2322 for the index mismatch");
    assert!(
        message.contains("{ [tagSym]: () => number; }"),
        "symbol-keyed method should display its source key: {message}"
    );
}

#[test]
fn a_dotted_entity_name_keyed_method_keeps_the_dotted_spelling() {
    let source = r#"
declare const box: { label: string };
interface StrTable { [slot: string]: string }
const table: StrTable = { [box.label]() { return 4; } };
"#;
    let message = message_for(source, 2322).expect("TS2322 for the index mismatch");
    assert!(
        message.contains("{ [box.label]: () => number; }"),
        "dotted entity key should keep its spelling: {message}"
    );
}

#[test]
fn a_method_with_parameters_displays_its_full_function_type() {
    let source = r#"
declare const opKey: string;
interface StrTable { [slot: string]: string }
const table: StrTable = { [opKey](count: number, label: string) { return 6; } };
"#;
    let message = message_for(source, 2322).expect("TS2322 for the index mismatch");
    assert!(
        message.contains("{ [opKey]: (count: number, label: string) => number; }"),
        "method parameters should survive in the display: {message}"
    );
}

#[test]
fn a_getter_setter_pair_displays_both_members_like_tsc() {
    // tsc renders the pair as two property-style members with the same
    // spelling (getter by return type, setter by parameter type).
    let source = r#"
declare const pairKey: string;
interface StrTable { [slot: string]: string }
const table: StrTable = { get [pairKey]() { return 7; }, set [pairKey](v: number) {} };
"#;
    let message = message_for(source, 2322).expect("TS2322 for the index mismatch");
    assert!(
        message.contains("{ [pairKey]: number; [pairKey]: number; }"),
        "accessor pair should render both members: {message}"
    );
}

// ---------------------------------------------------------------------------
// The #16721 fold rule applies to methods exactly as to property assignments.
// ---------------------------------------------------------------------------

#[test]
fn a_lone_non_entity_wide_keyed_method_folds_to_the_generic_clause() {
    let source = r#"
declare const stem: string;
interface StrTable { [slot: string]: string }
const table: StrTable = { [stem + "s"]() { return 8; } };
"#;
    let message = message_for(source, 2322).expect("TS2322 for the index mismatch");
    assert!(
        message.contains("{ [x: string]: () => number; }"),
        "non-entity key cannot be re-spelled and must fold: {message}"
    );
}

#[test]
fn one_non_entity_sibling_folds_the_entity_named_method_too() {
    let source = r#"
declare const goodKey: string;
interface StrTable { [slot: string]: string }
const table: StrTable = { [goodKey]() { return 9; }, [goodKey + "x"]() { return 10; } };
"#;
    let message = message_for(source, 2322).expect("TS2322 for the index mismatch");
    assert!(
        message.contains("{ [x: string]: () => number; }"),
        "one non-entity sibling folds the whole group: {message}"
    );
    assert!(
        !message.contains("[goodKey]:"),
        "no member may keep its own spelling once the group folds: {message}"
    );
}

// ---------------------------------------------------------------------------
// Fallback and negative controls.
// ---------------------------------------------------------------------------

#[test]
fn a_plainly_named_method_keeps_the_structural_display() {
    // Not captured into an index bucket, so the literal falls back to the
    // structural formatter — named methods keep their method-signature form.
    let source = r#"
interface StrTable { [slot: string]: string }
const table: StrTable = { plainAction() { return 11; } };
"#;
    let message = message_for(source, 2322).expect("TS2322 for the mismatch");
    assert!(
        message.contains("plainAction"),
        "named method display should be unchanged: {message}"
    );
    assert!(
        !message.contains("[x: string]"),
        "a named method is not an index-signature contributor: {message}"
    );
}

#[test]
fn an_assignable_wide_keyed_method_reports_nothing() {
    let source = r#"
declare const quietKey: string;
const table: { [slot: string]: () => number } = { [quietKey]() { return 12; } };
"#;
    assert_eq!(
        check_source_strict_messages(source),
        vec![],
        "assignable literal must stay silent"
    );
}

#[test]
fn a_well_known_symbol_method_stays_a_named_member() {
    // `[Symbol.iterator]` late-binds to the well-known member; a value
    // mismatch against a target declaring it reports per-member, never the
    // synthesized index clause or a re-spelled wide-key form.
    let source = r#"
interface WithIter { [Symbol.iterator]: string }
const holder: WithIter = { [Symbol.iterator]() { return 13; } };
"#;
    let messages = check_source_strict_messages(source);
    assert!(
        !messages.is_empty(),
        "the mismatch must still be reported somewhere"
    );
    assert!(
        messages.iter().all(|(_, m)| !m.contains("[x: ")),
        "well-known symbol member must not fold into an index clause: {messages:?}"
    );
}
