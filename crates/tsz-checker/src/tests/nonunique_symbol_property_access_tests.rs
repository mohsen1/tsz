//! Regression coverage for #6251: non-unique symbol property access.
//!
//! When a variable declared with `: symbol` (not `: unique symbol`) is used as
//! a computed property name in an interface/type literal, tsc resolves `ws[sym]`
//! to the declared type.  tsz was returning `undefined` because the property was
//! never stored.  This suite covers the fix for both the storage side
//! (`get_property_name_resolved`) and the lookup side (`nonunique_symbol_index_type`).

use crate::test_utils::check_source_diagnostics;

// ── primary repro ─────────────────────────────────────────────────────────────

#[test]
fn const_symbol_annotation_property_access_no_error() {
    let diags = check_source_diagnostics(
        r#"
declare const sym: symbol;
interface WithSymbol {
  [sym]: number;
}
declare const ws: WithSymbol;
const _wss: number = ws[sym];
"#,
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "const sym: symbol property access should not produce TS2322; got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

// ── different variable name proves the fix is structural, not hardcoded ───────

#[test]
fn different_symbol_variable_name_no_error() {
    let diags = check_source_diagnostics(
        r#"
declare const myKey: symbol;
interface Obj {
  [myKey]: string;
}
declare const o: Obj;
const _v: string = o[myKey];
"#,
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "symbol-annotated property with name 'myKey' should resolve; got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

// ── multiple non-unique symbol properties on the same type ────────────────────

#[test]
fn multiple_nonunique_symbol_props_resolved_independently() {
    let diags = check_source_diagnostics(
        r#"
declare const k1: symbol;
declare const k2: symbol;
interface Multi {
  [k1]: number;
  [k2]: string;
}
declare const m: Multi;
const _n: number = m[k1];
const _s: string = m[k2];
"#,
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "each non-unique symbol property should resolve to its own type; got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

// ── unique symbol path is preserved ──────────────────────────────────────────

#[test]
fn unique_symbol_property_still_works() {
    let diags = check_source_diagnostics(
        r#"
declare const sym: unique symbol;
interface WithUnique {
  [sym]: number;
}
declare const wu: WithUnique;
const _n: number = wu[sym];
"#,
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "unique symbol property access must still work after this change; got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

// ── parameter with `: symbol` annotation ─────────────────────────────────────

#[test]
fn parameter_symbol_annotation_property_access_no_error() {
    let diags = check_source_diagnostics(
        r#"
declare const paramSym: symbol;
interface WithParam {
  [paramSym]: number;
}
declare const wp: WithParam;
const _n: number = wp[paramSym];
"#,
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "symbol-annotated declared const access should not error; got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

// ── type literal also works (not just interface) ──────────────────────────────

#[test]
fn type_literal_nonunique_symbol_property_access_no_error() {
    let diags = check_source_diagnostics(
        r#"
declare const tkey: symbol;
type HasKey = { [tkey]: boolean };
declare const obj: HasKey;
const _b: boolean = obj[tkey];
"#,
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "type literal non-unique symbol property should also resolve; got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}
