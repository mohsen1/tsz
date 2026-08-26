// ---------------------------------------------------------------------------
// Value-level element access `obj[sym]` where the receiver's only applicable
// index signature is a `string` index. tsc's `checkIndexedAccessIndexType`
// reports TS2538 ("Type 'X' cannot be used as an index type") here — the access
// still resolves to the string signature's value type (so no cascade), but a
// `string` index does not accept a `symbol` key. Previously tsz silently
// resolved the wide-`symbol` case and mis-coded the `unique symbol` case as
// TS7053.
// ---------------------------------------------------------------------------

const TS2538: u32 = diagnostic_codes::TYPE_CANNOT_BE_USED_AS_AN_INDEX_TYPE;
const TS7015: u32 =
    diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_INDEX_EXPRESSION_IS_NOT_OF_TYPE_NUMBE;

fn ts2538_messages_for_ts(source: &str) -> Vec<String> {
    check_source_code_messages(source)
        .into_iter()
        .filter_map(|(code, message)| (code == TS2538).then_some(message))
        .collect()
}

#[test]
fn wide_symbol_value_access_on_string_index_reports_ts2538() {
    let codes = diagnostic_codes_for_ts(
        r#"
declare const sym: symbol;
declare const strIdx: { [k: string]: number };
strIdx[sym];
"#,
    );
    assert!(
        codes.contains(&TS2538),
        "wide `symbol` value access on a string-index object must report TS2538, got {codes:?}",
    );
    assert!(
        !codes.contains(&TS7053),
        "a string-index fallthrough is TS2538, not the implicit-any TS7053, got {codes:?}",
    );
}

#[test]
fn wide_symbol_value_access_on_string_index_reports_exact_ts2538_message() {
    let messages = ts2538_messages_for_ts(
        r#"
declare const token: symbol;
declare const store: { [entry: string]: number };
store[token];
"#,
    );

    assert!(
        messages
            .iter()
            .any(|message| message == "Type 'symbol' cannot be used as an index type."),
        "expected exact TS2538 message for wide symbol key, got {messages:?}",
    );
}

#[test]
fn wide_symbol_value_access_on_string_index_reports_ts2538_renamed_binders() {
    // Anti-hardcoding: identical structural shape, different binder names.
    let codes = diagnostic_codes_for_ts(
        r#"
declare const banana: symbol;
declare const WEIRD_9: { [potato: string]: number };
WEIRD_9[banana];
"#,
    );
    assert!(
        codes.contains(&TS2538),
        "the rule must be structural, not name-driven; expected TS2538, got {codes:?}",
    );
}

#[test]
fn callable_interface_string_index_indexed_by_symbol_reports_ts2538() {
    let codes = diagnostic_codes_for_ts(
        r#"
declare const channel: symbol;
interface Dispatcher {
    (): void;
    [slot: string]: unknown;
}
declare const dispatch: Dispatcher;
dispatch[channel];
"#,
    );

    assert!(
        codes.contains(&TS2538),
        "callable interface with only a string index must reject symbol access as TS2538, got {codes:?}",
    );
    assert!(
        !codes.contains(&TS7053),
        "callable string-index fallthrough is TS2538, not TS7053, got {codes:?}",
    );
}

#[test]
fn unique_symbol_value_access_on_string_index_reports_ts2538() {
    // Previously mis-coded as TS7053: an unmatched `unique symbol` key that only
    // falls through to a `string` index is TS2538, exactly like the wide case.
    let codes = diagnostic_codes_for_ts(
        r#"
declare const us: unique symbol;
declare const noProp: { [k: string]: number };
noProp[us];
"#,
    );
    assert!(
        codes.contains(&TS2538),
        "unmatched `unique symbol` on a string-index object must report TS2538, got {codes:?}",
    );
    assert!(
        !codes.contains(&TS7053),
        "the unique-symbol/string-index case is TS2538, not TS7053, got {codes:?}",
    );
}

#[test]
fn unique_symbol_value_access_on_string_index_reports_exact_ts2538_message() {
    let messages = ts2538_messages_for_ts(
        r#"
declare const privateKey: unique symbol;
declare const slots: { [entry: string]: number };
slots[privateKey];
"#,
    );

    assert!(
        messages
            .iter()
            .any(|message| message == "Type 'unique symbol' cannot be used as an index type."),
        "expected exact TS2538 message for unique symbol key, got {messages:?}",
    );
}

#[test]
fn unique_symbol_value_access_on_string_index_reports_ts2538_renamed_binders() {
    let codes = diagnostic_codes_for_ts(
        r#"
declare const Lexeme: unique symbol;
declare const registry: { [slotName: string]: boolean };
registry[Lexeme];
"#,
    );

    assert!(
        codes.contains(&TS2538),
        "renamed unique-symbol/string-index case should still report TS2538, got {codes:?}",
    );
}

#[test]
fn wide_symbol_value_access_on_class_string_index_reports_ts2538() {
    let codes = diagnostic_codes_for_ts(
        r#"
declare const sym: symbol;
class Bag { [k: string]: number; }
declare const bag: Bag;
bag[sym];
"#,
    );
    assert!(
        codes.contains(&TS2538),
        "a class instance with a string index signature must report TS2538, got {codes:?}",
    );
}

