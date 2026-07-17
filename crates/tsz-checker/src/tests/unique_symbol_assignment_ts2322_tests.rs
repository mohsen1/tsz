//! Regression tests for #9752: assigning between two distinct `unique symbol`
//! types must emit TS2322 (like tsc), not TS2719 ("two different types with
//! this name"). TS2719 is reserved for distinct *named nominal* types sharing a
//! name; two `unique symbol` types stringify identically but are separate
//! symbol identities, so the failure must route through the standard TS2322
//! path. The fix detects unique-symbol operands structurally, not by display.

use crate::context::CheckerOptions;
use crate::test_utils::check_source;

fn strict() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
}

fn codes(src: &str) -> Vec<u32> {
    check_source(src, "test.ts", strict())
        .iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn assigning_unique_symbol_to_other_unique_symbol_is_ts2322_not_ts2719() {
    let c = codes(
        r#"
declare const s1: unique symbol;
declare const s2: unique symbol;
let b: typeof s1;
b = s2;
"#,
    );
    assert!(c.contains(&2322), "expected TS2322, got {c:?}");
    assert!(!c.contains(&2719), "must not emit TS2719, got {c:?}");
}

#[test]
fn declaration_form_unique_symbol_mismatch_is_ts2322() {
    let c = codes(
        r#"
declare const s1: unique symbol;
declare const s2: unique symbol;
const a: typeof s1 = s2;
"#,
    );
    assert!(c.contains(&2322), "expected TS2322, got {c:?}");
    assert!(!c.contains(&2719), "must not emit TS2719, got {c:?}");
}

#[test]
fn renamed_unique_symbols_still_ts2322_not_name_based() {
    // Different identifier spellings — proves the routing is structural, not
    // keyed on a shared display name.
    let c = codes(
        r#"
declare const alpha: unique symbol;
declare const beta: unique symbol;
let target: typeof alpha;
target = beta;
"#,
    );
    assert!(c.contains(&2322), "expected TS2322, got {c:?}");
    assert!(!c.contains(&2719), "must not emit TS2719, got {c:?}");
}

#[test]
fn same_unique_symbol_assignment_is_clean() {
    let c = codes(
        r#"
declare const s1: unique symbol;
let b: typeof s1;
b = s1;
"#,
    );
    assert!(
        !c.contains(&2322) && !c.contains(&2719),
        "same-symbol assignment must be clean, got {c:?}"
    );
}

#[test]
fn unique_symbol_vs_wide_symbol_stays_ts2322() {
    let c = codes(
        r#"
declare const s1: unique symbol;
declare const w: symbol;
const x: typeof s1 = w;
"#,
    );
    assert!(c.contains(&2322), "expected TS2322, got {c:?}");
    assert!(!c.contains(&2719), "must not emit TS2719, got {c:?}");
}

// --- #58: a bare `unique symbol` binding initializer widens to `symbol` ---
// tsc's `getWidenedUniqueESSymbolType` widens a bare unique-symbol *value read*
// at any variable-like binding (`let`/`const`/`var`) to `symbol`, so the binding
// is no longer assignable to `typeof cs`. tsz previously kept the unique symbol
// (a false-negative: two missed TS2322s). Verified against tsc 7.0.2.

#[test]
fn let_binding_of_unique_symbol_widens_to_symbol() {
    let c = codes(
        r#"
declare const cs: unique symbol;
let p = cs;
const chk: typeof cs = p;
"#,
    );
    assert!(
        c.contains(&2322),
        "let p = cs must widen to symbol (TS2322 on `typeof cs = p`), got {c:?}"
    );
}

#[test]
fn plain_const_binding_of_unique_symbol_widens_to_symbol() {
    let c = codes(
        r#"
declare const cs: unique symbol;
const p = cs;
const chk: typeof cs = p;
"#,
    );
    assert!(
        c.contains(&2322),
        "const p = cs must widen to symbol (unlike a literal const), got {c:?}"
    );
}

#[test]
fn var_binding_of_unique_symbol_widens_to_symbol() {
    let c = codes(
        r#"
declare const cs: unique symbol;
var p = cs;
const chk: typeof cs = p;
"#,
    );
    assert!(
        c.contains(&2322),
        "var p = cs must widen to symbol, got {c:?}"
    );
}

#[test]
fn renamed_unique_symbol_binding_widens_to_symbol() {
    // Different identifier spellings — proves the widening is structural, not
    // keyed on the `cs` name.
    let c = codes(
        r#"
declare const alpha: unique symbol;
let beta = alpha;
const chk: typeof alpha = beta;
"#,
    );
    assert!(
        c.contains(&2322),
        "renamed binders must still widen, got {c:?}"
    );
}

#[test]
fn annotated_unique_symbol_binding_stays_unique() {
    // An explicit `typeof cs` annotation preserves the unique identity — the
    // widening only applies to *inferred* binding types.
    let c = codes(
        r#"
declare const cs: unique symbol;
const p: typeof cs = cs;
const chk: typeof cs = p;
"#,
    );
    assert!(
        !c.contains(&2322),
        "annotated binding must stay unique (no widening), got {c:?}"
    );
}

#[test]
fn factory_const_symbol_stays_unique() {
    // A freshly minted `const s = Symbol()` keeps its own `typeof s` identity;
    // only an *alias* of an existing unique symbol widens.
    let c = codes(
        r#"
const s = Symbol();
const chk: typeof s = s;
"#,
    );
    assert!(
        !c.contains(&2322),
        "const s = Symbol() must keep its unique identity, got {c:?}"
    );
}

#[test]
fn union_of_unique_symbols_binding_is_not_widened() {
    // Only a *bare* unique symbol widens; a union member is preserved
    // (`typeof a | typeof b`), matching tsc — assigning back to that union is
    // clean, which would fail if the binding had widened to `symbol`.
    let c = codes(
        r#"
declare const sA: unique symbol;
declare const sB: unique symbol;
let u = Math.random() ? sA : sB;
const chk: typeof sA | typeof sB = u;
"#,
    );
    assert!(
        !c.contains(&2322),
        "a union of unique symbols must not widen to symbol, got {c:?}"
    );
}
