use tsz_checker::CheckerOptions;
use tsz_checker::diagnostics::diagnostic_codes;
use tsz_checker::test_utils::{
    check_js_source_diagnostics, check_multi_file, check_source_code_messages,
    check_source_diagnostics, check_source_with_libs, load_lib_files,
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
fn renamed_unique_symbol_property_access_reports_computed_property_value_mismatch() {
    let codes = diagnostic_codes_for_ts(
        r#"
declare const Sym: { readonly foo: unique symbol };

const table: { [k: symbol]: string } = {
    [Sym.foo]: 123,
};
"#,
    );

    assert!(
        codes.contains(
            &diagnostic_codes::TYPE_OF_COMPUTED_PROPERTYS_VALUE_IS_WHICH_IS_NOT_ASSIGNABLE_TO_TYPE
        ),
        "expected TS2418 for renamed unique-symbol property access, got {codes:?}",
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
fn plain_symbol_property_access_reports_whole_object_ts2322() {
    // A plain (non-`unique`) `symbol`-typed property-access key that is NOT
    // well-known syntax (`Sym.foo`, not `Symbol.foo`) is an index contributor,
    // not a late-bound named member: it folds into a `[k: symbol]: number`
    // index signature, so the whole-object relation owns the failure with
    // TS2322 (`'symbol' index signatures are incompatible`), NOT a per-property
    // TS2418. Oracled against `tsc` 6.0.2 (`--strict`). See #16662.
    let codes = diagnostic_codes_for_ts(
        r#"
declare const Sym: { readonly foo: symbol };

const table: { [k: symbol]: string } = {
    [Sym.foo]: 123,
};
"#,
    );

    assert!(
        codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "expected whole-object TS2322 for a plain-symbol index-contributor key, got {codes:?}",
    );
    assert!(
        !codes.contains(
            &diagnostic_codes::TYPE_OF_COMPUTED_PROPERTYS_VALUE_IS_WHICH_IS_NOT_ASSIGNABLE_TO_TYPE
        ),
        "a wide-symbol index-contributor key must not take the late-bound TS2418, got {codes:?}",
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

fn assert_interface_symbol_index_merge_is_clean(codes: &[u32], context: &str) {
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "{context}: interface merge should not produce TS2322, got {codes:?}",
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE),
        "{context}: symbol index access should not produce TS2536, got {codes:?}",
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_CANNOT_BE_USED_AS_AN_INDEX_TYPE),
        "{context}: symbol index access should not produce TS2538, got {codes:?}",
    );
    assert!(
        !codes.contains(&diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_EXPRESSION_OF_TYPE_CANT_BE_USED_TO_IN),
        "{context}: symbol index access should not produce TS7053, got {codes:?}",
    );
}

#[test]
fn interface_extends_plain_base_preserves_derived_symbol_index_signature() {
    let codes = diagnostic_codes_for_ts(
        r#"
declare const key: symbol;

interface Base {
    base: number;
}

interface Derived extends Base {
    [k: symbol]: boolean;
    own: string;
}

declare const d: Derived;
const indexed: boolean = d[key];
const inherited: number = d.base;
const own: string = d.own;
const keyofIncludesSymbol: keyof Derived = key;
"#,
    );

    assert_interface_symbol_index_merge_is_clean(&codes, "derived symbol index plus plain base");
}

#[test]
fn interface_extends_base_symbol_index_preserves_base_symbol_index_signature() {
    let codes = diagnostic_codes_for_ts(
        r#"
declare const key: symbol;

interface Base {
    [k: symbol]: boolean;
    base: number;
}

interface Derived extends Base {
    own: string;
}

declare const d: Derived;
const indexed: boolean = d[key];
const inherited: number = d.base;
const own: string = d.own;
const keyofIncludesSymbol: keyof Derived = key;
"#,
    );

    assert_interface_symbol_index_merge_is_clean(&codes, "plain derived plus base symbol index");
}

#[test]
fn interface_extends_base_symbol_index_and_derived_string_index_merges_both_key_spaces() {
    let diagnostics = check_source_code_messages(
        r#"
declare const key: symbol;

interface Base {
    [k: symbol]: boolean;
    base: number;
}

interface Derived extends Base {
    [k: string]: number;
    own: number;
}

declare const d: Derived;
const symbolValue: boolean = d[key];
const stringValue: number = d["anything"];
const inherited: number = d.base;
const symbolKey: keyof Derived = key;
"#,
    );
    let codes: Vec<u32> = diagnostics.iter().map(|(code, _)| *code).collect();

    assert!(
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "base symbol index plus derived string index: interface merge should not produce TS2322, got {diagnostics:?}",
    );
    assert_interface_symbol_index_merge_is_clean(
        &codes,
        "base symbol index plus derived string index",
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

    for code in [
        diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_EXPRESSION_OF_TYPE_CANT_BE_USED_TO_IN,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
    ] {
        assert!(
            !codes.contains(&code),
            "imported same-binding symbol access should preserve declared member type, got {codes:?}",
        );
    }
}

#[test]
fn imported_distinct_symbol_binding_does_not_match_computed_member() {
    let codes = diagnostic_codes_for_project(
        &[
            (
                "./a.ts",
                r#"
export declare const memberKey: symbol;
export declare const otherKey: symbol;

export interface WithSymbol {
    [memberKey]: number;
}
"#,
            ),
            (
                "./b.ts",
                r#"
import { otherKey, type WithSymbol } from "./a";

declare const ws: WithSymbol;
const value = ws[otherKey];
"#,
            ),
        ],
        "./b.ts",
    );

    assert!(
        codes.contains(
            &diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_EXPRESSION_OF_TYPE_CANT_BE_USED_TO_IN
        ),
        "different exported symbol bindings must not resolve to the declared member type, got {codes:?}",
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

#[test]
fn ts2353_computed_symbol_excess_property_displays_source_name() {
    let diagnostics = check_source_code_messages(
        r#"
const sym = Symbol();
const obj: { [key: number]: string } = { [sym]: "hello" };
"#,
    );
    let ts2353 = diagnostics
        .iter()
        .find(|(code, _)| *code == diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_DOES_NOT_EXIST_IN_TYPE);
    let Some((_, msg)) = ts2353 else {
        panic!(
            "expected TS2353 for symbol key against number index signature, got: {diagnostics:?}"
        );
    };
    assert!(
        msg.contains("'[sym]'"),
        "computed symbol excess-property diagnostics should display the source key, got: {msg:?}",
    );
    assert!(
        !msg.contains("__unique_"),
        "computed symbol excess-property diagnostics must not leak synthetic symbol keys, got: {msg:?}",
    );
}

#[test]
fn ts7053_branded_string_union_index_display_omits_alias_parentheses() {
    let diagnostics = check_source_code_messages(
        r#"
type Tag1 = { __tag1__: void };
type Tag2 = { __tag2__: void };
type TaggedString1 = string & Tag1;
type TaggedString2 = string & Tag2;

declare let key: TaggedString1 | TaggedString2;
interface Box { [key: TaggedString1]: string }
declare let boxy: Box;
boxy[key];
"#,
    );
    let ts7053 = diagnostics.iter().find(|(code, _)| {
        *code
            == diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_EXPRESSION_OF_TYPE_CANT_BE_USED_TO_IN
    });
    let Some((_, msg)) = ts7053 else {
        panic!("expected TS7053 for incompatible branded-string union key, got: {diagnostics:?}");
    };
    assert!(
        msg.contains("expression of type 'TaggedString1 | TaggedString2'"),
        "TS7053 should preserve the union of branded aliases without member parentheses, got: {msg:?}",
    );
    assert!(
        !msg.contains("(TaggedString1) | (TaggedString2)"),
        "TS7053 should not parenthesize branded alias union members, got: {msg:?}",
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

// Scope guard: a bare wide-`symbol` expression that is not a `symbol`-typed
// identifier (here a call result) is intentionally NOT flagged. tsz widens
// `unique symbol` reads (e.g. `Symbol.iterator`) to wide `symbol`, so a bare
// wide-`symbol` value cannot be distinguished from a widened well-known symbol;
// reporting it would risk false positives on valid well-known-symbol access.
#[test]
fn wide_symbol_call_expression_index_is_not_flagged() {
    let codes = diagnostic_codes_for_ts(
        r#"
declare function makeSym(): symbol;
const o = { a: 1 };
const v = o[makeSym()];
"#,
    );

    assert!(
        !codes.contains(&diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_EXPRESSION_OF_TYPE_CANT_BE_USED_TO_IN),
        "bare wide-symbol call-expression index must not be flagged (widening-safety), got {codes:?}",
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

// ─── Issue #9755: object-literal inference for wide-`symbol` computed keys ──
//
// Structural rule: when an object literal has a computed property whose key
// expression has the wide `symbol` type (`TypeId::SYMBOL`), the inferred
// shape must contribute a `[k: symbol]: V` index signature — not a
// late-bound `__symbol_<file>_<sym>` named member. This matches tsc:
//
//   declare const sym: symbol;
//   const o = { [sym]: 1 };
//   type V = (typeof o)[symbol];   // tsc: number
//   type K = keyof typeof o;       // tsc: symbol
//
// The bypass is limited to bare-identifier computed keys. Property-access
// chains like `[Symbol.iterator]` (unique-symbol-typed) still produce
// canonical named members so that TS2418 mismatches fire as before.

#[test]
fn object_literal_wide_symbol_key_produces_symbol_index_for_indexed_access() {
    let codes = diagnostic_codes_for_ts(
        r#"
declare const sym: symbol;
const o = { [sym]: 1 };
type V = (typeof o)[symbol];
const _v: number = ({} as V);
"#,
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE),
        "wide-symbol computed key should yield a symbol-indexable object, got {codes:?}",
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "(typeof o)[symbol] should resolve to the index value type, got {codes:?}",
    );
}

#[test]
fn object_literal_wide_symbol_key_appears_in_keyof_alongside_named_keys() {
    let codes = diagnostic_codes_for_ts(
        r#"
declare const sym: symbol;
const o = { [sym]: 1, a: 2 };
type K = keyof typeof o;
declare const someSym: symbol;
const _k1: K = "a";
const _k2: K = someSym;
"#,
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "keyof of an object literal with a wide-symbol computed key must include `symbol`, got {codes:?}",
    );
}

#[test]
fn keyof_mixed_string_symbol_index_signature_includes_symbol() {
    let codes = diagnostic_codes_for_ts(
        r#"
interface R {
    [k: string]: number;
    [k: symbol]: number;
}
declare const x: symbol;
const k: keyof R = x;
"#,
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "symbol should be assignable to keyof mixed string/symbol index signature, got {codes:?}",
    );
}

#[test]
fn object_literal_wide_symbol_key_is_structural_renamed_identifier_one() {
    // Different key-variable name — the rule is structural, not identifier-keyed.
    let codes = diagnostic_codes_for_ts(
        r#"
declare const myKey: symbol;
const obj = { [myKey]: "hello" };
type V = (typeof obj)[symbol];
const _v: string = ({} as V);
"#,
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE),
        "wide-symbol computed key with renamed variable should still yield a symbol-indexable object, got {codes:?}",
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "(typeof obj)[symbol] should resolve to the index value type, got {codes:?}",
    );
}

#[test]
fn object_literal_wide_symbol_key_is_structural_renamed_identifier_two() {
    // Third distinct spelling so any test failure attributable to a literal
    // identifier name (`sym`, `myKey`, etc.) would surface here.
    let codes = diagnostic_codes_for_ts(
        r#"
declare const fieldKey: symbol;
const record = { [fieldKey]: true };
type V = (typeof record)[symbol];
const _v: boolean = ({} as V);
"#,
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE),
        "wide-symbol computed key with third spelling should also yield a symbol-indexable object, got {codes:?}",
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "(typeof record)[symbol] should resolve to the index value type, got {codes:?}",
    );
}

#[test]
fn object_literal_wide_symbol_parameter_key_produces_symbol_index() {
    let codes = diagnostic_codes_for_ts(
        r#"
function readField(fieldKey: symbol) {
    const record = { [fieldKey]: 123 };
    type V = (typeof record)[symbol];
    const value: number = ({} as V);
    return value;
}
"#,
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE),
        "wide-symbol parameter key should yield a symbol-indexable object, got {codes:?}",
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "(typeof record)[symbol] should resolve to the parameter-keyed value type, got {codes:?}",
    );
}

#[test]
fn object_literal_wide_symbol_method_key_produces_symbol_index_method_type() {
    let codes = diagnostic_codes_for_ts(
        r#"
declare const fnKey: symbol;
const handlers = { [fnKey](x: number) { return x > 0; } };
type V = (typeof handlers)[symbol];
declare const v: V;
const _ok: boolean = v(1);
"#,
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE),
        "method shorthand with a wide-symbol key should produce a symbol index signature, got {codes:?}",
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "(typeof handlers)[symbol] should resolve to a callable, got {codes:?}",
    );
}

#[test]
fn object_literal_wide_symbol_accessor_key_produces_symbol_index_value() {
    let codes = diagnostic_codes_for_ts(
        r#"
declare const accKey: symbol;
const view = {
    get [accKey](): number { return 0; },
};
type V = (typeof view)[symbol];
const _v: number = ({} as V);
"#,
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE),
        "getter with a wide-symbol key should produce a symbol index signature, got {codes:?}",
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "(typeof view)[symbol] should resolve to the getter return type, got {codes:?}",
    );
}

#[test]
fn object_literal_unique_symbol_key_still_produces_named_member_regression_guard() {
    // Unique symbol keys must keep their named-member semantics — the rule
    // for object-literal inference is restricted to the WIDE `symbol` type.
    let codes = diagnostic_codes_for_ts(
        r#"
declare const uSym: unique symbol;
const obj = { [uSym]: 42 };
type V = (typeof obj)[typeof uSym];
const _v: number = ({} as V);
"#,
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE),
        "unique symbol key access must still resolve to the declared value, got {codes:?}",
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "unique symbol key access must still type as the declared value, got {codes:?}",
    );
}

#[test]
fn object_literal_well_known_symbol_property_access_key_still_resolves_named_member() {
    // Unique-symbol property-access keys must continue to produce canonical
    // named members. Plain-symbol property-access keys can use the symbol-index
    // path, but a structurally declared `unique symbol` member must keep the
    // named-member path intact.
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
        "property-access symbol keys must keep named-member semantics so TS2418 still fires, got {codes:?}",
    );
}

#[test]
fn object_literal_wide_symbol_key_is_assignable_to_explicit_symbol_index_target() {
    // Cross-check: the inferred shape should satisfy an annotated
    // `{ [k: symbol]: V }` target. Before the fix this routed the value
    // into a named property, which was sometimes coincidentally compatible
    // but did not roundtrip through `keyof`/indexed access.
    let codes = diagnostic_codes_for_ts(
        r#"
interface SymbolTable { [key: symbol]: number; }
declare const sym: symbol;
const literal = { [sym]: 5 };
const t: SymbolTable = literal;
const _v: number = t[sym];
"#,
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "object literal with wide-symbol key must satisfy `{{ [k: symbol]: V }}` targets, got {codes:?}",
    );
}

// Issue #9701: keyof of inline/anonymous object type literal drops unique symbol computed keys.
//
// Structural rule: `keyof { [s]: V; ... }` where `s: unique symbol` must include `typeof s`
// in the key union, identically to `type O = { [s]: V; ... }; keyof O`.
#[test]
fn keyof_inline_type_literal_unique_symbol_key_symbol_only() {
    let codes = diagnostic_codes_for_ts(
        r#"
declare const s: unique symbol;
const _key: keyof { [s]: 1 } = s;
"#,
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "s should be assignable to keyof of inline symbol-only type literal (got never), codes: {codes:?}",
    );
}

#[test]
fn keyof_inline_type_literal_unique_symbol_key_mixed_keys() {
    let codes = diagnostic_codes_for_ts(
        r#"
declare const s: unique symbol;
const _key: keyof { [s]: 1; a: 2 } = s;
"#,
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "s should be assignable to keyof of inline mixed type literal (symbol key dropped), codes: {codes:?}",
    );
}

#[test]
fn keyof_inline_type_literal_unique_symbol_named_with_different_variable() {
    // Same rule applies regardless of the variable name used for the symbol.
    let codes = diagnostic_codes_for_ts(
        r#"
declare const mySymbol: unique symbol;
declare const anotherSym: unique symbol;
const _k1: keyof { [mySymbol]: string } = mySymbol;
const _k2: keyof { [anotherSym]: number; prop: boolean } = anotherSym;
"#,
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "unique symbol key in inline type literal must be in keyof regardless of name, codes: {codes:?}",
    );
}

#[test]
fn keyof_inline_type_literal_unique_symbol_const_sym_call_with_lib() {
    // `const t = Symbol()` (no annotation) must be recognised as a unique symbol when the
    // global Symbol constructor is available from lib.  Both inline and named-alias forms
    // must produce the same keyof result so cross-assignment is error-free.
    let libs = load_lib_files(&["es5.d.ts", "es2015.symbol.d.ts"]);
    if libs.is_empty() {
        return;
    }
    let diags = check_source_with_libs(
        r#"
const t = Symbol();
const u = Symbol();
const _a: keyof { [t]: number } = t;
const _b: keyof { [u]: string; prop: boolean } = u;
const _c: keyof { [u]: string; prop: boolean } = "prop";
type Named = { [t]: number };
declare let namedKey: keyof Named;
const _d: keyof { [t]: number } = namedKey;
"#,
        "test.ts",
        CheckerOptions::default(),
        &libs,
    );
    assert!(
        !diags
            .iter()
            .any(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Symbol()-initialised const must appear in keyof of inline type literal when lib is loaded, diags: {diags:?}",
    );
}

#[test]
fn keyof_inline_type_literal_named_alias_and_inline_agree() {
    let codes = diagnostic_codes_for_ts(
        r#"
declare const s: unique symbol;
type Named = { [s]: 1; a: 2 };
type Inline = { [s]: 1; a: 2 };
declare let namedKey: keyof Named;
declare let inlineKey: keyof Inline;
const _a: keyof Named = inlineKey;
const _b: keyof Inline = namedKey;
"#,
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "keyof of named alias and inline type literal with same shape must be identical, codes: {codes:?}",
    );
}

#[test]
fn keyof_inline_type_literal_equal_to_named_alias_via_conditional_types() {
    let codes = diagnostic_codes_for_ts(
        r#"
type Equal<X, Y> =
  (<T>() => T extends X ? 1 : 2) extends
  (<T>() => T extends Y ? 1 : 2)
    ? true
    : false;
type Expect<T extends true> = T;

declare const s: unique symbol;

type B1 = Expect<Equal<keyof { [s]: 1 }, typeof s>>;
type B2 = Expect<Equal<keyof { [s]: 1; a: 2 }, typeof s | 'a'>>;

type O = { [s]: 1; a: 2 };
type C1 = Expect<Equal<keyof O, typeof s | 'a'>>;
"#,
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT),
        "Expect<Equal<...>> must not fail (TS2344): keyof inline type literal should equal keyof named alias, codes: {codes:?}",
    );
}

// When a computed key is a numeric literal (`[0]`) and the target has a
// matching named property (`{ 0: string }`), tsc emits TS2322, not TS2418.
// Structural rule: a computed literal key that resolves to a concrete named
// property is equivalent to a direct property assignment — the type
// mismatch code is TS2322, and the literal value is widened (1 → number).
#[test]
fn numeric_literal_computed_key_matching_named_property_reports_ts2322() {
    let codes = diagnostic_codes_for_ts(
        r#"
const o: { 0: string } = { [0]: 1 };
"#,
    );
    assert!(
        codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "expected TS2322 for numeric-literal computed key matching named property, got {codes:?}",
    );
    assert!(
        !codes.contains(
            &diagnostic_codes::TYPE_OF_COMPUTED_PROPERTYS_VALUE_IS_WHICH_IS_NOT_ASSIGNABLE_TO_TYPE
        ),
        "did not expect TS2418 when computed key resolves to a named property, got {codes:?}",
    );
}

// Same rule with a renamed numeric literal key variable name to prove the
// fix is structural, not keyed on any specific identifier spelling.
#[test]
fn numeric_literal_computed_key_matching_named_property_reports_ts2322_renamed_key() {
    let codes = diagnostic_codes_for_ts(
        r#"
const o: { 1: boolean } = { [1]: "not-a-bool" };
"#,
    );
    assert!(
        codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "expected TS2322 for renamed numeric-literal computed key, got {codes:?}",
    );
    assert!(
        !codes.contains(
            &diagnostic_codes::TYPE_OF_COMPUTED_PROPERTYS_VALUE_IS_WHICH_IS_NOT_ASSIGNABLE_TO_TYPE
        ),
        "did not expect TS2418 when computed key resolves to a named property, got {codes:?}",
    );
}

// Multiple numeric literal computed keys — each mismatch should be TS2322.
#[test]
fn multiple_numeric_literal_computed_keys_matching_named_properties_report_ts2322() {
    let codes = diagnostic_codes_for_ts(
        r#"
const o: { 0: string; 1: number } = { [0]: 1, [1]: "x" };
"#,
    );
    let ts2322_count = codes
        .iter()
        .filter(|&&c| c == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .count();
    assert_eq!(
        ts2322_count, 2,
        "expected two TS2322 errors for two mismatched numeric-literal computed keys, got {codes:?}",
    );
    assert!(
        !codes.contains(
            &diagnostic_codes::TYPE_OF_COMPUTED_PROPERTYS_VALUE_IS_WHICH_IS_NOT_ASSIGNABLE_TO_TYPE
        ),
        "did not expect any TS2418 when all computed keys resolve to named properties, got {codes:?}",
    );
}

// String literal computed key matching a named string property → TS2322.
#[test]
fn string_literal_computed_key_matching_named_property_reports_ts2322() {
    let codes = diagnostic_codes_for_ts(
        r#"
const o: { foo: string } = { ["foo"]: 42 };
"#,
    );
    assert!(
        codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "expected TS2322 for string-literal computed key matching named property, got {codes:?}",
    );
    assert!(
        !codes.contains(
            &diagnostic_codes::TYPE_OF_COMPUTED_PROPERTYS_VALUE_IS_WHICH_IS_NOT_ASSIGNABLE_TO_TYPE
        ),
        "did not expect TS2418 when string-literal computed key resolves to a named property, got {codes:?}",
    );
}

// A *literal-spelled* computed key against a real index signature uses TS2322,
// not TS2418 — the same code it uses when the key resolves to a named property
// just above. This test previously asserted the opposite, drawing the boundary
// at named-member-vs-index-signature; the real boundary is how the key is
// *spelled*. `tsc` 7.0.2 on the exact fixture below:
//
//   $ tsc --noEmit --strict --pretty false --target es2020 exact.ts
//   exact.ts(1,38): error TS2322: Type 'number' is not assignable to type 'string'.
//
// `isComputedNonLiteralName` is false for `[0]`, so the member is an ordinary
// property and never reaches the computed-property message. The negative
// control for the index-signature half is the late-bound row below.
#[test]
fn literal_spelled_computed_key_against_real_number_index_signature_uses_ts2322() {
    let codes = diagnostic_codes_for_ts(
        r#"
const o: { [k: number]: string } = { [0]: 42 };
"#,
    );
    assert!(
        codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "expected TS2322 for a literal-spelled computed key against a number index signature, got {codes:?}",
    );
    assert!(
        !codes.contains(
            &diagnostic_codes::TYPE_OF_COMPUTED_PROPERTYS_VALUE_IS_WHICH_IS_NOT_ASSIGNABLE_TO_TYPE
        ),
        "did not expect TS2418 for a literal-spelled computed key, got {codes:?}",
    );
}

// Negative: a *late-bound* computed key against a real index signature does
// still use TS2418. Same target, same value, different spelling — this is the
// row that stops the rule above from widening into "every computed key against
// an index signature takes TS2322".
#[test]
fn late_bound_computed_key_against_real_number_index_signature_keeps_ts2418() {
    let codes = diagnostic_codes_for_ts(
        r#"
const slot = 0;
const o: { [k: number]: string } = { [slot]: 42 };
"#,
    );
    assert!(
        codes.contains(
            &diagnostic_codes::TYPE_OF_COMPUTED_PROPERTYS_VALUE_IS_WHICH_IS_NOT_ASSIGNABLE_TO_TYPE
        ),
        "expected TS2418 for a late-bound computed key against a number index signature, got {codes:?}",
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "did not expect TS2322 for a late-bound key, got {codes:?}",
    );
}

// ---------------------------------------------------------------------------
// Bare `symbol` index signature (issue #14230).
//
// A type can carry a `symbol` index signature simultaneously with `string`
// and/or `number` ones (e.g. `Record<PropertyKey, V>` /
// `{ [K in string | number | symbol]: V }`). Indexing by the bare `symbol`
// intrinsic must resolve to the value type, not raise TS2536.
// ---------------------------------------------------------------------------

#[test]
fn mapped_property_key_indexed_by_symbol_is_valid() {
    let codes = diagnostic_codes_for_ts(
        "type M = { [K in string | number | symbol]: number };\ntype A = M[symbol];\nexport {};\n",
    );
    assert!(
        !codes.contains(&2536),
        "M[symbol] over a string|number|symbol mapped must not raise TS2536, got {codes:?}",
    );
}

#[test]
fn mapped_property_key_indexed_by_string_and_number_still_valid() {
    let codes = diagnostic_codes_for_ts(
        "type M = { [K in string | number | symbol]: number };\ntype S = M[string];\ntype N = M[number];\nexport {};\n",
    );
    assert!(
        !codes.contains(&2536),
        "M[string]/M[number] arms must not raise TS2536, got {codes:?}",
    );
}

#[test]
fn symbol_only_mapped_indexed_by_symbol_is_valid() {
    let codes = diagnostic_codes_for_ts(
        "type M = { [K in symbol]: number };\ntype A = M[symbol];\nexport {};\n",
    );
    assert!(
        !codes.contains(&2536),
        "symbol-only mapped M[symbol] must not raise TS2536, got {codes:?}",
    );
}

#[test]
fn object_with_both_string_and_symbol_index_keeps_both() {
    let codes = diagnostic_codes_for_ts(
        "type M = { [k: string]: boolean; [k: symbol]: number };\ntype StrV = M[string];\ntype SymV = M[symbol];\nexport {};\n",
    );
    assert!(
        !codes.contains(&2536),
        "string+symbol index coexistence must not raise TS2536, got {codes:?}",
    );
}

#[test]
fn string_only_index_still_rejects_symbol_access() {
    // Negative control: the fix must not blanket-suppress the error. A concrete
    // object with no symbol index, indexed by the bare `symbol` type, is rejected
    // by tsc as TS2538 ("Type 'symbol' cannot be used as an index type") — the
    // `getPropertyTypeForIndexType` message family for a concrete object — not
    // TS2536, which tsc reserves for generic / type-parameter object types.
    let codes = diagnostic_codes_for_ts(
        "type N = { [K in string]: number };\ntype B = N[symbol];\nexport {};\n",
    );
    assert!(
        codes.contains(&2538),
        "string-only index must still reject [symbol] as TS2538, got {codes:?}",
    );
    assert!(
        !codes.contains(&2536),
        "a concrete object indexed by `symbol` is TS2538, not TS2536, got {codes:?}",
    );
}

// A concrete object type (interface, type literal, class, array, …) indexed by
// the bare `symbol` type with no matching symbol index is rejected by tsc as
// TS2538 ("cannot be used as an index type") — the `getPropertyTypeForIndexType`
// message family for concrete objects — never TS2536 ("cannot be used to index
// type"), which tsc reserves for generic / type-parameter object types. Binder
// names are varied so the assertions track the structural shape, not a spelling.

#[test]
fn concrete_interface_string_index_indexed_by_symbol_is_ts2538() {
    let codes = diagnostic_codes_for_ts(
        "interface Bag { [k: string]: number }\ntype V = Bag[symbol];\nexport {};\n",
    );
    assert!(codes.contains(&2538), "expected TS2538, got {codes:?}");
    assert!(!codes.contains(&2536), "must not be TS2536, got {codes:?}");
}

#[test]
fn concrete_number_index_indexed_by_symbol_is_ts2538() {
    let codes = diagnostic_codes_for_ts(
        "type Grid = { [Idx in number]: boolean };\ntype V = Grid[symbol];\nexport {};\n",
    );
    assert!(codes.contains(&2538), "expected TS2538, got {codes:?}");
    assert!(!codes.contains(&2536), "must not be TS2536, got {codes:?}");
}

#[test]
fn concrete_object_literal_no_index_indexed_by_symbol_is_ts2538() {
    let codes = diagnostic_codes_for_ts(
        "type Point = { px: 1; py: 2 };\ntype V = Point[symbol];\nexport {};\n",
    );
    assert!(codes.contains(&2538), "expected TS2538, got {codes:?}");
    assert!(!codes.contains(&2536), "must not be TS2536, got {codes:?}");
}

#[test]
fn concrete_array_indexed_by_symbol_is_ts2538() {
    let codes = diagnostic_codes_for_ts("type V = number[][symbol];\nexport {};\n");
    assert!(codes.contains(&2538), "expected TS2538, got {codes:?}");
    assert!(!codes.contains(&2536), "must not be TS2536, got {codes:?}");
}

#[test]
fn concrete_object_indexed_by_string_or_symbol_union_is_ts2537_and_ts2538() {
    // A union index reports per-member: the general-`string` member has no
    // matching index signature on a property-only object (TS2537), and the
    // `symbol` member cannot be used as an index type (TS2538) — matching tsc.
    let codes = diagnostic_codes_for_ts(
        "type Rec = { only: 1 };\ntype V = Rec[string | symbol];\nexport {};\n",
    );
    assert!(
        codes.contains(&2537),
        "expected TS2537 for string member, got {codes:?}"
    );
    assert!(
        codes.contains(&2538),
        "expected TS2538 for symbol member, got {codes:?}"
    );
    assert!(!codes.contains(&2536), "must not be TS2536, got {codes:?}");
}

#[test]
fn concrete_unique_symbol_keyed_object_indexed_by_bare_symbol_is_ts2538() {
    // The object has a *unique-symbol* key, not a general `symbol` index, so the
    // bare `symbol` index matches nothing and tsc reports TS2538.
    let codes = diagnostic_codes_for_ts(
        "declare const tag: unique symbol;\ntype Tagged = { [tag]: number };\ntype V = Tagged[symbol];\nexport {};\n",
    );
    assert!(codes.contains(&2538), "expected TS2538, got {codes:?}");
    assert!(!codes.contains(&2536), "must not be TS2536, got {codes:?}");
}

#[test]
fn symbol_keyed_mapped_indexed_by_symbol_renamed_binder_is_valid() {
    // Positive control (renamed binder): when the object *does* have a symbol
    // index, `[symbol]` is a valid access — no TS2536/TS2537/TS2538.
    let codes = diagnostic_codes_for_ts(
        "type Reg = { [Sk in symbol]: string };\ntype V = Reg[symbol];\nexport {};\n",
    );
    assert!(
        !codes.contains(&2536) && !codes.contains(&2537) && !codes.contains(&2538),
        "symbol-keyed object indexed by symbol must be accepted, got {codes:?}",
    );
}

#[test]
fn all_keys_keyed_mapped_indexed_by_symbol_is_valid() {
    // Positive control: a `string | number | symbol`-keyed mapped type has a
    // symbol index, so `[symbol]` is valid (the original #14230 witness).
    let codes = diagnostic_codes_for_ts(
        "type Any = { [Pk in string | number | symbol]: number };\ntype V = Any[symbol];\nexport {};\n",
    );
    assert!(
        !codes.contains(&2536) && !codes.contains(&2538),
        "all-keys mapped indexed by symbol must be accepted, got {codes:?}",
    );
}

// ---------------------------------------------------------------------------
// Issue #14796: indexing a *symbol-only* index-signature type by a string or
// number *literal* key must NOT resolve through the symbol index signature.
//
// Structural rule: when the receiver's only index signature is `[k: symbol]: V`
// and the key expression is a string/number literal (or otherwise non-`symbol`)
// property name, tsc treats the access as an implicit-`any` element access and
// reports TS7053 (the symbol index is not a string/number index). tsz was
// silently resolving `x["foo"]` / `x[1]` to `V` because property resolution
// fell through the `string_index` slot, which historically also stored the
// `symbol` index. The fix routes the literal-key lookup through
// `string_index_signature()` (which excludes a `symbol`-keyed signature) and
// makes `string_index_signature_accepts_property` reject `symbol` keys.
// ---------------------------------------------------------------------------

const TS7053: u32 = diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_EXPRESSION_OF_TYPE_CANT_BE_USED_TO_IN;

#[test]
fn symbol_only_index_string_literal_access_reports_ts7053() {
    let codes = diagnostic_codes_for_ts(
        r#"
declare const so: { [k: symbol]: number };
const v = so["foo"];
"#,
    );
    assert!(
        codes.contains(&TS7053),
        "string-literal key on a symbol-only index must report TS7053, got {codes:?}",
    );
}

#[test]
fn symbol_only_index_number_literal_access_reports_ts7053() {
    let codes = diagnostic_codes_for_ts(
        r#"
declare const so: { [k: symbol]: number };
const w = so[1];
"#,
    );
    assert!(
        codes.contains(&TS7053),
        "number-literal key on a symbol-only index must report TS7053, got {codes:?}",
    );
}

#[test]
fn symbol_only_index_interface_string_literal_access_reports_ts7053() {
    let codes = diagnostic_codes_for_ts(
        r#"
interface SymBag { [k: symbol]: number; }
declare const b: SymBag;
const v = b["foo"];
"#,
    );
    assert!(
        codes.contains(&TS7053),
        "string-literal key on a symbol-only interface index must report TS7053, got {codes:?}",
    );
}

#[test]
fn symbol_only_index_class_string_literal_access_reports_ts7053() {
    let codes = diagnostic_codes_for_ts(
        r#"
class C { [k: symbol]: number; }
declare const c: C;
const cv = c["foo"];
"#,
    );
    assert!(
        codes.contains(&TS7053),
        "string-literal key on a symbol-only class index must report TS7053, got {codes:?}",
    );
}

// Renamed binder name and key spelling — proves the rule is structural, not
// keyed on the identifier `so`/`k`/`foo`.
#[test]
fn symbol_only_index_string_literal_access_reports_ts7053_renamed() {
    let codes = diagnostic_codes_for_ts(
        r#"
declare const registry: { [entry: symbol]: boolean };
const hit = registry["lookup"];
"#,
    );
    assert!(
        codes.contains(&TS7053),
        "renamed symbol-only index must still report TS7053 for a string key, got {codes:?}",
    );
}

#[test]
fn symbol_only_index_intersection_string_literal_access_reports_ts7053() {
    let codes = diagnostic_codes_for_ts(
        r#"
declare const x: { [k: symbol]: number } & { name: string };
const a = x["foo"];
"#,
    );
    assert!(
        codes.contains(&TS7053),
        "string key with no named member on a symbol-index intersection must report TS7053, got {codes:?}",
    );
}

// Control: the named half of the intersection still resolves cleanly.
#[test]
fn symbol_only_index_intersection_named_member_access_is_clean() {
    let codes = diagnostic_codes_for_ts(
        r#"
declare const x: { [k: symbol]: number } & { name: string };
const n: string = x["name"];
"#,
    );
    assert!(
        !codes.contains(&TS7053),
        "a named member on a symbol-index intersection must resolve without TS7053, got {codes:?}",
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "x[\"name\"] must type as string, got {codes:?}",
    );
}

// Control: a symbol-typed (wide) key access stays clean — the symbol index
// applies. Mirrors the issue's `so[s]` control.
#[test]
fn symbol_only_index_wide_symbol_key_access_is_clean() {
    let codes = diagnostic_codes_for_ts(
        r#"
declare const so: { [k: symbol]: number };
declare const s: symbol;
const ok: number = so[s];
"#,
    );
    assert!(
        !codes.contains(&TS7053),
        "wide-symbol key on a symbol index must resolve cleanly, got {codes:?}",
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "so[s] must type as the symbol-index value, got {codes:?}",
    );
}

#[test]
fn symbol_only_index_object_rest_preserves_symbol_key_access() {
    let codes = diagnostic_codes_for_ts(
        r#"
declare const sym: symbol;
declare const source: { [entry: symbol]: boolean; drop: string };

const { drop, ...rest } = source;
const value: boolean = rest[sym];
"#,
    );
    assert!(
        !codes.contains(&TS7053),
        "object rest from a symbol-indexed source must preserve the symbol index, got {codes:?}",
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "rest[sym] must type as the symbol-index value, got {codes:?}",
    );
}

// Control: when BOTH a string and a symbol index are present, a string-literal
// key must resolve through the string index (not be rejected).
#[test]
fn string_and_symbol_index_string_literal_access_uses_string_index() {
    let codes = diagnostic_codes_for_ts(
        r#"
declare const m: { [k: string]: number; [k: symbol]: string };
const v: number = m["foo"];
"#,
    );
    assert!(
        !codes.contains(&TS7053),
        "a string key on a string+symbol index must resolve through the string index, got {codes:?}",
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "m[\"foo\"] must type as the string-index value (number), got {codes:?}",
    );
}

// Control: a string-only index still resolves a string-literal key cleanly —
// the fix must not over-reject genuine string index access.
#[test]
fn string_only_index_string_literal_access_is_clean() {
    let codes = diagnostic_codes_for_ts(
        r#"
declare const so: { [k: string]: number };
const v: number = so["foo"];
"#,
    );
    assert!(
        !codes.contains(&TS7053),
        "string-only index must resolve a string-literal key cleanly, got {codes:?}",
    );
}

// Generic adjacent case: a type parameter constrained to a symbol-only index
// indexed by a string literal must report TS7053 (the constraint resolves to a
// symbol-only index, which does not cover a string key).
#[test]
fn symbol_only_index_generic_constraint_string_literal_access_reports_ts7053() {
    let codes = diagnostic_codes_for_ts(
        r#"
function read<T extends { [k: symbol]: number }>(x: T) {
    return x["foo"];
}
"#,
    );
    assert!(
        codes.contains(&TS7053),
        "string-literal key on a symbol-only-constrained type parameter must report TS7053, got {codes:?}",
    );
}

// Generic instantiation adjacent case: `Record<symbol, V>` indexed by a string
// literal flows through the Application property-resolution path, which must
// also exclude the symbol index for a non-symbol key.
#[test]
fn record_symbol_key_string_literal_access_reports_ts7053() {
    let libs = load_lib_files(&["es5.d.ts"]);
    if libs.is_empty() {
        return;
    }
    let diags = check_source_with_libs(
        r#"
declare const r: Record<symbol, number>;
const v = r["foo"];
"#,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    );
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&TS7053),
        "Record<symbol, V> indexed by a string literal must report TS7053, got {codes:?}",
    );
}

// Callable adjacent case: a callable/function type that carries only a symbol
// index signature, indexed by a string literal, flows through the Callable
// property-resolution path, which must also exclude the symbol index.
#[test]
fn symbol_only_index_callable_string_literal_access_reports_ts7053() {
    let codes = diagnostic_codes_for_ts(
        r#"
declare const f: { (): void; [k: symbol]: number };
const v = f["foo"];
"#,
    );
    assert!(
        codes.contains(&TS7053),
        "string-literal key on a symbol-only-index callable must report TS7053, got {codes:?}",
    );
}

// Type-reveal witness from the issue: assigning the result to an incompatible
// annotation must NOT surface a TS2322 (which would prove the access wrongly
// resolved to `number`); the access is an implicit-any TS7053 instead.
#[test]
fn symbol_only_index_string_literal_access_does_not_leak_value_type() {
    let codes = diagnostic_codes_for_ts(
        r#"
declare const so: { [k: symbol]: number };
const x: { reveal: true } = so["foo"];
"#,
    );
    assert!(
        codes.contains(&TS7053),
        "the type-reveal witness must report TS7053, got {codes:?}",
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "so[\"foo\"] must be implicit-any (not number), so no TS2322 should fire, got {codes:?}",
    );
}

include!("symbol_index_signature_tests_parts/part_00.rs");
