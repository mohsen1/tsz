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

// --- #58: a bare `unique symbol` binding read widens to `symbol` ---
// tsc's `widenTypeForVariableLikeDeclaration` widens a bare unique-symbol
// *alias* bound by an un-annotated variable declaration (`let`/`const`/`var`)
// to `symbol` inside `getTypeOfSymbol`, so the binding is no longer assignable
// to `typeof cs`. tsz previously kept the unique symbol (a false-negative: two
// missed TS2322s). The freshly minted `const s = Symbol()` factory (whose
// unique symbol's owning symbol *is* the declaration) and an explicit
// `typeof`/`unique symbol` annotation stay unique. Verified against tsc 7.0.2.

#[test]
fn let_binding_of_unique_symbol_alias_widens_to_symbol() {
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
fn plain_const_binding_of_unique_symbol_alias_widens_to_symbol() {
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
fn var_binding_of_unique_symbol_alias_widens_to_symbol() {
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
fn renamed_unique_symbol_alias_binding_widens_to_symbol() {
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
fn typeof_of_widened_unique_symbol_alias_is_symbol() {
    // The `get_type_of_symbol` read-widening also flows into a `typeof p` query:
    // tsc reports `typeof p` as `symbol`, so `symbol` !<: `typeof cs`.
    let c = codes(
        r#"
declare const cs: unique symbol;
const p = cs;
type T = typeof p;
declare const t: T;
const chk: typeof cs = t;
"#,
    );
    assert!(
        c.contains(&2322),
        "typeof p (p = cs alias) must be symbol, got {c:?}"
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
    // A freshly minted `const s = Symbol()` keeps its own `typeof s` identity —
    // its unique symbol's owning symbol IS the declaration; only an *alias* of
    // an existing unique symbol widens.
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

// --- #60: a bare `unique symbol` destructuring binding element widens to `symbol` ---
// tsc's `widenTypeForVariableLikeDeclaration` `isBindingElement` branch ALWAYS
// widens a bare unique-symbol element — its pattern annotation types the source,
// not the element binding — so an array/object/nested/`let` destructuring element
// reads as `symbol` and is no longer assignable to `typeof cs`. Verified vs tsc 7.0.2.

#[test]
fn array_destructuring_unique_symbol_element_widens_to_symbol() {
    let c = codes(
        r#"
declare const cs: unique symbol;
declare const t: [typeof cs];
const [db] = t;
const chk: typeof cs = db;
"#,
    );
    assert!(
        c.contains(&2322),
        "const [db] = t must widen db to symbol, got {c:?}"
    );
}

#[test]
fn pattern_annotated_destructuring_element_still_widens() {
    // The pattern annotation `[typeof cs]` types the SOURCE, not the element
    // binding, so the element still widens (tsc's isBindingElement branch has no
    // annotation guard).
    let c = codes(
        r#"
declare const cs: unique symbol;
declare const t: [typeof cs];
const [db]: [typeof cs] = t;
const chk: typeof cs = db;
"#,
    );
    assert!(
        c.contains(&2322),
        "pattern-annotated destructuring element must still widen, got {c:?}"
    );
}

#[test]
fn object_and_nested_destructuring_unique_symbol_elements_widen() {
    let c = codes(
        r#"
declare const cs: unique symbol;
declare const o: { k: typeof cs };
declare const on2: [[typeof cs]];
const { k: dd } = o;
const [[dn]] = on2;
const cdd: typeof cs = dd;
const cdn: typeof cs = dn;
"#,
    );
    assert!(
        c.contains(&2322),
        "object + nested destructuring elements must widen to symbol, got {c:?}"
    );
}

#[test]
fn renamed_destructuring_element_widens_to_symbol() {
    // Different binder names — proves the widening is structural, not name-keyed.
    let c = codes(
        r#"
declare const alpha: unique symbol;
declare const tup: [typeof alpha];
const [beta] = tup;
const chk: typeof alpha = beta;
"#,
    );
    assert!(
        c.contains(&2322),
        "renamed destructuring element must widen, got {c:?}"
    );
}

#[test]
fn destructuring_union_of_unique_symbols_element_is_not_widened() {
    // Only a *bare* unique-symbol element widens; a union element is preserved.
    let c = codes(
        r#"
declare const sA: unique symbol;
declare const sB: unique symbol;
declare const t: [typeof sA | typeof sB];
const [u] = t;
const chk: typeof sA | typeof sB = u;
"#,
    );
    assert!(
        !c.contains(&2322),
        "a union destructuring element must not widen to symbol, got {c:?}"
    );
}

// --- #60: a bare `unique symbol` class-field alias widens to `symbol` ---
// tsc widens a bare unique-symbol *alias* class field (static/instance,
// readonly or mutable) to `symbol`, EXCEPT a freshly minted `= Symbol()`
// factory (whose unique symbol's owning symbol is the field itself) or an
// explicit `typeof`/`unique symbol` annotation. Verified vs tsc 7.0.2.

#[test]
fn static_readonly_field_unique_symbol_alias_widens_to_symbol() {
    let c = codes(
        r#"
declare const cs: unique symbol;
class C { static readonly ra = cs; }
const chk: typeof cs = C.ra;
"#,
    );
    assert!(
        c.contains(&2322),
        "static readonly ra = cs must widen to symbol, got {c:?}"
    );
}

#[test]
fn instance_readonly_field_unique_symbol_alias_widens_to_symbol() {
    let c = codes(
        r#"
declare const cs: unique symbol;
class C { readonly ia = cs; }
declare const c: C;
const chk: typeof cs = c.ia;
"#,
    );
    assert!(
        c.contains(&2322),
        "readonly ia = cs must widen to symbol, got {c:?}"
    );
}

#[test]
fn renamed_readonly_field_unique_symbol_alias_widens() {
    // Different binder names — proves the widening is structural.
    let c = codes(
        r#"
declare const alpha: unique symbol;
class K { static readonly beta = alpha; }
const chk: typeof alpha = K.beta;
"#,
    );
    assert!(
        c.contains(&2322),
        "renamed readonly field alias must widen, got {c:?}"
    );
}

#[test]
fn static_readonly_field_symbol_factory_stays_unique() {
    // A freshly minted `static readonly rf = Symbol()` keeps its own `typeof rf`
    // identity — its unique symbol's owning symbol IS the field.
    let c = codes(
        r#"
class C { static readonly rf = Symbol(); }
const chk: typeof C.rf = C.rf;
"#,
    );
    assert!(
        !c.contains(&2322),
        "static readonly rf = Symbol() must keep its unique identity, got {c:?}"
    );
}

#[test]
fn annotated_readonly_field_unique_symbol_stays_unique() {
    // An explicit `typeof cs` annotation preserves the unique identity.
    let c = codes(
        r#"
declare const cs: unique symbol;
class C { static readonly rt: typeof cs = cs; }
const chk: typeof cs = C.rt;
"#,
    );
    assert!(
        !c.contains(&2322),
        "annotated readonly field must stay unique, got {c:?}"
    );
}

// ── Object/array-literal element widening (#64) ─────────────────────────────
// A fresh object/array literal in a const/let binding widens a bare
// `unique symbol` element to `symbol` (tsc `getWidenedUniqueESSymbolType`
// applied recursively by `getWidenedType`), so a `typeof cs` read fails; an
// `as const` or annotated position preserves the unique identity.

#[test]
fn const_object_literal_unique_symbol_property_widens() {
    // Renamed binder `sym`/`holder` proves the routing is structural.
    let c = codes(
        r#"
declare const sym: unique symbol;
const holder = { m: sym };
const chk: typeof sym = holder.m;
"#,
    );
    assert!(
        c.contains(&2322),
        "const object-literal unique-symbol property must widen to symbol, got {c:?}"
    );
}

#[test]
fn const_array_literal_unique_symbol_element_widens() {
    let c = codes(
        r#"
declare const uniq: unique symbol;
const list = [uniq];
const chk: typeof uniq = list[0];
"#,
    );
    assert!(
        c.contains(&2322),
        "const array-literal unique-symbol element must widen to symbol, got {c:?}"
    );
}

#[test]
fn nested_object_literal_unique_symbol_widens() {
    let c = codes(
        r#"
declare const tag: unique symbol;
const outer = { inner: { leaf: tag } };
const chk: typeof tag = outer.inner.leaf;
"#,
    );
    assert!(
        c.contains(&2322),
        "nested object-literal unique-symbol must widen to symbol, got {c:?}"
    );
}

#[test]
fn nested_array_literal_unique_symbol_widens() {
    let c = codes(
        r#"
declare const tok: unique symbol;
const grid = [[tok]];
const chk: typeof tok = grid[0][0];
"#,
    );
    assert!(
        c.contains(&2322),
        "nested array-literal unique-symbol must widen to symbol, got {c:?}"
    );
}

#[test]
fn let_object_literal_unique_symbol_property_widens() {
    let c = codes(
        r#"
declare const key: unique symbol;
let box = { m: key };
const chk: typeof key = box.m;
"#,
    );
    assert!(
        c.contains(&2322),
        "let object-literal unique-symbol property must widen to symbol, got {c:?}"
    );
}

#[test]
fn union_element_in_object_literal_widens() {
    // A conditional over two distinct unique symbols is a mutable element, so
    // each member widens to `symbol` (tsc widens the whole property to symbol).
    let c = codes(
        r#"
declare const first: unique symbol;
declare const second: unique symbol;
declare const cond: boolean;
const wrap = { m: cond ? first : second };
const chk: typeof first | typeof second = wrap.m;
"#,
    );
    assert!(
        c.contains(&2322),
        "union of unique symbols in an object-literal property must widen, got {c:?}"
    );
}

#[test]
fn as_const_object_literal_unique_symbol_preserved() {
    // `as const` marks the property readonly; the unique symbol is preserved.
    let c = codes(
        r#"
declare const pin: unique symbol;
const frozen = { m: pin } as const;
const chk: typeof pin = frozen.m;
"#,
    );
    assert!(
        !c.contains(&2322),
        "as-const object-literal must preserve the unique symbol, got {c:?}"
    );
}

#[test]
fn as_const_array_literal_unique_symbol_preserved() {
    let c = codes(
        r#"
declare const seal: unique symbol;
const frozenList = [seal] as const;
const chk: typeof seal = frozenList[0];
"#,
    );
    assert!(
        !c.contains(&2322),
        "as-const array-literal must preserve the unique symbol, got {c:?}"
    );
}

#[test]
fn annotated_object_literal_binding_unique_symbol_preserved() {
    // An explicit annotation types the binding; its unique symbol is preserved.
    let c = codes(
        r#"
declare const mark: unique symbol;
const annotated: { m: typeof mark } = { m: mark };
const chk: typeof mark = annotated.m;
"#,
    );
    assert!(
        !c.contains(&2322),
        "annotated object-literal binding must preserve the unique symbol, got {c:?}"
    );
}

#[test]
fn nested_as_const_inside_fresh_object_literal_preserved() {
    // A nested `as const` value keeps its unique symbol even inside a fresh
    // outer object literal (readonly positions are never widened).
    let c = codes(
        r#"
declare const csn: unique symbol;
const inner = { n: csn } as const;
const outer = { m: inner };
const chk: typeof csn = outer.m.n;
"#,
    );
    assert!(
        !c.contains(&2322),
        "nested as-const unique symbol must be preserved inside a fresh literal, got {c:?}"
    );
}
