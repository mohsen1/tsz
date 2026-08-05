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
fn type_alias_wide_symbol_computed_member_routes_to_symbol_index() {
    // #16307 type-literal leg: the interface, object-literal and class paths
    // route a plain-`symbol`-keyed computed member into the containing type's
    // symbol index signature, but the CHECKER-side type-literal builder
    // (`get_type_from_type_literal`) minted a synthetic `__symbol_<file>_<sym>`
    // NAMED member instead — so a `type` alias with a `symbol`-keyed member was
    // NOT mutually assignable with an explicit `[k: symbol]` alias, the
    // placeholder key leaked into TS2741 text, and `t[other]` reported TS7053.
    // tsc 7.0.2: exit 0 on every line below.
    assert_clean(
        r#"
declare const s: symbol;
declare const other: symbol;
type Implicit = { [s]: number };
type Explicit = { [k: symbol]: number };
declare const i: Implicit;
declare const e: Explicit;
export const readImplicit: number = i[other];
export const implicitToExplicit: Explicit = i;
export const explicitToImplicit: Implicit = e;
"#,
    );
}

#[test]
fn inline_type_literal_wide_symbol_computed_member_routes_to_symbol_index() {
    // Same rule for an inline `{ [s]: T }` annotation written directly (no
    // alias), both assignability directions.
    assert_clean(
        r#"
declare const s: symbol;
declare const ex: { [k: symbol]: number };
declare const im: { [s]: number };
export const toExplicit: { [k: symbol]: number } = im;
export const toImplicit: { [s]: number } = ex;
"#,
    );
}

#[test]
fn two_independent_wide_symbol_keyed_type_aliases_are_mutually_assignable() {
    // Type-literal counterpart of the interface case: two aliases each keyed by
    // a DIFFERENT `symbol`-typed const still describe the same member set.
    assert_clean(
        r#"
declare const symA: symbol;
declare const symB: symbol;
type FromA = { [symA]: number };
type FromB = { [symB]: number };
declare const a: FromA;
declare const b: FromB;
export const aToB: FromB = a;
export const bToA: FromA = b;
"#,
    );
}

#[test]
fn type_alias_wide_symbol_index_alongside_named_members() {
    // A `symbol`-keyed computed member and ordinary named members coexist on the
    // same type literal: the named members stay named, the symbol member becomes
    // the symbol index signature.
    assert_clean(
        r#"
declare const s: symbol;
declare const other: symbol;
type Mixed = { name: string; [s]: number };
declare const m: Mixed;
export const nameRead: string = m.name;
export const symbolRead: number = m[other];
"#,
    );
}

#[test]
fn type_alias_wide_symbol_computed_member_renamed_binders() {
    // Anti-hardcoding: rename every identifier so nothing keys off a specific
    // binder name.
    assert_clean(
        r#"
declare const registryKey: symbol;
declare const lookupKey: symbol;
type Registry = { [registryKey]: string };
type SymbolBag = { [pk: symbol]: string };
declare const reg: Registry;
declare const bag: SymbolBag;
export const readReg: string = reg[lookupKey];
export const regToBag: SymbolBag = reg;
export const bagToReg: Registry = bag;
"#,
    );
}

#[test]
fn type_alias_distinct_wide_symbols_union_their_value_types() {
    // Distinct `symbol`-keyed computed members contribute to ONE symbol index
    // signature whose value type is the UNION of their values — they do not
    // collide. tsc reads `t[other]` here as `string | number` (exit 0); the
    // element access must not report TS7053.
    assert_clean(
        r#"
declare const s1: symbol;
declare const s2: symbol;
declare const other: symbol;
type T = { [s1]: number; [s2]: string };
declare const t: T;
export const r: string | number = t[other];
"#,
    );
}

#[test]
fn type_alias_wide_symbol_method_member_routes_to_symbol_index() {
    // Method form of the type-literal computed `symbol` key.
    assert_clean(
        r#"
declare const s: symbol;
declare const other: symbol;
type HasMethod = { [s](): number };
declare const h: HasMethod;
export const called: number = h[other]();
"#,
    );
}

