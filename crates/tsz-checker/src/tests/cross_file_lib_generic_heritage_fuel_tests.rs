//! #16308: a cross-file interface heritage base resolved to a *lib generic*
//! (e.g. `interface X<T> extends Array<T>`) must not lose its inherited
//! members when `get_type_of_symbol` returns `ERROR`/`UNKNOWN` for that base
//! because the shared `type_resolution_fuel` was already exhausted by
//! unrelated resolution work earlier in the same file-check
//! (`core.rs::get_type_of_symbol`'s fuel gate caches `ERROR` for the symbol).
//!
//! The same-file heritage merge (`merge_interface_heritage_types_inner`,
//! `interface_type.rs`) already falls back to `resolve_lib_type_by_name` when
//! `get_type_of_symbol` comes back `ERROR`/`UNKNOWN` for a heritage base. The
//! cross-file merge (`merge_cross_file_heritage`, `cross_file_lowering.rs`)
//! lacked that fallback and just dropped the base — reproducing the mobx
//! `IObservableArray<T> extends Array<T>` false-positive family (10/33 of the
//! row), which is cross-file only: the interface's own file resolves `Array`
//! through the same-file path (which already has the fallback); an importing
//! file resolves it through the cross-file path (which did not).

use crate::context::CheckerOptions;
use crate::diagnostics::diagnostic_codes;
use crate::test_utils::{check_multi_file_with_libs, load_default_lib_files};

const DEFS: &str = r#"
export interface Bucket<T = any> extends Array<T> {
  own(): T;
}
"#;

fn property_errors(files: &[(&str, &str)], entry: &str) -> Vec<String> {
    let libs = load_default_lib_files();
    let diags = check_multi_file_with_libs(
        files,
        entry,
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    );
    diags
        .into_iter()
        .filter(|d| d.code == diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE)
        .map(|d| d.message_text.to_string())
        .collect()
}

/// Same-file control: extending a lib generic and using inherited `Array`
/// members in the declaring file already works (`merge_interface_heritage_types_inner`
/// has the `resolve_lib_type_by_name` fallback).
#[test]
fn same_file_lib_generic_heritage_member_resolves() {
    let errors = property_errors(
        &[(
            "./main.ts",
            r#"
interface Bucket<T = any> extends Array<T> {
  own(): T;
}
declare const b: Bucket<number>;
b.map(x => x);
b.length;
b.own();
"#,
        )],
        "./main.ts",
    );
    assert!(
        errors.is_empty(),
        "same-file inherited Array members must resolve, got: {errors:?}",
    );
}

/// Cross-file witness: importing the interface and using inherited `Array`
/// members must resolve identically to the same-file case (#16308).
#[test]
fn cross_file_lib_generic_heritage_member_resolves() {
    let errors = property_errors(
        &[
            ("./defs.ts", DEFS),
            (
                "./main.ts",
                r#"
import type { Bucket } from "./defs";
declare const b: Bucket<number>;
b.map(x => x);
b.length;
b.own();
"#,
            ),
        ],
        "./main.ts",
    );
    assert!(
        errors.is_empty(),
        "cross-file inherited Array members must resolve, got: {errors:?}",
    );
}

/// Renamed binder: the imported local alias must not matter.
#[test]
fn cross_file_lib_generic_heritage_renamed_binder_resolves() {
    let errors = property_errors(
        &[
            ("./defs.ts", DEFS),
            (
                "./main.ts",
                r#"
import type { Bucket as Sack } from "./defs";
declare const s: Sack<string>;
s.slice(0);
s.forEach(x => x);
s.own();
"#,
            ),
        ],
        "./main.ts",
    );
    assert!(
        errors.is_empty(),
        "renamed cross-file inherited Array members must resolve, got: {errors:?}",
    );
}

/// Wrapper/nesting: the cross-file generic base is reached through a member
/// of another cross-file interface, not a bare `declare const`.
#[test]
fn cross_file_lib_generic_heritage_through_wrapper_resolves() {
    let errors = property_errors(
        &[
            ("./defs.ts", DEFS),
            (
                "./main.ts",
                r#"
import type { Bucket } from "./defs";
interface Holder { items: Bucket<number> }
declare const h: Holder;
h.items.splice(0, 1);
h.items.own();
"#,
            ),
        ],
        "./main.ts",
    );
    assert!(
        errors.is_empty(),
        "wrapped cross-file inherited Array members must resolve, got: {errors:?}",
    );
}

/// Concrete (non-generic) form control: a concrete cross-file interface
/// extending a lib generic with a fixed type argument.
#[test]
fn cross_file_lib_generic_heritage_concrete_form_resolves() {
    let errors = property_errors(
        &[
            (
                "./defs.ts",
                r#"
export interface StringBucket extends Array<string> {
  own(): string;
}
"#,
            ),
            (
                "./main.ts",
                r#"
import type { StringBucket } from "./defs";
declare const s: StringBucket;
s.join(",");
s.own();
"#,
            ),
        ],
        "./main.ts",
    );
    assert!(
        errors.is_empty(),
        "concrete cross-file inherited Array members must resolve, got: {errors:?}",
    );
}

/// Negative control: a genuinely missing member on the cross-file lib-generic
/// heritage base must still report TS2339 — the fallback must not paper over
/// real gaps.
#[test]
fn cross_file_lib_generic_heritage_missing_member_still_errors() {
    let errors = property_errors(
        &[
            ("./defs.ts", DEFS),
            (
                "./main.ts",
                r#"
import type { Bucket } from "./defs";
declare const b: Bucket<number>;
b.own();
b.totallyAbsentMember;
"#,
            ),
        ],
        "./main.ts",
    );
    assert_eq!(
        errors.len(),
        1,
        "expected exactly one TS2339 for the genuinely missing member, got: {errors:?}",
    );
    assert!(
        errors[0].contains("totallyAbsentMember"),
        "the surviving TS2339 must be for the missing member, got: {errors:?}",
    );
}
