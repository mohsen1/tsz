//! Regression coverage for tsc's isValidIndexKeyType AST surface.
//!
//! tsc accepts an intersection like `string & Brand` or two pattern-literal
//! templates intersected as a valid index-signature parameter type, and
//! accepts unions like `string | number | symbol` similarly. These resolve
//! to composite `TypeIds` that don't match the primitive equality check tsz
//! used to perform, so the AST shape must be inspected as a fallback.

use crate::test_utils::check_source_codes;

/// `string & Brand` should be a valid index signature parameter type — no TS1268.
#[test]
fn intersection_of_string_and_brand_is_valid_index_param() {
    let codes = check_source_codes(
        r#"
type Brand = { __brand: 'id' };
type Tagged = string & Brand;

declare let inline: { [key: Tagged]: number };
interface I { [key: Tagged]: number }
type Aliased = { [key: Tagged]: number };
"#,
    );
    assert!(
        !codes.contains(&1268),
        "TS1268 should not fire for `string & Brand` index sig param: {codes:?}"
    );
}

/// Intersection of two pattern-literal templates is valid.
#[test]
fn intersection_of_template_literals_is_valid_index_param() {
    let codes = check_source_codes(
        r#"
declare let v: { [x: `${string}xxx${string}` & `${string}yyy${string}`]: string };
"#,
    );
    assert!(
        !codes.contains(&1268),
        "TS1268 should not fire for intersection of template literal types: {codes:?}"
    );
}

/// `string | number` and other unions of valid types must not trigger TS1268.
#[test]
fn union_of_valid_index_keywords_is_valid_index_param() {
    let codes = check_source_codes(
        r#"
type T1 = { [key: string | number]: any };
type T2 = { [key: number | symbol]: any };
type T3 = { [key: symbol | `foo${string}`]: any };
declare let inline: { [key: string | number]: any };
"#,
    );
    assert!(
        !codes.contains(&1268),
        "TS1268 should not fire for unions of string/number/symbol/template-literal: {codes:?}"
    );
}

/// `T & string` is generic — must trigger TS1337 (literal/generic), not TS1268.
#[test]
fn intersection_with_type_parameter_emits_ts1337_not_ts1268() {
    let codes = check_source_codes(
        r#"
type Invalid<T extends string> = {
    [key: T & string]: string;
};
"#,
    );
    assert!(
        codes.contains(&1337),
        "TS1337 expected for `T & string` (generic intersection): {codes:?}"
    );
    assert!(
        !codes.contains(&1268),
        "TS1268 should not fire when TS1337 already covers the case: {codes:?}"
    );
}

/// Genuinely invalid index sig param types still trigger TS1268.
#[test]
fn boolean_index_sig_param_still_emits_ts1268() {
    let codes = check_source_codes(
        r#"
declare let v: { [key: boolean]: string };
"#,
    );
    assert!(
        codes.contains(&1268),
        "TS1268 expected for `boolean` index sig param: {codes:?}"
    );
}

/// Regression for Devin finding 1: intersection arm in
/// `is_valid_index_sig_param_type` must NOT accept `T & string` and bypass
/// TS1337 in callers that gate on validity. Covers the `interface` path
/// (`interface_type.rs:435`) and the type-alias path (`type_alias_checking.rs:690`).
#[test]
fn generic_intersection_in_interface_emits_ts1337() {
    let codes = check_source_codes(
        r#"
interface I<T extends string> {
    [key: T & string]: string;
}
"#,
    );
    assert!(
        codes.contains(&1337),
        "TS1337 expected for `T & string` in interface (Devin finding 1): {codes:?}"
    );
}

/// Regression for Devin finding 1, `index_signature_checks.rs:100` call site.
/// Mirrors the type-alias case but ensures the validity check inside
/// `index_signature_checks` still routes the generic intersection to TS1337.
#[test]
fn generic_intersection_in_type_alias_emits_ts1337() {
    let codes = check_source_codes(
        r#"
type Bag<T extends string> = {
    [key: T & string]: string;
};
"#,
    );
    assert!(
        codes.contains(&1337),
        "TS1337 expected for `T & string` in type alias (Devin finding 1): {codes:?}"
    );
}

