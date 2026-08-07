//! A `symbol`-keyed object-literal computed property is validated against the
//! `[k: symbol]` index signature, never the `[k: string]`/`[k: number]` one.
//!
//! Regression for #16637. When an object-literal target carried both a string
//! and a symbol index signature (or a string signature only), tsz resolved a
//! `symbol`-keyed computed property's target value type through the *string*
//! index signature instead of the *symbol* one — a false positive when the
//! value mismatched the string signature and a false negative when it happened
//! to satisfy it. Per tsc `getApplicableIndexInfo`, a symbol key selects the
//! symbol index info; a string/number index info does not apply to it.
//!
//! Oracle: `typescript@7.0.2`, `--noEmit --strict --lib es2024 --target es2022`.
//! Binder names (symbol const, alias, index value types) are varied across the
//! rows so no fixture-name string can drive the decision.

use tsz_checker::context::CheckerOptions;

fn check_strict(source: &str) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..Default::default()
    };
    tsz_checker::test_utils::check_source(source, "test.ts", options)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

fn ts2418_count(diags: &[(u32, String)]) -> usize {
    diags.iter().filter(|(c, _)| *c == 2418).count()
}

// ---- False positive: value satisfies the symbol signature -------------------

#[test]
fn both_signatures_symbol_value_ok_no_ts2418() {
    // `[k: symbol]: string` covers `[key]: "v"`; the `[k: string]: number`
    // signature does not apply to the symbol key.
    let source = r#"
declare const key: unique symbol;
interface Bag { [s: string]: number; [s: symbol]: string; }
const bag: Bag = { count: 1, [key]: "v" };
"#;
    let diags = check_strict(source);
    assert!(
        diags.is_empty(),
        "symbol key satisfying the symbol signature must not error: {diags:?}"
    );
}

#[test]
fn both_signatures_symbol_value_ok_renamed_binders() {
    // Same rule, different names + a type-literal target instead of an
    // interface, and the value type swapped (symbol sig -> boolean).
    let source = r#"
declare const marker: unique symbol;
type Store = { [prop: string]: number; [prop: symbol]: boolean };
const store: Store = { [marker]: true };
"#;
    let diags = check_strict(source);
    assert!(
        diags.is_empty(),
        "unique-symbol key satisfying the symbol signature must not error: {diags:?}"
    );
}

#[test]
fn string_signature_only_symbol_key_is_uncovered_no_error() {
    // No symbol signature: the symbol key is not covered by the string
    // signature and is not an excess property either. tsc reports nothing.
    let source = r#"
declare const tag: unique symbol;
interface OnlyString { [k: string]: number; }
const a: OnlyString = { [tag]: "not a number" };
const b: OnlyString = { [tag]: 123 };
"#;
    let diags = check_strict(source);
    assert!(
        diags.is_empty(),
        "a symbol key is uncovered (not string-indexed, not excess): {diags:?}"
    );
}

// ---- False negative: value violates the symbol signature --------------------

#[test]
fn both_signatures_symbol_value_bad_emits_ts2418() {
    // `[k: symbol]: string` requires a string; the numeric value must fail even
    // though it would satisfy the string signature (`number`).
    let source = r#"
declare const key: unique symbol;
interface Bag { [s: string]: number; [s: symbol]: string; }
const bag: Bag = { [key]: 1 };
"#;
    let diags = check_strict(source);
    assert_eq!(
        ts2418_count(&diags),
        1,
        "symbol value violating the symbol signature must emit exactly one TS2418: {diags:?}"
    );
    let msg = &diags.iter().find(|(c, _)| *c == 2418).unwrap().1;
    assert!(
        msg.contains("'number'") && msg.contains("'string'"),
        "TS2418 must compare the value against the symbol signature value type: {msg}"
    );
}

#[test]
fn symbol_signature_only_value_bad_emits_ts2418() {
    // Guard: with only a symbol signature the pre-existing path already errored;
    // it must keep doing so after the selection fix.
    let source = r#"
declare const s: unique symbol;
interface SymOnly { [k: symbol]: string; }
const v: SymOnly = { [s]: 42 };
"#;
    let diags = check_strict(source);
    assert_eq!(
        ts2418_count(&diags),
        1,
        "symbol-only signature still reports the value mismatch: {diags:?}"
    );
}

#[test]
fn both_signatures_string_and_symbol_each_reported() {
    // A bad string-keyed value routes through the string signature (TS2322)
    // while the bad symbol-keyed value routes through the symbol signature
    // (TS2418): both are reported, at their own keys.
    let source = r#"
declare const key: unique symbol;
interface Bag { [s: string]: number; [s: symbol]: string; }
const bag: Bag = { named: "x", [key]: 1 };
"#;
    let diags = check_strict(source);
    assert_eq!(
        diags.iter().filter(|(c, _)| *c == 2322).count(),
        1,
        "the string-keyed value mismatch is TS2322: {diags:?}"
    );
    assert_eq!(
        ts2418_count(&diags),
        1,
        "the symbol-keyed value mismatch is TS2418: {diags:?}"
    );
}

// ---- Negative controls ------------------------------------------------------

#[test]
fn declared_unique_symbol_member_uses_the_member_not_the_index() {
    // The target declares a named member for the unique symbol, so the value is
    // checked against that member (`number`), not any index signature.
    let source = r#"
declare const K: unique symbol;
interface Named { [K]: number; [s: string]: string; }
const ok: Named = { [K]: 7, extra: "s" };
const bad: Named = { [K]: "no" };
"#;
    let diags = check_strict(source);
    assert_eq!(
        ts2418_count(&diags),
        1,
        "only the wrong-typed named unique-symbol member errors: {diags:?}"
    );
    let msg = &diags.iter().find(|(c, _)| *c == 2418).unwrap().1;
    assert!(
        msg.contains("'string'") && msg.contains("'number'"),
        "the named member's declared type (`number`) is the target: {msg}"
    );
}

#[test]
fn numeric_key_still_uses_string_signature() {
    // A non-symbol computed key is unaffected: a numeric-named value still flows
    // through the string signature and reports its mismatch.
    let source = r#"
interface Bag { [s: string]: number; [s: symbol]: string; }
const bag: Bag = { ["0"]: "not a number" };
"#;
    let diags = check_strict(source);
    assert!(
        !diags.is_empty(),
        "a string-literal computed key must still be checked against the string signature: {diags:?}"
    );
}
