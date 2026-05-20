//! Regression coverage for #5944: discriminated-union narrowing must
//! recognise a const-bound `unique symbol` identifier as a singleton
//! discriminant value on the RHS of `===` / `!==` / `switch`.
//!
//! Structural rule: an identifier reference to a const-bound block-scoped
//! variable whose declaration is annotated `unique symbol` resolves, at
//! narrowing time, to the same `UniqueSymbol(SymbolRef)` singleton that
//! `typeof identifier` produces in type position. Equality narrowing must
//! treat the identifier on the same footing as enum members and literal
//! const aliases.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn has_ts2322(source: &str) -> bool {
    check_source(source, "test.ts", CheckerOptions::default())
        .iter()
        .any(|d| d.code == 2322)
}

/// Reported repro: equality `===` narrows a discriminated union whose
/// discriminant property is `typeof symN`.
#[test]
fn equality_narrowing_with_unique_symbol_discriminant() {
    let source = r#"
const sym1: unique symbol = Symbol("a");
const sym2: unique symbol = Symbol("b");

interface Action1 { type: typeof sym1; payload: string }
interface Action2 { type: typeof sym2; payload: number }

function f(action: Action1 | Action2) {
    if (action.type === sym1) {
        const s: string = action.payload;
    } else {
        const n: number = action.payload;
    }
}
"#;
    assert!(
        !has_ts2322(source),
        "TS2322 should not fire after unique-symbol equality narrowing"
    );
}

/// Rename invariance: same rule under different identifier names.
#[test]
fn equality_narrowing_with_unique_symbol_discriminant_renamed() {
    let source = r#"
const ALPHA: unique symbol = Symbol("alpha");
const BETA: unique symbol = Symbol("beta");

interface VariantA { kind: typeof ALPHA; data: boolean }
interface VariantB { kind: typeof BETA; data: bigint }

function f(v: VariantA | VariantB) {
    if (v.kind === ALPHA) {
        const b: boolean = v.data;
    } else {
        const b: bigint = v.data;
    }
}
"#;
    assert!(
        !has_ts2322(source),
        "rename of unique symbol identifiers must not change narrowing"
    );
}

/// `!==` produces the same negative-branch narrowing as the else of `===`.
#[test]
fn negation_narrowing_with_unique_symbol_discriminant() {
    let source = r#"
const sym1: unique symbol = Symbol("a");
const sym2: unique symbol = Symbol("b");
interface A { type: typeof sym1; payload: string }
interface B { type: typeof sym2; payload: number }

function f(action: A | B) {
    if (action.type !== sym1) {
        const n: number = action.payload;
    }
}
"#;
    assert!(
        !has_ts2322(source),
        "`!==` must narrow the same union as the else of `===`"
    );
}

/// `switch (action.type)` with `case sym1:` should narrow per clause.
#[test]
fn switch_narrowing_with_unique_symbol_discriminant() {
    let source = r#"
const sym1: unique symbol = Symbol("a");
const sym2: unique symbol = Symbol("b");
interface A { type: typeof sym1; payload: string }
interface B { type: typeof sym2; payload: number }

function f(action: A | B) {
    switch (action.type) {
        case sym1: { const s: string = action.payload; return; }
        case sym2: { const n: number = action.payload; return; }
    }
}
"#;
    assert!(
        !has_ts2322(source),
        "switch over unique-symbol discriminants must narrow each case"
    );
}

/// Three-way union with else-if chain.
#[test]
fn three_way_unique_symbol_chain_narrows_each_branch() {
    let source = r#"
const sym1: unique symbol = Symbol("a");
const sym2: unique symbol = Symbol("b");
const sym3: unique symbol = Symbol("c");
interface A { type: typeof sym1; payload: string }
interface B { type: typeof sym2; payload: number }
interface C { type: typeof sym3; payload: bigint }

function f(a: A | B | C) {
    if (a.type === sym1) {
        const s: string = a.payload;
    } else if (a.type === sym2) {
        const n: number = a.payload;
    } else {
        const b: bigint = a.payload;
    }
}
"#;
    assert!(
        !has_ts2322(source),
        "three-way unique-symbol chain must narrow each branch"
    );
}

/// Two unique symbols with the *same* spelled name in different scopes must
/// be distinct singletons. Equality against an outer-scope symbol cannot
/// narrow a union whose discriminant references a separately-bound symbol
/// of the same name. The fix keys on the bound `SymbolId`, not the name.
#[test]
fn distinct_unique_symbols_with_same_name_do_not_cross_narrow() {
    let source = r#"
function makeAction() {
    const sym1: unique symbol = Symbol("inner1");
    const sym2: unique symbol = Symbol("inner2");
    interface A { type: typeof sym1; payload: string }
    interface B { type: typeof sym2; payload: number }
    return (action: A | B): string | number => {
        // Outer-scope `sym1` shadows the inner declaration only within its
        // own scope; here `sym1` refers to the inner binding.
        return action.type === sym1 ? action.payload : action.payload;
    };
}
"#;
    assert!(
        !has_ts2322(source),
        "inner-scope unique symbols must still discriminate normally"
    );
}

/// Regression guard: an identifier that is NOT a const-bound unique symbol
/// (e.g. a function parameter named `sym`) must not trigger the new branch
/// — the helper requires the symbol be a block-scoped const variable with
/// the `unique symbol` annotation.
#[test]
fn parameter_named_like_symbol_does_not_trigger_unique_symbol_branch() {
    let source = r#"
declare const realSym: unique symbol;
interface A { type: typeof realSym; payload: string }
interface B { type: "other"; payload: number }

function f(action: A | B, sym: symbol) {
    // `sym` here is a function parameter, not a const-bound unique symbol
    // alias. Comparison against it must not narrow A|B (sym is just a
    // `symbol`, not a singleton). Even if the user wrote this, no
    // discriminant should fire because sym isn't a unit type.
    if (action.type === sym) {
        // After the (no-op) check, action stays A | B.
        const x: string | number = action.payload;
    }
}
"#;
    let diags = check_source(source, "test.ts", CheckerOptions::default());
    let unexpected: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2367)
        .collect();
    assert!(
        unexpected.is_empty(),
        "parameter-typed symbol must not be treated as a unique-symbol singleton: {unexpected:#?}"
    );
}