/// Inline type literal in a function parameter: ensures the type-literal
/// checker still emits TS1337 for `T & string` keys.
#[test]
fn generic_intersection_inline_type_literal_emits_ts1337() {
    let codes = check_source_codes(
        r#"
function f<T extends string>(x: { [k: T & string]: any }): void {}
"#,
    );
    assert!(
        codes.contains(&1337),
        "TS1337 expected for `T & string` in inline type literal: {codes:?}"
    );
}

/// Regression for Devin finding 2 (the AST-fallback path in `type_node.rs)`:
/// when a type literal containing `[k: T & string]` is reached via the
/// `TypeNodeChecker`'s `get_type_from_type_literal` (e.g. as the operand of
/// `keyof` or `readonly[]`), the helper `is_type_param_or_literal_in_index_sig`
/// in `type_node_helpers.rs` must recurse into intersection members so that
/// TS1337 fires (instead of TS1268, which is the wrong diagnostic for a
/// generic type parameter). Before the fix, the helper returned false and
/// the AST validity fallback accepted `T & string` as valid, producing
/// only TS1268 — or no error at all.
#[test]
fn generic_intersection_via_keyof_emits_ts1337() {
    let codes = check_source_codes(
        r#"
type X<T extends string> = keyof { [k: T & string]: any };
"#,
    );
    assert!(
        codes.contains(&1337),
        "TS1337 expected for `T & string` in keyof type literal (Devin finding 2): {codes:?}"
    );
    assert!(
        !codes.contains(&1268),
        "TS1268 should NOT fire when TS1337 is the correct diagnostic: {codes:?}"
    );
}

/// Same regression as above, exercised via a `readonly Array<...>` rest of
/// the type-node dispatch (also reaches the `TypeNodeChecker` variant of
/// `is_type_param_or_literal_in_index_sig`).
#[test]
fn generic_intersection_via_readonly_array_emits_ts1337() {
    let codes = check_source_codes(
        r#"
type X<T extends string> = readonly { [k: T & string]: any }[];
"#,
    );
    assert!(
        codes.contains(&1337),
        "TS1337 expected for `T & string` in readonly[] (Devin finding 2): {codes:?}"
    );
    assert!(
        !codes.contains(&1268),
        "TS1268 should NOT fire when TS1337 is the correct diagnostic: {codes:?}"
    );
}

