//! Regression coverage for tsc's `isValidIndexKeyType` over *resolved* types.
//!
//! The AST-only validity fallback can only recurse type-alias bodies that live
//! in the current file's arena. A bare reference to the lib global `PropertyKey`
//! (`type PropertyKey = string | number | symbol`, declared in `lib.es5.d.ts`)
//! resolves to a symbol whose declaration node lives in a *different* arena, so
//! the AST recursion bailed out and tsz emitted a TS1268 false positive that
//! `tsc` never reports (issue #12371). The same gap affects any user alias
//! declared in another module.
//!
//! The fix routes the primary determination through the resolved key `TypeId`
//! (tsc's `everyType(type, isValidIndexKeyType)`), so a union/intersection of
//! `string`/`number`/`symbol`/template-literal members is accepted regardless of
//! which source file declares the alias.

use crate::CheckerOptions;
use crate::test_utils::{
    check_multi_file_with_libs, check_source_with_libs, diagnostic_codes, load_default_lib_files,
};

fn codes_with_lib(source: &str) -> Vec<u32> {
    diagnostic_codes(&check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions::default(),
        &load_default_lib_files(),
    ))
}

/// The reported witness: a `readonly [k: PropertyKey]` index signature in a
/// type-literal alias must not emit TS1268. `PropertyKey` is the lib global
/// `string | number | symbol`.
#[test]
fn property_key_index_signature_in_type_alias_is_valid() {
    let codes =
        codes_with_lib("export type UnknownProperties = { readonly [k: PropertyKey]: unknown };");
    assert!(
        !codes.contains(&1268),
        "TS1268 must not fire for `[k: PropertyKey]` (lib union alias): {codes:?}"
    );
}

/// The same `PropertyKey` key must be accepted in every index-signature
/// context: interface, class, and inline type literal — not just type aliases.
#[test]
fn property_key_index_signature_across_containers_is_valid() {
    let codes = codes_with_lib(
        r#"
interface IFace { [k: PropertyKey]: unknown }
class Cls { [k: PropertyKey]: unknown }
declare const inline: { [k: PropertyKey]: unknown };
type Alias = { [k: PropertyKey]: unknown };
"#,
    );
    assert!(
        !codes.contains(&1268),
        "TS1268 must not fire for `PropertyKey` keys in interface/class/inline/alias: {codes:?}"
    );
}

/// An index value typed with `PropertyKey` is still usable: assigning numeric,
/// string, and symbol keys is accepted (the signature really is established).
#[test]
fn property_key_index_signature_accepts_all_key_kinds() {
    let codes = codes_with_lib(
        r#"
type UnknownProperties = { readonly [k: PropertyKey]: unknown };
const x: UnknownProperties = { a: 1, 2: 2, [Symbol()]: 3 };
"#,
    );
    assert!(!codes.contains(&1268), "TS1268 must not fire: {codes:?}");
    assert!(
        !codes.contains(&2322) && !codes.contains(&2353),
        "string/number/symbol keys must be assignable to a PropertyKey index: {codes:?}"
    );
}

/// A user-defined alias that resolves to a valid key union but is declared in a
/// *different* module exercises the same cross-arena path as the lib global.
#[test]
fn cross_module_key_alias_is_valid() {
    let codes = diagnostic_codes(&check_multi_file_with_libs(
        &[
            ("keys.ts", "export type Key = string | number | symbol;"),
            (
                "main.ts",
                r#"
import type { Key } from "./keys";
export type Bag = { [k: Key]: unknown };
"#,
            ),
        ],
        "main.ts",
        CheckerOptions::default(),
        &load_default_lib_files(),
    ));
    assert!(
        !codes.contains(&1268),
        "TS1268 must not fire for a valid key alias imported from another module: {codes:?}"
    );
}

/// Subsets of `string | number | symbol` via the lib alias chain stay valid.
#[test]
fn aliases_to_key_subsets_are_valid() {
    let codes = codes_with_lib(
        r#"
type Sk = string | symbol;
type Nk = number | symbol;
type A = { [k: Sk]: unknown };
type B = { [k: Nk]: unknown };
"#,
    );
    assert!(
        !codes.contains(&1268),
        "TS1268 must not fire for `string | symbol` / `number | symbol` aliases: {codes:?}"
    );
}

/// The fix must not over-accept: a union alias that includes a non-key member
/// (`boolean`) still triggers TS1268, matching tsc.
#[test]
fn union_alias_with_invalid_member_still_emits_ts1268() {
    let codes = codes_with_lib(
        r#"
type Bad = string | boolean;
type T = { [k: Bad]: unknown };
"#,
    );
    assert!(
        codes.contains(&1268),
        "TS1268 expected for `string | boolean` index key alias: {codes:?}"
    );
}

/// `any` is not a valid index key type in tsc (TS1268), even though every type
/// is assignable to/from `any`. This pins that the fix uses tsc's structural
/// `isValidIndexKeyType` rather than an assignability test.
#[test]
fn any_index_key_still_emits_ts1268() {
    let codes = codes_with_lib("type T = { [k: any]: unknown };");
    assert!(
        codes.contains(&1268),
        "TS1268 expected for `[k: any]`: {codes:?}"
    );
}
