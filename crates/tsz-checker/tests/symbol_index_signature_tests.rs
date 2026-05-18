use tsz_checker::diagnostics::diagnostic_codes;
use tsz_checker::test_utils::{
    check_js_source_diagnostics, check_source_diagnostics, check_source_strict_codes,
};

// Helper: assert no TS2322 errors
fn assert_no_ts2322(codes: &[u32], context: &str) {
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "{context}: expected no TS2322, got {codes:?}",
    );
}

fn diagnostic_codes_for_ts(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

fn strict_diagnostic_codes_for_ts(source: &str) -> Vec<u32> {
    check_source_strict_codes(source)
}

fn diagnostic_codes_for_js(source: &str) -> Vec<u32> {
    check_js_source_diagnostics(source)
        .into_iter()
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

// ── Issue #6251: non-unique symbol property access ────────────────────────────
//
// Rule: when an object is indexed with the general `symbol` type, the result
// is the union of all symbol-named property types (computed symbol keys), plus
// any general `[key: symbol]: T` index signature — NOT `undefined`.

#[test]
fn non_unique_symbol_property_read_returns_declared_type_on_interface() {
    // The exact repro from issue #6251.
    // `sym` has the general `symbol` type (not `unique symbol`).
    let codes = diagnostic_codes_for_ts(
        r#"
declare const sym: symbol;

interface WithSymbol {
  [sym]: number;
}

declare const ws: WithSymbol;
const _wss: number = ws[sym];
"#,
    );
    assert_no_ts2322(
        &codes,
        "interface with symbol-keyed prop accessed via symbol type",
    );
}

#[test]
fn non_unique_symbol_property_read_with_renamed_param_still_works() {
    // Vary the variable name: `key` instead of `sym`.
    // Both are `symbol`-typed (not `unique symbol`).
    let codes = diagnostic_codes_for_ts(
        r#"
declare const key: symbol;

interface Container {
  [key]: string;
}

declare const c: Container;
const _v: string = c[key];
"#,
    );
    assert_no_ts2322(
        &codes,
        "renamed symbol variable in interface computed property",
    );
}

#[test]
fn symbol_indexed_access_returns_union_of_broad_symbol_computed_props() {
    // Multiple broad symbol-keyed properties: accessing with `symbol` returns their union.
    let codes = diagnostic_codes_for_ts(
        r#"
declare const s1: symbol;
declare const s2: symbol;

interface Multi {
  [s1]: number;
  [s2]: string;
}

declare const m: Multi;
declare const k: symbol;

// The access must not be assignable to just one branch — must be the union.
const _n: number | string = m[k];
"#,
    );
    assert_no_ts2322(
        &codes,
        "symbol access on interface with multiple symbol-named props",
    );
}

#[test]
fn symbol_indexed_access_rejects_unique_symbol_only_interface() {
    // A general `symbol` cannot index specific unique-symbol-only properties.
    let codes = strict_diagnostic_codes_for_ts(
        r#"
declare const s1: unique symbol;
declare const s2: unique symbol;

interface Multi {
  [s1]: number;
  [s2]: string;
}

declare const m: Multi;
declare const k: symbol;
const _n: number | string = m[k];
"#,
    );
    assert!(
        codes.contains(&7053),
        "general symbol index should reject unique-symbol-only interface props with TS7053, got {codes:?}",
    );
}

#[test]
fn symbol_indexed_access_rejects_unique_symbol_only_object_type_literal() {
    let codes = strict_diagnostic_codes_for_ts(
        r#"
declare const sym: unique symbol;
declare const obj: { [sym]: boolean };
declare const k: symbol;
const _b: boolean = obj[k];
"#,
    );
    assert!(
        codes.contains(&7053),
        "general symbol index should reject unique-symbol-only type literal props with TS7053, got {codes:?}",
    );
}

#[test]
fn symbol_indexed_access_combined_with_symbol_index_signature() {
    // An interface that has both a specific symbol property AND a general
    // `[key: symbol]: T` index sig. The access must resolve correctly.
    let codes = diagnostic_codes_for_ts(
        r#"
declare const specific: unique symbol;

interface Mixed {
  [specific]: number;
  [key: symbol]: number | boolean;
}

declare const m: Mixed;
declare const k: symbol;
const _v: number | boolean = m[k];
"#,
    );
    assert_no_ts2322(
        &codes,
        "symbol access on interface with both specific prop and symbol index sig",
    );
}

#[test]
fn symbol_indexed_access_false_positive_does_not_occur_for_plain_object() {
    // Object without any symbol-keyed property — accessing with `symbol` should
    // still produce no TS2322 when the target type is compatible.
    // (The result is `undefined`; assigning to `undefined` is fine.)
    let codes = diagnostic_codes_for_ts(
        r#"
interface Plain { a: number }
declare const p: Plain;
declare const k: symbol;
const _u: undefined = p[k] as undefined;
"#,
    );
    assert_no_ts2322(
        &codes,
        "symbol access on plain interface with no symbol props (cast to undefined)",
    );
}
