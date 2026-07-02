//! TS2538 for a `symbol` / unique-symbol key indexing a receiver whose only
//! applicable index signature is `string`.
//!
//! Structural rule (issue #15411): a `string` index signature does not accept
//! `symbol` keys, so `tsc`'s `getPropertyTypeForIndexType`
//! (`indexInfo.keyType === stringType && !isTypeAssignableToKind(indexType,
//! String | Number)`) reports **TS2538** while resolving the access to the
//! string signature's value type — no cascade. tsz previously emitted nothing
//! for a wide `symbol` key (false negative) and TS7053 for a `unique symbol`
//! key (wrong code); both now report TS2538.
//!
//! The predicate is structural (`symbol`-like key + a plain `string` index +
//! no `symbol`-accepting index + no matching symbol member), not keyed on any
//! binder name — the renamed-binder cases below vary every identifier and must
//! behave identically. Cases that mention `Record` / `PropertyKey` / `Symbol()`
//! load the default libs so those names resolve exactly as the CLI resolves
//! them against `tsc`'s lib.

use std::sync::Arc;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::test_utils::{
    diagnostic_code_messages, diagnostic_codes, load_default_lib_files, strict_checker_options,
};

fn code_messages(source: &str, libs: &[Arc<LibFile>]) -> Vec<(u32, String)> {
    diagnostic_code_messages(tsz_checker::test_utils::check_source_with_libs(
        source,
        "test.ts",
        strict_checker_options(),
        libs,
    ))
}

fn codes(source: &str, libs: &[Arc<LibFile>]) -> Vec<u32> {
    diagnostic_codes(&tsz_checker::test_utils::check_source_with_libs(
        source,
        "test.ts",
        strict_checker_options(),
        libs,
    ))
}

fn has_2538(source: &str, libs: &[Arc<LibFile>], expected_type: &str) -> bool {
    code_messages(source, libs).iter().any(|(code, msg)| {
        *code == 2538 && msg == &format!("Type '{expected_type}' cannot be used as an index type.")
    })
}

#[test]
fn wide_symbol_on_string_index_reports_2538() {
    let libs = load_default_lib_files();
    let source = r#"
declare const s: symbol;
declare const o: { [k: string]: number };
o[s];
"#;
    assert!(
        has_2538(source, &libs, "symbol"),
        "wide symbol key on a string-indexed object should report TS2538 'symbol'; got {:?}",
        code_messages(source, &libs),
    );
}

#[test]
fn unique_symbol_on_string_index_reports_2538() {
    let libs = load_default_lib_files();
    let source = r#"
declare const u: unique symbol;
declare const o: { [k: string]: number };
o[u];
"#;
    assert!(
        has_2538(source, &libs, "unique symbol"),
        "unique symbol key on a string-indexed object should report TS2538 'unique symbol'; got {:?}",
        code_messages(source, &libs),
    );
    // The wrong-code TS7053 must not co-occur.
    assert!(
        !codes(source, &libs).contains(&7053),
        "unique symbol key on a string index must not report TS7053",
    );
}

#[test]
fn class_with_string_index_reports_2538() {
    let libs = load_default_lib_files();
    let source = r#"
declare const s: symbol;
class C { [k: string]: number }
declare const c: C;
c[s];
"#;
    assert!(
        has_2538(source, &libs, "symbol"),
        "class string index → TS2538",
    );
}

#[test]
fn callable_interface_with_string_index_reports_2538() {
    let libs = load_default_lib_files();
    let source = r#"
declare const s: symbol;
interface F { (): void; [k: string]: any }
declare const f: F;
f[s];
"#;
    assert!(
        has_2538(source, &libs, "symbol"),
        "callable interface string index → TS2538",
    );
}

#[test]
fn renamed_binders_still_report_2538() {
    let libs = load_default_lib_files();
    // Every identifier is renamed; the rule is structural, not name-keyed.
    let wide = r#"
declare const zeta: symbol;
declare const bag: { [entry: string]: boolean };
bag[zeta];
"#;
    assert!(
        has_2538(wide, &libs, "symbol"),
        "renamed wide-symbol case → TS2538",
    );

    let uniq = r#"
declare const Rho: unique symbol;
declare const bag: { [entry: string]: boolean };
bag[Rho];
"#;
    assert!(
        has_2538(uniq, &libs, "unique symbol"),
        "renamed unique-symbol case → TS2538",
    );
}

#[test]
fn symbol_index_signature_stays_clean() {
    let libs = load_default_lib_files();
    // A genuine `symbol`-accepting index accepts the key: no diagnostic.
    for source in [
        r#"
declare const s: symbol;
declare const os: { [k: symbol]: number };
os[s];
"#,
        r#"
declare const s: symbol;
declare const rec: Record<symbol, number>;
rec[s];
"#,
        r#"
declare const s: symbol;
declare const pk: { [k: PropertyKey]: number };
pk[s];
"#,
        r#"
declare const s: symbol;
declare const both: { [k: string]: number; [k: symbol]: string };
both[s];
"#,
    ] {
        let got = codes(source, &libs);
        assert!(
            !got.contains(&2538) && !got.contains(&7053),
            "symbol-accepting index should stay clean; got {got:?} for {source}",
        );
    }
}

#[test]
fn real_symbol_member_alongside_string_index_stays_clean() {
    let libs = load_default_lib_files();
    // A concrete `unique symbol` member coexisting with a string index is a
    // valid access; tsc resolves it to the member type with no TS2538.
    let source = r#"
declare const kk: unique symbol;
declare const withMember: { [k: string]: number; [kk]: string };
withMember[kk];
"#;
    let got = codes(source, &libs);
    assert!(
        !got.contains(&2538),
        "real symbol member must resolve without TS2538; got {got:?}",
    );
}

#[test]
fn non_string_index_receivers_keep_their_own_codes() {
    let libs = load_default_lib_files();
    // No index signature → TS7053 (implicit any).
    let plain = r#"
declare const s: symbol;
declare const plain: { x: number };
plain[s];
"#;
    let got = codes(plain, &libs);
    assert!(
        got.contains(&7053) && !got.contains(&2538),
        "no-index receiver → TS7053, not TS2538; got {got:?}",
    );

    // Number-only index → TS7015 (index expression not of type number).
    let numi = r#"
declare const s: symbol;
declare const numi: { [k: number]: number };
numi[s];
"#;
    let got = codes(numi, &libs);
    assert!(
        got.contains(&7015) && !got.contains(&2538),
        "number-index receiver → TS7015, not TS2538; got {got:?}",
    );

    // Array → TS7015.
    let arr = r#"
declare const s: symbol;
declare const arr: number[];
arr[s];
"#;
    let got = codes(arr, &libs);
    assert!(
        got.contains(&7015) && !got.contains(&2538),
        "array receiver → TS7015, not TS2538; got {got:?}",
    );
}

#[test]
fn string_index_symbol_access_recovers_value_type_no_cascade() {
    let libs = load_default_lib_files();
    // tsc resolves the access to the string signature's value type (`number`),
    // so a downstream `: null` assignment cascades exactly one TS2322 (number
    // not assignable to null) — the access itself is not `error`/`any`.
    let source = r#"
declare const s: symbol;
declare const o: { [k: string]: number };
const r = o[s];
const bad: null = r;
"#;
    let got = codes(source, &libs);
    assert!(got.contains(&2538), "expected TS2538; got {got:?}");
    assert!(
        got.contains(&2322),
        "access should recover to `number` (string index value), yielding TS2322 on `: null`; got {got:?}",
    );
}
