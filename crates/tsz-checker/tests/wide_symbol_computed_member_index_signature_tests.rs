//! Adjacent-case matrix for issue #16307: a computed member keyed by a plain
//! (non-unique) `symbol` BINDING (`declare const s: symbol`) must route into
//! the containing type's symbol index signature, not mint a synthetic named
//! member. `Symbol.<member>` written as literal property-access syntax is a
//! DIFFERENT shape and stays a named `[Symbol.<member>]` member regardless of
//! `<member>`'s own declared kind on a (possibly user-augmented)
//! `SymbolConstructor` — `tsc`'s `isWellKnownSymbolSyntactically` decides
//! purely from the syntax, never the member's type.
use tsz_checker::CheckerOptions;
use tsz_checker::test_utils::{
    check_multi_file_with_libs, check_source_diagnostics, check_source_with_libs,
    load_default_lib_files,
};

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

#[test]
fn well_known_symbol_syntax_keeps_named_identity_even_with_a_wide_global_augmentation() {
    // Corrected adjacent case for #16307 (oracle-verified against pinned
    // `tsc` 7.0.2, both directions): the literal `Symbol.<member>` syntax —
    // unlike an identifier bound `: symbol` — is ALWAYS the well-known symbol
    // itself, regardless of what `<member>` is declared as on a (possibly
    // user-augmented) `SymbolConstructor`. `isWellKnownSymbolSyntactically`
    // decides this from the syntax alone, before tsc ever looks at the
    // member's type. So `[Symbol.observable]` mints the literal
    // `[Symbol.observable]` NAMED member even when the global augmentation
    // types `observable` as plain `symbol` — it does NOT fold into a symbol
    // index signature, and an interface built that way stays mutually
    // UN-assignable with a real `[key: symbol]` index-signature interface in
    // both directions, exactly like the real well-known (`Symbol.iterator`)
    // negative control below.
    let source = r#"
declare global {
  interface SymbolConstructor {
    readonly observable: symbol;
  }
}

interface Explicit { [key: symbol]: number }
declare const e: Explicit;

interface Implicit { [Symbol.observable]: number }
declare const i: Implicit;

export const implicitToExplicit: Explicit = i;
export const explicitToImplicit: Implicit = e;
"#;
    let libs = load_default_lib_files();
    let diags = check_source_with_libs(source, "test.ts", CheckerOptions::default(), &libs);
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2322) && codes.contains(&2741),
        "Symbol.observable must keep its own named identity even under a wide \
         global augmentation, so BOTH directions must mismatch (missing index \
         signature one way, missing named member the other); got: {diags:?}"
    );
}

#[test]
fn well_known_symbol_syntax_for_a_real_well_known_keeps_named_identity() {
    // Negative control: `Symbol.iterator` (and any genuine `unique
    // symbol`-typed `SymbolConstructor` member) must keep minting its own
    // literal `[Symbol.iterator]` named key, not fold into the wide-symbol
    // index-signature path the new global-augmentation leg added.
    let source = r#"
interface HasIterator { [Symbol.iterator](): number }
interface Other { other(): number }
declare const h: HasIterator;
export const bad: Other = h;
"#;
    let libs = load_default_lib_files();
    let diags = check_source_with_libs(source, "test.ts", CheckerOptions::default(), &libs);
    assert!(
        diags.iter().any(|d| d.code == 2322 || d.code == 2741),
        "Symbol.iterator must keep its own named identity, not unify with an \
         unrelated interface via a symbol index signature, got: {diags:?}"
    );
}

#[test]
fn issue_16307_xstate_cross_file_corpus_witness() {
    // The exact 4-file distillation of xstate's own `Symbol.observable`
    // interop convention from #16307: a `declare global` augmentation typed
    // plain `symbol`, an aliasing const annotated `typeof Symbol.observable`,
    // an interface keyed by the literal `[Symbol.observable]` syntax in one
    // file, and a class implementing it via the alias in another file. `tsc`
    // exits 0; this was tsz's original corpus witness (TS2420 + TS2741).
    let libs = load_default_lib_files();
    let files: &[(&str, &str)] = &[
        (
            "symbolObservable.ts",
            r#"
export const symbolObservable: typeof Symbol.observable = (() =>
  (typeof Symbol === 'function' && Symbol.observable) || '@@observable')() as any;
"#,
        ),
        (
            "types.ts",
            r#"
export interface InteropSubscribable { subscribe(o: (v: any) => void): { unsubscribe(): void } }
export interface InteropObservable { [Symbol.observable]: () => InteropSubscribable }
"#,
        ),
        (
            "actor.ts",
            r#"
import { symbolObservable } from './symbolObservable';
import type { InteropObservable, InteropSubscribable } from './types';
export class Actor implements InteropObservable {
  public [symbolObservable](): InteropSubscribable {
    return { subscribe: () => ({ unsubscribe() {} }) };
  }
}
declare function want(o: InteropObservable): void;
declare const a: Actor;
want(a);
"#,
        ),
        (
            "index.ts",
            r#"
export * from './types';
export * from './actor';
declare global { interface SymbolConstructor { readonly observable: symbol } }
"#,
        ),
    ];
    let diags = check_multi_file_with_libs(files, "actor.ts", CheckerOptions::default(), &libs);
    assert!(
        diags.is_empty(),
        "expected exit 0 like tsc for the xstate Symbol.observable cross-file \
         witness, got: {diags:?}"
    );
}

#[test]
fn well_known_symbol_syntax_direct_on_class_member_keeps_named_identity() {
    // Corrected adjacent case for #16307 (oracle-verified against pinned
    // `tsc` 7.0.2): a class member keyed DIRECTLY by `[Symbol.observable]`
    // (not via an identifier alias) mints its own named `[Symbol.observable]`
    // member — same rule as the interface case above — even though the
    // global augmentation types `observable` as plain `symbol`. It does NOT
    // structurally satisfy an unrelated `[key: symbol]` index-signature
    // interface.
    let source = r#"
declare global {
  interface SymbolConstructor {
    readonly observable: symbol;
  }
}

interface Explicit { [key: symbol]: () => number }
declare const e: Explicit;

class Impl {
  [Symbol.observable](): number { return 1; }
}
declare const i: Impl;
export const implicitToExplicit: Explicit = i;
"#;
    let libs = load_default_lib_files();
    let diags = check_source_with_libs(source, "test.ts", CheckerOptions::default(), &libs);
    assert!(
        diags.iter().any(|d| d.code == 2322 || d.code == 2741),
        "Symbol.observable on a class member must keep its own named identity \
         even under a wide global augmentation, so the class must NOT \
         structurally satisfy an unrelated symbol-index-signature interface; \
         got: {diags:?}"
    );
}

#[test]
fn well_known_symbol_iterator_direct_on_class_method_keeps_named_identity() {
    // Negative control: a real well-known (`Symbol.iterator`, `unique
    // symbol`-typed) class method must keep its own named identity, not
    // fold into the symbol-index-signature leg the fix above added.
    let source = r#"
class HasIterator {
  [Symbol.iterator](): number { return 1; }
}
interface Other { other(): number }
declare const h: HasIterator;
export const bad: Other = h;
"#;
    let libs = load_default_lib_files();
    let diags = check_source_with_libs(source, "test.ts", CheckerOptions::default(), &libs);
    assert!(
        diags.iter().any(|d| d.code == 2322 || d.code == 2741),
        "Symbol.iterator must keep its own named identity, not unify with an \
         unrelated interface via a symbol index signature, got: {diags:?}"
    );
}