#[test]
fn real_unique_symbol_member_with_string_index_stays_clean() {
    let codes = diagnostic_codes_for_ts(
        r#"
declare const precise: unique symbol;
declare const holder: { [slot: string]: number; [precise]: string };
const value: string = holder[precise];
"#,
    );

    assert!(
        !codes.contains(&TS2538),
        "a real unique-symbol member must win over the string index, got {codes:?}",
    );
    assert!(
        !codes.contains(&TS7053),
        "a real unique-symbol member is not an implicit-any access, got {codes:?}",
    );
}

#[test]
fn wide_symbol_value_access_keeps_string_index_value_type_no_cascade() {
    // tsc keeps the string signature's value type (`number`) for the access, so
    // assigning to `number` adds no error while assigning to `string` reports a
    // single TS2322 — proving the access did NOT collapse to `undefined`/`error`.
    let compatible = diagnostic_codes_for_ts(
        r#"
declare const sym: symbol;
declare const strIdx: { [k: string]: number };
const ok: number = strIdx[sym];
"#,
    );
    assert!(
        compatible.contains(&TS2538),
        "expected the index-type TS2538, got {compatible:?}",
    );
    assert!(
        !compatible.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "the resolved value type is `number`, so `const ok: number` must not add TS2322, got {compatible:?}",
    );

    let incompatible = diagnostic_codes_for_ts(
        r#"
declare const sym: symbol;
declare const strIdx: { [k: string]: number };
const bad: string = strIdx[sym];
"#,
    );
    assert!(
        incompatible.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "the resolved value type `number` is not assignable to `string`; expected TS2322, got {incompatible:?}",
    );
}

#[test]
fn record_symbol_and_property_key_accept_symbol_values() {
    let libs = load_lib_files(&["es5.d.ts"]);
    if libs.is_empty() {
        return;
    }
    let record_diags = check_source_with_libs(
        r#"
declare const mark: symbol;
declare const symbolRecord: Record<symbol, number>;
const a: number = symbolRecord[mark];
"#,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    );
    let record_codes: Vec<u32> = record_diags
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();

    assert!(
        !record_codes.contains(&TS2538),
        "Record<symbol, V> accepts symbol keys, got {record_codes:?}",
    );
    assert!(
        !record_codes.contains(&TS7053),
        "Record<symbol, V> must not degrade to implicit any, got {record_codes:?}",
    );

    let property_key_diags = check_source_with_libs(
        r#"
declare const mark: symbol;
declare const allKeys: { [key: PropertyKey]: string };
const b: string = allKeys[mark];
"#,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    );
    let property_key_codes: Vec<u32> = property_key_diags
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();

    assert!(
        !property_key_codes.contains(&TS2538),
        "PropertyKey index accepts symbol keys, got {property_key_codes:?}",
    );
    assert!(
        !property_key_codes.contains(&TS7053),
        "PropertyKey index must not degrade to implicit any, got {property_key_codes:?}",
    );
}

#[test]
fn wide_symbol_value_access_with_symbol_index_is_clean() {
    // Negative control: a genuine `symbol` index signature accepts any symbol key.
    let codes = diagnostic_codes_for_ts(
        r#"
declare const sym: symbol;
declare const symIdx: { [k: symbol]: number };
symIdx[sym];
declare const both: { [k: string]: number; [k: symbol]: string };
both[sym];
"#,
    );
    assert!(
        !codes.contains(&TS2538),
        "a symbol index signature accepts symbol keys; no TS2538 expected, got {codes:?}",
    );
    assert!(
        !codes.contains(&TS7053),
        "no implicit-any either when a symbol index resolves the access, got {codes:?}",
    );
}

#[test]
fn wide_symbol_value_access_without_index_still_reports_ts7053() {
    // Negative control: with no index signature the diagnostic stays TS7053,
    // not TS2538 (the access is genuinely implicit-any, not a string fallthrough).
    let codes = diagnostic_codes_for_ts(
        r#"
declare const sym: symbol;
declare const named: { x: number };
named[sym];
"#,
    );
    assert!(
        codes.contains(&TS7053),
        "no-index-signature symbol access must stay TS7053, got {codes:?}",
    );
    assert!(
        !codes.contains(&TS2538),
        "TS2538 is reserved for the string-index fallthrough case, got {codes:?}",
    );
}

#[test]
fn wide_symbol_value_access_on_number_index_still_reports_ts7015() {
    // Negative control: a number-only index signature yields TS7015, not TS2538.
    let codes = diagnostic_codes_for_ts(
        r#"
declare const sym: symbol;
declare const numIdx: { [k: number]: string };
numIdx[sym];
"#,
    );
    assert!(
        codes.contains(&TS7015),
        "a number-index object indexed by symbol must report TS7015, got {codes:?}",
    );
    assert!(
        !codes.contains(&TS2538),
        "the number-index case is TS7015, not TS2538, got {codes:?}",
    );
}
