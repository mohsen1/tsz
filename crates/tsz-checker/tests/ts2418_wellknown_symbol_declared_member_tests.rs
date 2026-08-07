//! `TS2418`-vs-`TS2322` selection for a well-known-symbol computed key against
//! a *declared* member (#16662, item 3).
//!
//! A well-known symbol (`Symbol.iterator`, `Symbol.toStringTag`, ...) or a
//! `unique symbol` const is never late-bound to an ordinary property, even
//! when the target has a named member spelled with the same symbol — `tsc`
//! reports `TS2418` there, exactly as it does when the same key only matches
//! through a `[k: symbol]` index signature. Only a *literal-spelled* computed
//! key (`["p"]`, `[0]`, `` [`p`] ``) is late-bound to the named member and
//! takes the ordinary `TS2322`/`TS2353` path (#16661).
//!
//! Oracle: pinned `typescript@7.0.2`, `--noEmit --strict --lib es2022
//! --target es2022`, every row measured on both sides.

use crate::context::CheckerOptions;
use crate::test_utils::{check_source_with_libs, load_lib_files};

fn symbol_libs() -> Vec<std::sync::Arc<tsz_binder::lib_loader::LibFile>> {
    load_lib_files(&[
        "es5.d.ts",
        "es2015.d.ts",
        "es2015.core.d.ts",
        "es2015.collection.d.ts",
        "es2015.iterable.d.ts",
        "es2015.symbol.d.ts",
        "es2015.symbol.wellknown.d.ts",
    ])
}

fn diag_codes(source: &str) -> Vec<u32> {
    check_source_with_libs(source, "test.ts", CheckerOptions::default(), &symbol_libs())
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn wellknown_symbol_against_declared_member_reports_ts2418() {
    let codes = diag_codes(
        r#"
const named: { [Symbol.iterator]: string } = { [Symbol.iterator]: 1 };
"#,
    );
    assert_eq!(
        codes,
        [2418],
        "well-known-symbol key against a declared member must report TS2418, got {codes:?}"
    );
}

#[test]
fn renamed_wellknown_symbol_against_declared_member_reports_ts2418() {
    let codes = diag_codes(
        r#"
const tagged: { [Symbol.toStringTag]: string } = { [Symbol.toStringTag]: 1 };
"#,
    );
    assert_eq!(
        codes,
        [2418],
        "a different well-known symbol must also report TS2418, got {codes:?}"
    );
}

#[test]
fn unique_symbol_const_against_declared_member_reports_ts2418() {
    let codes = diag_codes(
        r#"
declare const s: unique symbol;
interface Named { [s]: number; }
const named: Named = { [s]: "x" };
"#,
    );
    assert_eq!(
        codes,
        [2418],
        "unique-symbol const key against a declared member must report TS2418, got {codes:?}"
    );
}

#[test]
fn wellknown_symbol_against_index_signature_still_reports_ts2418() {
    let codes = diag_codes(
        r#"
interface WithSym { [k: symbol]: string; }
const w: WithSym = { [Symbol.iterator]: 1 };
"#,
    );
    assert_eq!(
        codes,
        [2418],
        "well-known-symbol key against an index signature must keep TS2418, got {codes:?}"
    );
}

#[test]
fn literal_spelled_key_against_declared_member_stays_ts2322() {
    let codes = diag_codes(
        r#"
interface Named { member: number; }
const named: Named = { ["member"]: "text" };
"#,
    );
    assert_eq!(
        codes,
        [2322],
        "a literal-spelled computed key must still take TS2322, got {codes:?}"
    );
}

#[test]
fn wellknown_symbol_against_declared_member_clean_when_assignable() {
    let codes = diag_codes(
        r#"
const named: { [Symbol.iterator]: string } = { [Symbol.iterator]: "ok" };
"#,
    );
    assert!(
        codes.is_empty(),
        "an assignable well-known-symbol value must not error, got {codes:?}"
    );
}

#[test]
fn wellknown_symbol_call_argument_against_declared_member_reports_ts2418() {
    let codes = diag_codes(
        r#"
function f(arg: { [Symbol.iterator]: string }): void {}
f({ [Symbol.iterator]: 1 });
"#,
    );
    assert_eq!(
        codes,
        [2418],
        "the call-argument elaborator must apply the same rule, got {codes:?}"
    );
}
