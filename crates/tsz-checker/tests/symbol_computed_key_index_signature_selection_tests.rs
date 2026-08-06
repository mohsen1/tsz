//! Parity pins for #16637: a `symbol`-keyed computed property in a fresh
//! object literal must select the target's **symbol** index signature (or a
//! declared symbol-named member), never the `[k: string]` / `[k: number]`
//! signature.
//!
//! tsc's rule (`getApplicableIndexInfo`): a `symbol`-keyed property is covered
//! only by a `[k: symbol]` index signature (or a declared named member for
//! that unique symbol). A `[k: string]` (or `[k: number]`) index signature does
//! not apply to symbol-keyed properties. Oracled against `typescript@7.0.2`,
//! `--noEmit --strict --target es2022`.
//!
//! A `unique symbol` key stands in for the issue's `const sym = Symbol()`
//! witness; both are a unique-symbol computed key, and using `declare const`
//! keeps the fixtures independent of which `lib` provides `Symbol`.

use tsz_checker::diagnostics::diagnostic_codes;
use tsz_checker::test_utils::check_source_diagnostics;

const TS2418: u32 =
    diagnostic_codes::TYPE_OF_COMPUTED_PROPERTYS_VALUE_IS_WHICH_IS_NOT_ASSIGNABLE_TO_TYPE;

fn codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

// ── false positive: value ok for the symbol signature ────────────────────────
// tsc: clean (`[sym]: "x"` checked against `[k: symbol]: string`).
#[test]
fn symbol_key_value_ok_for_symbol_signature_is_clean() {
    let source = r#"
declare const sym: unique symbol;
interface I { [k: string]: number; [k: symbol]: string; }
const i: I = { a: 1, [sym]: "x" };
"#;
    assert_eq!(codes(source), Vec::<u32>::new());
}

// ── false negative: value bad for the symbol signature ───────────────────────
// tsc: TS2418 (`number` is not assignable to the symbol signature's `string`).
#[test]
fn symbol_key_value_bad_for_symbol_signature_reports_ts2418() {
    let source = r#"
declare const sym: unique symbol;
interface I { [k: string]: number; [k: symbol]: string; }
const i: I = { [sym]: 1 };
"#;
    assert_eq!(codes(source), vec![TS2418]);
}

// ── string-only signature: symbol key is uncovered, not an error ─────────────
// tsc: clean (the symbol property is not covered by the string signature and is
// not excess).
#[test]
fn symbol_key_against_string_only_signature_is_uncovered_and_clean() {
    let source = r#"
declare const sym: unique symbol;
interface I { [k: string]: number; }
const i: I = { [sym]: "x" };
"#;
    assert_eq!(codes(source), Vec::<u32>::new());
}

// ── symbol-only signature: value bad still reports (already correct) ──────────
#[test]
fn symbol_key_against_symbol_only_signature_reports_ts2418_on_mismatch() {
    let source = r#"
declare const sym: unique symbol;
interface I { [k: symbol]: string; }
const i: I = { [sym]: 1 };
"#;
    assert_eq!(codes(source), vec![TS2418]);
}

// ── declared symbol-named member: named-member path, unchanged ───────────────
// A `unique symbol` NAMED member is checked as a property, not through any
// index signature.
#[test]
fn declared_symbol_named_member_value_mismatch_reports_ts2418() {
    let source = r#"
declare const sym: unique symbol;
interface I { [sym]: number; }
const i: I = { [sym]: "x" };
"#;
    assert_eq!(codes(source), vec![TS2418]);
}

#[test]
fn declared_symbol_named_member_value_ok_is_clean() {
    let source = r#"
declare const sym: unique symbol;
interface I { [sym]: number; }
const i: I = { [sym]: 1 };
"#;
    assert_eq!(codes(source), Vec::<u32>::new());
}

// ── negative controls: string / numeric literal keys still use the string /
//    number signature (symbol selection must not steal them) ─────────────────
#[test]
fn string_literal_key_still_uses_string_signature() {
    // `["a"]: "x"` against `[k: string]: number` — the string signature applies,
    // so the mismatch is reported.
    let source = r#"
interface I { [k: string]: number; [k: symbol]: string; }
const i: I = { ["a"]: "x" };
"#;
    assert_eq!(codes(source), vec![TS2418]);
}

#[test]
fn string_literal_key_value_ok_for_string_signature_is_clean() {
    let source = r#"
interface I { [k: string]: number; [k: symbol]: string; }
const i: I = { ["a"]: 1 };
"#;
    assert_eq!(codes(source), Vec::<u32>::new());
}

// ── renamed binders: the rule is structural, not tied to `sym`/`I`/`k` ───────
#[test]
fn renamed_binders_symbol_key_value_ok_is_clean() {
    let source = r#"
declare const marker: unique symbol;
interface Bag { [key: string]: number; [key: symbol]: string; }
const b: Bag = { [marker]: "ok" };
"#;
    assert_eq!(codes(source), Vec::<u32>::new());
}

#[test]
fn renamed_binders_symbol_key_value_bad_reports_ts2418() {
    let source = r#"
declare const marker: unique symbol;
interface Bag { [key: string]: number; [key: symbol]: string; }
const b: Bag = { [marker]: 1 };
"#;
    assert_eq!(codes(source), vec![TS2418]);
}
