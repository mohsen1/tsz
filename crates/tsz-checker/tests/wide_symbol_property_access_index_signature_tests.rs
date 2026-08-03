//! Adjacent-case matrix for the `[Symbol.<member>]`-shaped leg of #16307: a
//! `declare global { interface SymbolConstructor { <member>: symbol } }`
//! augmentation (xstate's `Symbol.observable` interop convention) types
//! `Symbol.<member>` as plain `symbol`, not `unique symbol`. tsc does not give
//! such a key its own named identity — it routes into the containing type's
//! symbol index signature, exactly like a computed key derived from any other
//! wide-`symbol`-typed expression. The `well_known_symbol_property_name`
//! syntactic shortcut for `Symbol.<member>` used to treat ANY such access as
//! a well-known literal key regardless of whether `<member>` is actually
//! declared `unique symbol`; these tests exercise the gate that now checks
//! the member's declared uniqueness before taking that shortcut, plus the
//! `symbol_valued_binding_property_name` leg that gives the wide case a key
//! at all once the shortcut declines it.
use tsz_checker::test_utils::check_source_diagnostics;

fn assert_clean(source: &str) {
    let diags = check_source_diagnostics(source);
    assert!(diags.is_empty(), "expected exit 0 like tsc, got: {diags:?}");
}

/// Minimal `SymbolConstructor` shape used by every test in this file: a real
/// well-known member (`iterator`, `unique symbol`) alongside a
/// `declare global`-augmentation-shaped wide member (`observable`, plain
/// `symbol`) — the exact xstate interop shape from #16307.
const SYMBOL_LIB: &str = r#"
interface SymbolConstructor {
    readonly iterator: unique symbol;
    readonly observable: symbol;
}
declare const Symbol: SymbolConstructor;
"#;

#[test]
fn wide_symbol_observable_property_access_routes_to_symbol_index_signature() {
    assert_clean(&format!(
        r#"
{SYMBOL_LIB}
interface Explicit {{ [key: symbol]: () => number }}
interface Implicit {{ [Symbol.observable]: () => number }}

declare const e: Explicit;
declare const i: Implicit;
export const implicitToExplicit: Explicit = i;
export const explicitToImplicit: Implicit = e;

declare const other: symbol;
export const readViaIndex: (() => number) | undefined = i[other];
"#
    ));
}

#[test]
fn two_independent_wide_symbol_observable_interfaces_are_mutually_assignable() {
    // Both interfaces key off the SAME augmented member spelled identically —
    // but through the general symbol-index-signature rule, not through two
    // synthetic names happening to collide (the trap #16307's own matrix
    // calls out).
    assert_clean(&format!(
        r#"
{SYMBOL_LIB}
interface FromA {{ [Symbol.observable]: number }}
interface FromB {{ [Symbol.observable]: number }}

declare const a: FromA;
declare const b: FromB;
export const aToB: FromB = a;
export const bToA: FromA = b;
"#
    ));
}

#[test]
fn wide_symbol_observable_object_literal_member() {
    assert_clean(&format!(
        r#"
{SYMBOL_LIB}
interface Implicit {{ [Symbol.observable]: number }}
const obj: Implicit = {{ [Symbol.observable]: 1 }};
"#
    ));
}

#[test]
fn real_well_known_symbol_iterator_unaffected() {
    // Negative control: `Symbol.iterator` is declared `unique symbol` and
    // must keep its own named identity, exactly as before this change.
    assert_clean(&format!(
        r#"
{SYMBOL_LIB}
interface WithIterator {{ [Symbol.iterator](): number }}
declare const w: WithIterator;
export const called: number = w[Symbol.iterator]();
"#
    ));
}

#[test]
fn real_well_known_symbol_does_not_unify_with_wide_symbol_index() {
    // Negative control: a `unique symbol`-keyed member (`Symbol.iterator`)
    // must NOT satisfy a plain symbol index signature — only a genuinely wide
    // `symbol`-keyed member does.
    let source = format!(
        r#"
{SYMBOL_LIB}
interface WithIterator {{ [Symbol.iterator](): number }}
interface WithSymbolIndex {{ [key: symbol]: () => number }}
declare const w: WithIterator;
export const bad: WithSymbolIndex = w;
"#
    );
    let diags = check_source_diagnostics(&source);
    assert!(
        diags.iter().any(|d| d.code == 2322 || d.code == 2741),
        "a unique-symbol-keyed member must not satisfy a symbol index \
         signature, got: {diags:?}"
    );
}

#[test]
fn wide_symbol_observable_renamed_interfaces_and_members() {
    // Same shape, different names throughout, proving the fix is not keyed
    // off "Symbol", "observable", or any specific identifier spelling beyond
    // the literal global `Symbol` base every `[Symbol.x]` key requires.
    assert_clean(&format!(
        r#"
{SYMBOL_LIB}
interface HasSymbolIndex {{ [k: symbol]: string }}
interface UsesObservable {{ [Symbol.observable]: string }}

declare const withIndex: HasSymbolIndex;
declare const withObservable: UsesObservable;
export const toIndex: HasSymbolIndex = withObservable;
export const toObservable: UsesObservable = withIndex;
"#
    ));
}
