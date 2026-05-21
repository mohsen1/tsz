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
    // `Symbol.iterator`-style property-access keys must continue to produce
    // canonical `[Symbol.xxx]` named members. The wide-symbol bypass is
    // gated on bare-identifier expressions, so a property access never
    // triggers it. The target's symbol index signature still rejects the
    // mismatched value via TS2418 — proving the named-member path is intact.
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

// Negative: a computed key against a real index signature must still use TS2418.
#[test]
fn computed_key_against_real_number_index_signature_keeps_ts2418() {
    let codes = diagnostic_codes_for_ts(
        r#"
const o: { [k: number]: string } = { [0]: 42 };
"#,
    );
    assert!(
        codes.contains(
            &diagnostic_codes::TYPE_OF_COMPUTED_PROPERTYS_VALUE_IS_WHICH_IS_NOT_ASSIGNABLE_TO_TYPE
        ),
        "expected TS2418 for computed key against a number index signature, got {codes:?}",
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "did not expect TS2322 for index-signature mismatch, got {codes:?}",
    );
}
