use tsz_checker::diagnostics::diagnostic_codes;
use tsz_checker::test_utils::{
    check_js_source_diagnostics, check_multi_file, check_source_code_messages,
    check_source_diagnostics,
};
use tsz_common::common::ModuleKind;

fn diagnostic_codes_for_ts(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

fn diagnostic_codes_for_js(source: &str) -> Vec<u32> {
    check_js_source_diagnostics(source)
        .into_iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

fn diagnostic_codes_for_project(files: &[(&str, &str)], entry_file: &str) -> Vec<u32> {
    check_multi_file(
        files,
        entry_file,
        tsz_checker::context::CheckerOptions {
            module: ModuleKind::ESNext,
            strict: true,
            ..tsz_checker::context::CheckerOptions::default()
        },
    )
    .into_iter()
    .filter(|diagnostic| diagnostic.code != 2318)
    .map(|diagnostic| diagnostic.code)
    .collect()
}

#[test]
fn unique_symbol_index_signature_reports_computed_property_value_mismatch() {
    let codes = diagnostic_codes_for_ts(
        r#"
declare const key: unique symbol;

const table: { [k: symbol]: string } = {
    [key]: 123,
};
"#,
    );

    assert!(
        codes.contains(
            &diagnostic_codes::TYPE_OF_COMPUTED_PROPERTYS_VALUE_IS_WHICH_IS_NOT_ASSIGNABLE_TO_TYPE
        ),
        "expected TS2418 for unique symbol index value mismatch, got {codes:?}",
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "did not expect the object-level TS2322 fallback, got {codes:?}",
    );
    assert!(
        !codes.contains(
            &diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_DOES_NOT_EXIST_IN_TYPE
        ),
        "did not expect TS2353 excess property fallback, got {codes:?}",
    );
}

#[test]
fn well_known_symbol_index_signature_reports_computed_property_value_mismatch() {
    let codes = diagnostic_codes_for_ts(
        r#"
declare const Symbol: { readonly iterator: unique symbol };

const table: { [k: symbol]: string } = {
    [Symbol.iterator]: 123,
};
"#,
    );

    assert!(
        codes.contains(
            &diagnostic_codes::TYPE_OF_COMPUTED_PROPERTYS_VALUE_IS_WHICH_IS_NOT_ASSIGNABLE_TO_TYPE
        ),
        "expected TS2418 for symbol index value mismatch, got {codes:?}",
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "did not expect the object-level TS2322 fallback, got {codes:?}",
    );
    assert!(
        !codes.contains(
            &diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_DOES_NOT_EXIST_IN_TYPE
        ),
        "did not expect TS2353 excess property fallback, got {codes:?}",
    );
}

#[test]
fn keyof_well_known_symbol_property_preserves_symbol_key_type() {
    let codes = diagnostic_codes_for_ts(
        r#"
declare const Symbol: { readonly iterator: unique symbol };

type Keys = keyof { [Symbol.iterator]: number };
declare let key: Keys;

const iter: typeof Symbol.iterator = key;
const key2: Keys = Symbol.iterator;
"#,
    );

    assert!(
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "keyof {{[Symbol.iterator]: ...}} should preserve symbol key identity and avoid TS2322, got {codes:?}",
    );
}

#[test]
fn annotated_symbol_index_signature_variable_allows_symbol_key_read() {
    let codes = diagnostic_codes_for_ts(
        r#"
declare const Symbol: { (description?: string): symbol };

interface SymbolIndex {
    [key: symbol]: boolean;
}

const sym = Symbol("key");
const symi: SymbolIndex = { [sym]: true };

const _symi: boolean = symi[sym];
"#,
    );

    assert!(
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "symbol index signature reads should return the signature value type, got {codes:?}",
    );
    assert!(
        !codes.contains(&diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_EXPRESSION_OF_TYPE_CANT_BE_USED_TO_IN),
        "symbol key reads should not report TS7053 when a symbol index signature is present, got {codes:?}",
    );
}

#[test]
fn symbol_typed_computed_interface_member_access_uses_declared_type() {
    let codes = diagnostic_codes_for_ts(
        r#"
declare const Symbol: { (description?: string): symbol };
const sym: symbol = Symbol("test");

interface WithSymbol {
    [sym]: number;
}

declare const ws: WithSymbol;
const value: number = ws[sym];
"#,
    );

    assert!(
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "symbol-valued computed key access should not resolve to undefined, got {codes:?}",
    );
}

#[test]
fn symbol_typed_computed_members_match_same_const_binding_across_shapes() {
    let codes = diagnostic_codes_for_ts(
        r#"
declare const Symbol: { (description?: string): symbol };
const fieldKey: symbol = Symbol("field");
const aliasKey: symbol = Symbol("alias");
const methodKey: symbol = Symbol("method");

interface InterfaceShape {
    [fieldKey]: number;
}

type LiteralShape = {
    [aliasKey]: string;
};

interface MethodShape {
    [methodKey](): boolean;
}

declare const interfaceValue: InterfaceShape;
declare const literalValue: LiteralShape;
declare const methodValue: MethodShape;

const field: number = interfaceValue[fieldKey];
const literal: string = literalValue[aliasKey];
const method: () => boolean = methodValue[methodKey];
const called: boolean = methodValue[methodKey]();
"#,
    );

    assert!(
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "same const symbol binding should preserve declared member types, got {codes:?}",
    );
    assert!(
        !codes.contains(&diagnostic_codes::CANNOT_INVOKE_AN_OBJECT_WHICH_IS_POSSIBLY_UNDEFINED),
        "symbol method access should not resolve to possibly undefined, got {codes:?}",
    );
}

#[test]
fn imported_symbol_typed_computed_member_access_uses_export_binding() {
    let codes = diagnostic_codes_for_project(
        &[
            (
                "./a.ts",
                r#"
export declare const sym: symbol;

export interface WithSymbol {
    [sym]: number;
}
"#,
            ),
            (
                "./b.ts",
                r#"
import { sym as importedSym, type WithSymbol } from "./a";

declare const ws: WithSymbol;
const value: number = ws[importedSym];
"#,
            ),
        ],
        "./b.ts",
    );

    assert!(
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "imported same-binding symbol access should preserve declared member type, got {codes:?}",
    );
}

#[test]
fn jsdoc_symbol_index_signature_reports_computed_property_value_mismatch() {
    let codes = diagnostic_codes_for_js(
        r#"
// @ts-check
/** @type {{ readonly iterator: symbol }} */
const Symbol = /** @type {any} */ ({});

/** @type {{[k: symbol]: string}} */
const table = {
    [Symbol.iterator]: 123,
};
"#,
    );

    assert!(
        codes.contains(
            &diagnostic_codes::TYPE_OF_COMPUTED_PROPERTYS_VALUE_IS_WHICH_IS_NOT_ASSIGNABLE_TO_TYPE
        ),
        "expected TS2418 for JSDoc symbol index value mismatch, got {codes:?}",
    );
    assert!(
        !codes.contains(
            &diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_DOES_NOT_EXIST_IN_TYPE
        ),
        "did not expect TS2353 for a property covered by a JSDoc symbol index, got {codes:?}",
    );
}

#[test]
fn invalid_boolean_index_signature_does_not_create_string_index_fallback() {
    let codes = diagnostic_codes_for_ts(
        r#"
type Table = { [k: boolean]: string };

const table: Table = {
    true: 123,
};
"#,
    );

    assert!(
        codes.contains(
            &diagnostic_codes::AN_INDEX_SIGNATURE_PARAMETER_TYPE_MUST_BE_STRING_NUMBER_SYMBOL_OR_A_TEMPLATE_LIT
        ),
        "expected TS1268 for boolean index signature parameter, got {codes:?}",
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "invalid boolean index signature should not cascade into TS2322, got {codes:?}",
    );
}

#[test]
fn jsdoc_invalid_boolean_index_signature_reports_ts1268_without_required_property() {
    let diagnostics = check_js_source_diagnostics(
        r#"
// @ts-check
/** @type {{[k: boolean]: string}} */
const obj = {};
"#,
    );
    let codes: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();

    assert!(
        codes.contains(
            &diagnostic_codes::AN_INDEX_SIGNATURE_PARAMETER_TYPE_MUST_BE_STRING_NUMBER_SYMBOL_OR_A_TEMPLATE_LIT
        ),
        "expected TS1268 for boolean JSDoc index signature parameter, got {codes:?}",
    );
    assert!(
        !codes.contains(&diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE),
        "invalid JSDoc index signature should not become a required property, got {diagnostics:?}",
    );
}

#[test]
fn jsdoc_unresolved_index_signature_key_reports_ts1268_and_ts2304_without_required_property() {
    let diagnostics = check_js_source_diagnostics(
        r#"
// @ts-check
/** @type {{[k: MissingKey]: string}} */
const obj = {};
"#,
    );
    let codes: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();

    assert!(
        codes.contains(
            &diagnostic_codes::AN_INDEX_SIGNATURE_PARAMETER_TYPE_MUST_BE_STRING_NUMBER_SYMBOL_OR_A_TEMPLATE_LIT
        ),
        "expected TS1268 for unresolved JSDoc index signature parameter, got {codes:?}",
    );
    assert!(
        codes.contains(&diagnostic_codes::CANNOT_FIND_NAME),
        "expected TS2304 for unresolved JSDoc index signature key, got {codes:?}",
    );
    assert!(
        !codes.contains(&diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE),
        "unresolved JSDoc index signature should not become a required property, got {diagnostics:?}",
    );
}

// When a TS2322 fires because the target has a symbol index signature, the
// diagnostic message must display the key type as `symbol`, not `string`.
// This covers the indexSignatures1.ts conformance fingerprint regression.
//
// Structural rule: when the target is `{ [k: symbol]: T }`, tsc shows
// `symbol` as the key kind; tsz was showing `string` due to hardcoding
// in the checker's structural index display path.
#[test]
fn ts2322_symbol_index_signature_target_displays_symbol_key_kind() {
    // Use a typed variable, not an object literal — object literal value
    // mismatches against a symbol index produce TS2418, not TS2322.
    let diagnostics = check_source_code_messages(
        r#"
declare const sym: unique symbol;
declare let src: { [sym]: number };
const dst: { [k: symbol]: string } = src;
"#,
    );
    let ts2322 = diagnostics.iter().find(|(code, _)| *code == 2322);
    let Some((_, msg)) = ts2322 else {
        panic!(
            "expected TS2322 for {{ [sym]: number }} assigned to symbol-indexed string target, got: {diagnostics:?}"
        );
    };
    assert!(
        msg.contains(": symbol]"),
        "TS2322 target must display symbol key kind, got: {msg:?}",
    );
    assert!(
        !msg.contains(": string]"),
        "TS2322 target must not display string key kind for a symbol index, got: {msg:?}",
    );
}

// Same structural rule with different param names to prove the fix is not
// keyed on identifier spelling ("k", "sym", etc.).
#[test]
fn ts2322_symbol_index_signature_target_displays_symbol_key_kind_renamed_params() {
    let diagnostics = check_source_code_messages(
        r#"
declare const myKey: unique symbol;
declare let source: { [myKey]: number };
const dest: { [index: symbol]: string } = source;
"#,
    );
    let ts2322 = diagnostics.iter().find(|(code, _)| *code == 2322);
    let Some((_, msg)) = ts2322 else {
        panic!(
            "expected TS2322 for {{ [myKey]: number }} assigned to symbol-indexed string target, got: {diagnostics:?}"
        );
    };
    assert!(
        msg.contains(": symbol]"),
        "TS2322 target must display symbol key kind regardless of param name, got: {msg:?}",
    );
    assert!(
        !msg.contains(": string]"),
        "TS2322 must not display string key kind for a symbol index signature, got: {msg:?}",
    );
}

// Indexing a concrete object by a wide `symbol`-typed value with no matching
// `symbol` index signature is an implicit-any element access (TS7053).
//
// Structural rule: when `obj[key]` has `key: symbol` (the wide primitive, not a
// `unique symbol` that names a member), and `obj` declares neither a member under
// that binding nor a `symbol` index signature, tsc reports TS7053 (objects) /
// TS7015 (arrays/tuples). This change makes tsz report it too.
#[test]
fn wide_symbol_index_on_plain_object_reports_ts7053() {
    let codes = diagnostic_codes_for_ts(
        r#"
let s: symbol = Symbol();
const o = { a: 1 };
const v1 = o[s];
"#,
    );

    assert!(
        codes.contains(&diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_EXPRESSION_OF_TYPE_CANT_BE_USED_TO_IN),
        "expected TS7053 for wide symbol indexing a plain object, got {codes:?}",
    );
}

// Renamed key variable — proves the rule is structural, not keyed on the name `s`.
#[test]
fn wide_symbol_index_on_plain_object_reports_ts7053_renamed_key() {
    let codes = diagnostic_codes_for_ts(
        r#"
let mySymKey: symbol = Symbol();
const record = { first: 1, second: 2 };
const value = record[mySymKey];
"#,
    );

    assert!(
        codes.contains(&diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_EXPRESSION_OF_TYPE_CANT_BE_USED_TO_IN),
        "expected TS7053 regardless of key variable name, got {codes:?}",
    );
}

// Arrays/tuples have a numeric index signature, so a `symbol` key produces the
// more specific TS7015 (index expression is not of type 'number').
#[test]
fn wide_symbol_index_on_array_reports_ts7015() {
    let codes = diagnostic_codes_for_ts(
        r#"
let s: symbol = Symbol();
const arr: number[] = [1];
const v2 = arr[s];
"#,
    );

    assert!(
        codes.contains(&diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_INDEX_EXPRESSION_IS_NOT_OF_TYPE_NUMBE),
        "expected TS7015 for wide symbol indexing an array, got {codes:?}",
    );
}

#[test]
fn wide_symbol_index_on_tuple_reports_ts7015() {
    let codes = diagnostic_codes_for_ts(
        r#"
let s: symbol = Symbol();
const tup: [number, string] = [1, "a"];
const v = tup[s];
"#,
    );

    assert!(
        codes.contains(&diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_INDEX_EXPRESSION_IS_NOT_OF_TYPE_NUMBE),
        "expected TS7015 for wide symbol indexing a tuple, got {codes:?}",
    );
}

// A non-identifier `symbol`-typed index expression (no binding identity to
// convert) must still report — proves the fix is not limited to identifiers.
#[test]
fn wide_symbol_call_expression_index_reports_ts7053() {
    let codes = diagnostic_codes_for_ts(
        r#"
declare function makeSym(): symbol;
const o = { a: 1 };
const v = o[makeSym()];
"#,
    );

    assert!(
        codes.contains(&diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_EXPRESSION_OF_TYPE_CANT_BE_USED_TO_IN),
        "expected TS7053 for a wide symbol call-expression index, got {codes:?}",
    );
}

// Negative control: a real `symbol` index signature makes the access valid.
#[test]
fn wide_symbol_index_with_symbol_index_signature_is_clean() {
    let codes = diagnostic_codes_for_ts(
        r#"
let s: symbol = Symbol();
const o: { [k: symbol]: number } = {};
const v = o[s];
"#,
    );

    assert!(
        !codes.contains(&diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_EXPRESSION_OF_TYPE_CANT_BE_USED_TO_IN),
        "symbol index signature should make symbol-key reads valid, got {codes:?}",
    );
    assert!(
        !codes.contains(&diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_INDEX_EXPRESSION_IS_NOT_OF_TYPE_NUMBE),
        "symbol index signature should not trigger TS7015, got {codes:?}",
    );
}

// Negative control: a `unique symbol` that actually names a member stays clean.
#[test]
fn unique_symbol_key_that_exists_is_clean() {
    let codes = diagnostic_codes_for_ts(
        r#"
declare const key: unique symbol;
const o = { [key]: 1 };
const v = o[key];
"#,
    );

    assert!(
        !codes.contains(&diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_EXPRESSION_OF_TYPE_CANT_BE_USED_TO_IN),
        "a unique symbol key that exists should not report TS7053, got {codes:?}",
    );
}

// A plain string index signature must still display as `string` (regression guard).
#[test]
fn ts2322_string_index_signature_target_still_displays_string_key_kind() {
    let diagnostics = check_source_code_messages(
        r#"
declare let src: { a: number };
const dst: { [k: string]: string } = src;
"#,
    );
    let ts2322 = diagnostics.iter().find(|(code, _)| *code == 2322);
    let Some((_, msg)) = ts2322 else {
        panic!(
            "expected TS2322 for {{ a: number }} assigned to string-indexed string target, got: {diagnostics:?}"
        );
    };
    assert!(
        msg.contains(": string]"),
        "TS2322 for a string index target must still display string key kind, got: {msg:?}",
    );
    assert!(
        !msg.contains(": symbol]"),
        "string index signature target must not display symbol key kind, got: {msg:?}",
    );
}