#[test]
fn inline_type_literal_unique_symbol_member_keeps_named_identity() {
    // Negative control for the type-literal leg: a genuine `unique symbol` key
    // must NOT fold into a symbol index signature. Assigning a `[k: symbol]`
    // index-signature value to a `unique`-keyed type literal is a real mismatch,
    // exactly as tsc reports (the `unique symbol` member is missing).
    let source = r#"
declare const u: unique symbol;
type Unique = { [u]: number };
declare const ex: { [k: symbol]: number };
export const bad: Unique = ex;
"#;
    let diags = check_source_diagnostics(source);
    assert!(
        diags.iter().any(|d| d.code == 2322 || d.code == 2741),
        "a `unique symbol`-keyed type literal must keep its named identity, not \
         accept an arbitrary symbol index signature, got: {diags:?}"
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

// ── Distinct-value symbol members fold into a UNION index (#16307 residual) ────
//
// tsc folds several plain-`symbol` computed members on one interface into a
// single `[key: symbol]` index whose value type is the UNION of the members'
// value types. tsz's interface lowerer used to collapse a value-type mismatch
// to `error` (assignable to anything), producing a false TS7053 on the read and
// losing the union. These cases pin the corrected behavior; every one is
// `tsc` 6.0.2 exit 0 (or the reported TS2322) on the same source.

/// Read of a `symbol`-keyed member on an interface carrying two distinct-value
/// symbol members yields the union of both value types.
#[test]
fn interface_distinct_value_symbol_members_read_yields_union() {
    assert_clean(
        r#"
declare const s1: symbol;
declare const s2: symbol;
declare const other: symbol;
interface Implicit { [s1]: number; [s2]: string }
declare const i: Implicit;
export const readUnion: number | string = i[other];
"#,
    );
}

/// The folded index is a real `[key: symbol]` signature, so `keyof` surfaces
/// `symbol` and the interface is mutually assignable with an explicit
/// union-valued symbol index signature — in both directions.
#[test]
fn interface_distinct_value_symbol_members_mutual_with_explicit_index() {
    assert_clean(
        r#"
declare const s1: symbol;
declare const s2: symbol;
interface Implicit { [s1]: number; [s2]: string }
interface Explicit { [key: symbol]: number | string }
declare const i: Implicit;
declare const e: Explicit;
export const iToE: Explicit = i;
export const eToI: Implicit = e;
export const k: symbol = null as unknown as keyof Implicit;
"#,
    );
}

/// The fold is a genuine union, not an unconditional widen: reading and
/// assigning to a SINGLE member's narrow type is the TS2322 tsc reports, and
/// tsz must report it too (the pre-fix `error` collapse silently accepted it).
#[test]
fn interface_distinct_value_symbol_members_narrow_read_reports_ts2322() {
    let diags = check_source_diagnostics(
        r#"
declare const s1: symbol;
declare const s2: symbol;
interface Implicit { [s1]: number; [s2]: string }
declare const i: Implicit;
export const narrow: number = i[s1];
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2322),
        "reading a folded `number | string` symbol index into `number` must \
         report TS2322 like tsc, got: {diags:?}"
    );
}

/// Three distinct-value members fold into the three-way union, and the fold is
/// insensitive to the binder names (renamed identifiers must behave identically).
#[test]
fn interface_three_distinct_value_symbol_members_renamed_binders_union() {
    assert_clean(
        r#"
declare const alpha: symbol;
declare const beta: symbol;
declare const gamma: symbol;
interface Three { [alpha]: number; [beta]: string; [gamma]: boolean }
declare const t: Three;
export const r: number | string | boolean = t[alpha];
"#,
    );
}

/// An ordinary named member coexisting with the folded symbol index keeps its
/// own type; the symbol index still unions independently.
#[test]
fn interface_named_member_alongside_folded_symbol_index() {
    assert_clean(
        r#"
declare const s1: symbol;
declare const s2: symbol;
declare const other: symbol;
interface Mixed { readonly tag: boolean; [s1]: number; [s2]: string }
declare const m: Mixed;
export const viaSymbol: number | string = m[other];
export const viaName: boolean = m.tag;
"#,
    );
}

/// Method-signature members keyed by plain symbols fold the same way as
/// property members (the lowerer routes both through the implicit-symbol path).
#[test]
fn interface_distinct_value_symbol_method_members_union() {
    assert_clean(
        r#"
declare const s1: symbol;
declare const s2: symbol;
declare const other: symbol;
interface Callable { [s1](): number; [s2](): string }
declare const c: Callable;
export const via: (() => number) | (() => string) = c[other];
"#,
    );
}

// NOTE: the mixed case — an explicit `[key: symbol]: T` signature coexisting
// with an implicit computed-`symbol` member of a different value type
// (`interface M { [key: symbol]: number; [s1]: string }`) — is intentionally
// NOT covered here. tsc accepts it (exit 0), but tsz reports a separate,
// pre-existing false-positive `TS2411` from the checker-side index-member
// compatibility check (which reads the explicit `number` index directly),
// unrelated to the implicit-fold union this change owns. That divergence is
// left to its own fix so this suite pins only the implicit-fold behavior.