/// Reverse member order: `string & T` should also be detected as generic.
/// Guards against any regression where only the first member is inspected.
#[test]
fn generic_intersection_string_first_emits_ts1337() {
    let codes = check_source_codes(
        r#"
type Bag<T extends string> = {
    [key: string & T]: string;
};
"#,
    );
    assert!(
        codes.contains(&1337),
        "TS1337 expected for `string & T` (generic on right side): {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// #14735: an index signature keyed by a *generic alias application* whose
// instantiation is a concrete branded-string intersection (e.g.
// `Brand<string, 'event'>` with `type Brand<T, Tag> = T & { __tag: Tag }`)
// must NOT emit TS1337. tsc decides TS1337 from the resolved key type, not the
// uninstantiated alias body; the resolved key here is the valid index key
// `string & { __tag: 'event' }`. Witnessed in xstate `graph/types.ts`.
// ---------------------------------------------------------------------------

/// Interface index signature keyed by a generic branded-string alias
/// application is clean (no TS1337, no TS1268).
#[test]
fn branded_string_alias_application_index_key_is_clean_interface() {
    let codes = check_source_codes(
        r#"
type Brand<T, Tag extends string> = T & { __tag: Tag };
type SerializedEvent = Brand<string, 'event'>;
interface AdjacencyMap {
    [key: SerializedEvent]: number;
}
"#,
    );
    assert!(
        !codes.contains(&1337),
        "TS1337 must not fire for an instantiated branded-string index key: {codes:?}"
    );
    assert!(
        !codes.contains(&1268),
        "TS1268 must not fire for an instantiated branded-string index key: {codes:?}"
    );
}

/// The same key spelled inline as a generic-alias application, and as an
/// alias-of-an-alias, are both clean. Covers the type-literal/type-alias paths.
#[test]
fn branded_string_alias_application_index_key_is_clean_inline_and_aliased() {
    let codes = check_source_codes(
        r#"
type Brand<T, Tag extends string> = T & { __tag: Tag };
interface Inline { [key: Brand<string, 'event'>]: number }
type Ser = Brand<string, 'event'>;
type Ser2 = Ser;
type ObjLit = { [key: Ser2]: number };
"#,
    );
    assert!(
        !codes.contains(&1337),
        "TS1337 must not fire for inline/aliased branded-string index keys: {codes:?}"
    );
    assert!(
        !codes.contains(&1268),
        "TS1268 must not fire for inline/aliased branded-string index keys: {codes:?}"
    );
}

/// Numeric and symbol brands resolve to `number & {..}` / `symbol & {..}`,
/// which are also valid index keys. Binder names are varied to guard against
/// any name-based fast path.
#[test]
fn branded_numeric_and_symbol_alias_application_index_keys_are_clean() {
    let codes = check_source_codes(
        r#"
type Flavored<Base, K extends string> = Base & { readonly _kind: K };
type RowId = Flavored<number, 'row'>;
type ColId = Flavored<symbol, 'col'>;
interface Grid {
    [r: RowId]: number;
}
type ColMap = { [c: ColId]: string };
"#,
    );
    assert!(
        !codes.contains(&1337),
        "TS1337 must not fire for numeric/symbol branded index keys: {codes:?}"
    );
    assert!(
        !codes.contains(&1268),
        "TS1268 must not fire for numeric/symbol branded index keys: {codes:?}"
    );
}

/// A branded-string key reached through `keyof` (the `TypeNodeChecker` AST
/// path in `type_node.rs`) is also clean.
#[test]
fn branded_string_alias_application_index_key_is_clean_via_keyof() {
    let codes = check_source_codes(
        r#"
type Brand<T, Tag extends string> = T & { __tag: Tag };
type SerializedEvent = Brand<string, 'event'>;
type K = keyof { [key: SerializedEvent]: number };
"#,
    );
    assert!(
        !codes.contains(&1337),
        "TS1337 must not fire for a branded-string index key under keyof: {codes:?}"
    );
}

/// A generic alias application whose argument is itself a free type parameter
/// (`Brand<X, 'g'>` for free `X`) is still generic and must keep TS1337.
#[test]
fn branded_alias_application_with_free_arg_emits_ts1337() {
    let codes = check_source_codes(
        r#"
type Brand<T, Tag extends string> = T & { __tag: Tag };
interface Box<X> {
    [key: Brand<X, 'g'>]: number;
}
"#,
    );
    assert!(
        codes.contains(&1337),
        "TS1337 expected for `Brand<X, 'g'>` with a free type parameter: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// #14735 adjacent coverage: sites/keys not exercised above. Binder names are
// varied to guard against any name-keyed fast path.
// ---------------------------------------------------------------------------

/// A branded-string alias application used as an index key in an *inline type
/// literal in a function-parameter position* (and in a bare type-literal alias)
/// is clean — neither TS1337 nor TS1268. Covers the type-literal grammar path
/// in addition to the interface/alias sites above.
#[test]
fn branded_key_in_function_parameter_type_literal_is_clean() {
    let codes = check_source_codes(
        r#"
type Stamp<Base, Marker> = Base & { __kind: Marker };
type EventKey = Stamp<string, 'evt'>;
declare function take(x: { [k: EventKey]: number }): void;
type LitForm = { [k: EventKey]: number };
"#,
    );
    assert!(
        !codes.contains(&1337) && !codes.contains(&1268),
        "a branded index key in a function-parameter type literal must be clean: {codes:?}"
    );
}

/// Negative parity: the fix must not over-accept. A bare type parameter and a
/// literal union are still generic/literal index keys (TS1337).
#[test]
fn bare_type_param_and_literal_union_index_keys_still_emit_ts1337() {
    let bare = check_source_codes(r#"interface G<K> { [k: K]: number }"#);
    assert!(
        bare.contains(&1337),
        "bare type parameter key must still emit TS1337: {bare:?}"
    );

    let lit = check_source_codes(r#"interface L { [k: 'a' | 'b']: number }"#);
    assert!(
        lit.contains(&1337),
        "literal-union key must still emit TS1337: {lit:?}"
    );
}
