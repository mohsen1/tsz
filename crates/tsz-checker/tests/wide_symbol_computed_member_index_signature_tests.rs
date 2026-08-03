//! Adjacent-case matrix for issue #16307: a computed member keyed by a plain
//! (non-unique) `symbol` must route into the containing type's symbol index
//! signature, not mint a synthetic named member.
use tsz_checker::test_utils::check_source_diagnostics;

fn assert_clean(source: &str) {
    let diags = check_source_diagnostics(source);
    assert!(diags.is_empty(), "expected exit 0 like tsc, got: {diags:?}");
}

#[test]
fn issue_16307_minimal_repro() {
    assert_clean(
        r#"
declare const s: symbol;
declare const other: symbol;

interface Explicit { [key: symbol]: number }
declare const e: Explicit;
export const readExplicit: number = e[other];

interface Implicit { [s]: number }
declare const i: Implicit;
export const readImplicit: number = i[other];
export const implicitToExplicit: Explicit = i;
export const explicitToImplicit: Implicit = e;
"#,
    );
}

#[test]
fn wide_symbol_computed_member_renamed_binders() {
    // Same structural shape as the minimal repro, with every identifier
    // renamed, proving the fix is not keyed off a specific binder name.
    assert_clean(
        r#"
declare const mySymbolKey: symbol;
declare const anotherSymbol: symbol;

interface WithExplicitIndex { [k: symbol]: string }
declare const withExplicit: WithExplicitIndex;
export const readViaExplicit: string = withExplicit[anotherSymbol];

interface WithComputedKey { [mySymbolKey]: string }
declare const withComputed: WithComputedKey;
export const readViaComputed: string = withComputed[anotherSymbol];
export const computedToExplicit: WithExplicitIndex = withComputed;
export const explicitToComputed: WithComputedKey = withExplicit;
"#,
    );
}

#[test]
fn two_independent_wide_symbol_keyed_interfaces_are_mutually_assignable() {
    // Two interfaces each computed-keyed by a DIFFERENT `symbol`-typed const
    // still describe the same member set (tsc widens both to the symbol
    // index signature, discarding per-declaration identity).
    assert_clean(
        r#"
declare const symA: symbol;
declare const symB: symbol;

interface FromA { [symA]: number }
interface FromB { [symB]: number }

declare const a: FromA;
declare const b: FromB;
export const aToB: FromB = a;
export const bToA: FromA = b;
"#,
    );
}

#[test]
fn wide_symbol_index_alongside_named_string_members() {
    assert_clean(
        r#"
declare const s: symbol;
interface Mixed {
    name: string;
    [s]: number;
}
declare const m: Mixed;
declare const other: symbol;
export const nameRead: string = m.name;
export const symbolRead: number = m[other];
"#,
    );
}

#[test]
fn wide_symbol_computed_method_signature_routes_to_symbol_index() {
    assert_clean(
        r#"
declare const s: symbol;
declare const other: symbol;
interface HasMethod { [s](): number }
declare const h: HasMethod;
export const called: number = h[other]();
"#,
    );
}

#[test]
fn unique_symbol_computed_member_keeps_distinct_identity_not_index() {
    // Negative control: a genuine `unique symbol` key must NOT be folded
    // into a symbol index signature — it keeps its own named identity, so
    // two interfaces keyed by two DIFFERENT unique symbols are NOT
    // mutually assignable.
    let source = r#"
declare const u1: unique symbol;
declare const u2: unique symbol;
interface FromU1 { [u1]: number }
interface FromU2 { [u2]: number }
declare const a: FromU1;
export const bad: FromU2 = a;
"#;
    let diags = check_source_diagnostics(source);
    assert!(
        diags.iter().any(|d| d.code == 2322 || d.code == 2741),
        "unique-symbol-keyed interfaces must not unify via a shared symbol \
         index signature, got: {diags:?}"
    );
}
