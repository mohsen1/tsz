use tsz_checker::diagnostics::diagnostic_codes;
use tsz_checker::test_utils::{
    check_js_source_diagnostics, check_multi_file, check_source_diagnostics,
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
import { sym, type WithSymbol } from "./a";

declare const ws: WithSymbol;
const value: number = ws[sym];
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
