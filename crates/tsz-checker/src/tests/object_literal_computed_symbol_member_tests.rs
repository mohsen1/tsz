//! Regression coverage for #9763: object literal members declared with a
//! computed `unique symbol` key were not being registered as symbol-named
//! when written as method shorthand or accessors, so the resulting object
//! type's indexable key set was missing the symbol key.
//!
//! Structural rule: when an object literal member has a computed
//! `unique symbol` key, the synthesized `PropertyInfo.is_symbol_named`
//! flag must be set, regardless of whether the member was written as a
//! property assignment, method shorthand, generator/async method, or
//! get/set accessor. Symmetrically, a literal-string-keyed member
//! (`"foo"() {}`) must carry `is_string_named: true`.

use crate::test_utils::check_source_diagnostics;

fn assert_no_ts2322_or_2536(label: &str, src: &str) {
    let diags = check_source_diagnostics(src);
    let bad: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2322 | 2536))
        .collect();
    assert!(
        bad.is_empty(),
        "{label}: expected no TS2322/TS2536 from indexed access; got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

// ── primary repro: method shorthand with unique symbol key ──────────────────

#[test]
fn method_shorthand_unique_symbol_indexable_by_typeof() {
    assert_no_ts2322_or_2536(
        "method shorthand",
        r#"
declare const s: unique symbol;
const om = { [s]() { return 1; } };
type M = (typeof om)[typeof s];
const f: () => number = om[s];
"#,
    );
}

// ── property form (regression guard) ────────────────────────────────────────

#[test]
fn property_arrow_unique_symbol_still_indexable() {
    assert_no_ts2322_or_2536(
        "property assignment",
        r#"
declare const s: unique symbol;
const op = { [s]: () => 1 };
type P = (typeof op)[typeof s];
const f: () => number = op[s];
"#,
    );
}

// ── renamed binding proves the fix is structural, not name-keyed ────────────

#[test]
fn renamed_unique_symbol_method_indexable() {
    assert_no_ts2322_or_2536(
        "renamed symbol binding",
        r#"
declare const sym: unique symbol;
const o = { [sym]() { return "x"; } };
type R = (typeof o)[typeof sym];
const f: () => string = o[sym];
"#,
    );
}

// ── async and generator method shorthand variants ───────────────────────────

#[test]
fn async_method_shorthand_unique_symbol_indexable() {
    assert_no_ts2322_or_2536(
        "async method",
        r#"
declare const s: unique symbol;
const oa = { async [s]() { return 1; } };
type Ma = (typeof oa)[typeof s];
const f: () => Promise<number> = oa[s];
"#,
    );
}

#[test]
fn generator_method_shorthand_unique_symbol_indexable() {
    assert_no_ts2322_or_2536(
        "generator method",
        r#"
declare const s: unique symbol;
const og = { *[s]() { yield 1; } };
type Mg = (typeof og)[typeof s];
"#,
    );
}

// ── get/set accessors with unique symbol keys ───────────────────────────────

#[test]
fn getter_accessor_unique_symbol_indexable() {
    assert_no_ts2322_or_2536(
        "getter accessor",
        r#"
declare const s: unique symbol;
const og = { get [s]() { return 1; } };
type Mg = (typeof og)[typeof s];
const n: number = og[s];
"#,
    );
}

#[test]
fn setter_accessor_unique_symbol_indexable() {
    assert_no_ts2322_or_2536(
        "setter accessor",
        r#"
declare const s: unique symbol;
const os = { set [s](v: number) {} };
os[s] = 42;
"#,
    );
}

#[test]
fn paired_getter_setter_unique_symbol_indexable() {
    assert_no_ts2322_or_2536(
        "paired accessor",
        r#"
declare const s: unique symbol;
const ogs = {
    get [s]() { return 1; },
    set [s](v: number) {}
};
type M = (typeof ogs)[typeof s];
const n: number = ogs[s];
ogs[s] = 2;
"#,
    );
}

// ── keyof must include `typeof s` (the unique symbol type), not a string ────

#[test]
fn keyof_method_shorthand_unique_symbol_yields_symbol_key() {
    let diags = check_source_diagnostics(
        r#"
declare const s: unique symbol;
const om = { [s]() { return 1; } };
type KM = keyof typeof om;
const k1: KM = s;
const k2: KM = "anything";
"#,
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "keyof of a unique-symbol-keyed method should reject string assignment but accept the symbol; got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

// ── mismatched indexed access still reports TS2322 ──────────────────────────

#[test]
fn method_shorthand_unique_symbol_wrong_type_still_errors() {
    let diags = check_source_diagnostics(
        r#"
declare const s: unique symbol;
const om = { [s]() { return 1; } };
const bad: () => string = om[s];
"#,
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "wrong-typed assignment must still produce a single TS2322; got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

// ── mixed: string-keyed and symbol-keyed members coexist ────────────────────

#[test]
fn mixed_string_and_symbol_method_keys_resolve_independently() {
    assert_no_ts2322_or_2536(
        "mixed key shapes",
        r#"
declare const s: unique symbol;
const mx = { foo: 1, [s]() { return "y"; } };
type SVal = (typeof mx)[typeof s];
type FooVal = (typeof mx)["foo"];
const sv: () => string = mx[s];
const fv: number = mx.foo;
"#,
    );
}

// ── multiple distinct symbol-keyed methods ──────────────────────────────────

#[test]
fn multiple_unique_symbol_methods_resolve_independently() {
    assert_no_ts2322_or_2536(
        "multiple symbol methods",
        r#"
declare const s1: unique symbol;
declare const s2: unique symbol;
const m = { [s1]() { return 1; }, [s2]() { return "x"; } };
const a: () => number = m[s1];
const b: () => string = m[s2];
"#,
    );
}

// ── well-known symbol via method shorthand ──────────────────────────────────

#[test]
fn well_known_symbol_method_shorthand_indexable() {
    assert_no_ts2322_or_2536(
        "well-known symbol method",
        r#"
const oit = {
    [Symbol.iterator]() {
        return { next: () => ({ value: 1, done: false }) };
    }
};
type Mit = (typeof oit)[typeof Symbol.iterator];
"#,
    );
}
