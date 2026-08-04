//! A computed key written through a `typeof Symbol.<member>` alias keys the
//! same member as `[Symbol.<member>]` written inline.
//!
//! Structural rule, verified against the pinned `tsc` oracle: `typeof
//! Symbol.iterator` IS the well-known symbol's own type (`tsc` reports it as
//! `unique symbol`), so `declare const it: typeof Symbol.iterator; interface T
//! { [it]: V }` declares the canonical `[Symbol.iterator]` member — exactly
//! what `interface U { [Symbol.iterator]: V }` declares. Before this leg the
//! alias fell through to binding-identity naming (`__unique_<id>`), so `T` and
//! `U` described different member sets and `T` reported a missing
//! `[Symbol.iterator]` against `U` where `tsc` is clean.
//!
//! One boundary the rule must not cross, oracle-checked: an UNANNOTATED
//! `const it = Symbol.iterator` widens to `symbol`, so it keys a symbol index
//! signature, not the well-known member. `tsc` reports the mismatch against
//! an inline `[Symbol.iterator]` member; so must tsz.
//!
//! `typeof Symbol.<member>` is `unique symbol` to `tsc` — and so names the
//! canonical `[Symbol.<member>]` member — regardless of `<member>`'s own
//! declared kind on a (possibly user-augmented) `SymbolConstructor`. A PLAIN
//! (non-`unique`) augmented member is NOT an exception: `tsc`'s discriminator
//! is the syntactic reach to the global `Symbol`, never the member's declared
//! type (#16307).
//!
//! Binder names vary across cases on purpose: the rule reads the declaration's
//! annotation, never the identifier the user chose.

use std::sync::Arc;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::{CheckerOptions, ScriptTarget};
use tsz_checker::test_utils::check_source_with_libs;

fn load_symbol_lib_files_for_test() -> Vec<Arc<LibFile>> {
    tsz_checker::test_utils::load_compiled_lib_files(&[
        "lib.es5.d.ts",
        "lib.es2015.symbol.d.ts",
        "lib.es2015.symbol.wellknown.d.ts",
    ])
}

fn codes(source: &str) -> Vec<u32> {
    let lib_files = load_symbol_lib_files_for_test();
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2022,
            ..CheckerOptions::default()
        },
        &lib_files,
    )
    .into_iter()
    .map(|diagnostic| diagnostic.code)
    .collect()
}

fn assert_no_missing_property(source: &str) {
    let result = codes(source);
    assert!(
        !result.contains(&2741) && !result.contains(&2345) && !result.contains(&2322),
        "expected the aliased well-known key to name the same member, got: {result:?}"
    );
}

// 1. Reported repro: an interface keyed through the alias satisfies one keyed
//    inline.
#[test]
fn type_query_alias_keys_the_same_member_as_the_inline_well_known() {
    assert_no_missing_property(
        r#"
declare const it: typeof Symbol.iterator;
interface Aliased { [it]: () => number }
interface Inline { [Symbol.iterator]: () => number }
declare const aliased: Aliased;
declare function want(value: Inline): void;
want(aliased);
"#,
    );
}

// 2. The other direction: inline-keyed source against an alias-keyed target.
#[test]
fn inline_well_known_key_satisfies_a_type_query_alias_target() {
    assert_no_missing_property(
        r#"
declare const marker: typeof Symbol.iterator;
interface Aliased { [marker]: () => number }
interface Inline { [Symbol.iterator]: () => number }
declare const inline: Inline;
declare function want(value: Aliased): void;
want(inline);
"#,
    );
}

// 3. Renamed binders and a different well-known member — the rule is
//    structural, not a match on any particular spelling.
#[test]
fn renamed_alias_of_another_well_known_member_is_structural() {
    assert_no_missing_property(
        r#"
declare const asyncKey: typeof Symbol.asyncIterator;
interface ThroughAlias { [asyncKey]: () => number }
interface WrittenOut { [Symbol.asyncIterator]: () => number }
declare const source: ThroughAlias;
declare function want(value: WrittenOut): void;
want(source);
"#,
    );
}

// 4. A class member keyed through the alias satisfies an interface that keys
//    the member inline — the alias must reach every declaration form, not just
//    interface lowering.
#[test]
fn class_member_keyed_through_the_alias_matches_an_inline_interface_member() {
    assert_no_missing_property(
        r#"
declare const slot: typeof Symbol.iterator;
class Holder { [slot](): number { return 1; } }
interface Shape { [Symbol.iterator]: () => number }
declare const holder: Holder;
declare function want(value: Shape): void;
want(holder);
"#,
    );
}

// 5. Two aliases of the SAME well-known member agree with each other, even
//    though they are distinct bindings — binding identity must not survive.
#[test]
fn two_distinct_aliases_of_one_well_known_member_agree() {
    assert_no_missing_property(
        r#"
declare const first: typeof Symbol.iterator;
declare const second: typeof Symbol.iterator;
interface Producer { [first]: () => number }
interface Consumer { [second]: () => number }
declare const producer: Producer;
declare function want(value: Consumer): void;
want(producer);
"#,
    );
}

// 6. Negative: aliases of DIFFERENT well-known members still describe
//    different members, so the mismatch is still reported.
#[test]
fn aliases_of_different_well_known_members_still_mismatch() {
    let result = codes(
        r#"
declare const iterKey: typeof Symbol.iterator;
declare const asyncIterKey: typeof Symbol.asyncIterator;
interface HasIterator { [iterKey]: () => number }
interface WantsAsync { [asyncIterKey]: () => number }
declare const value: HasIterator;
declare function want(target: WantsAsync): void;
want(value);
"#,
    );
    assert!(
        result.contains(&2345) || result.contains(&2741),
        "two different well-known members must not collapse into one key, got: {result:?}"
    );
}

// 7. Boundary: an UNANNOTATED `const` initialized from `Symbol.iterator`
//    widens to `symbol`, so it keys an index signature rather than the
//    well-known member. tsc reports the mismatch; the alias leg must not
//    swallow it.
#[test]
fn unannotated_const_initialized_from_symbol_iterator_stays_wide() {
    let result = codes(
        r#"
const inferred = Symbol.iterator;
interface Widened { [inferred]: () => number }
interface Inline { [Symbol.iterator]: () => number }
declare const widened: Widened;
declare function want(value: Inline): void;
want(widened);
"#,
    );
    assert!(
        result.contains(&2345) || result.contains(&2741),
        "an unannotated Symbol.iterator const widens to `symbol` and keys an index signature, got: {result:?}"
    );
}

// 8. Boundary (oracle-verified against pinned `tsc` 7.0.2, corrected from an
//    earlier premise): a `typeof Symbol.<member>` alias for a PLAIN
//    (non-`unique`) augmented member is STILL the well-known member, not a
//    wide-`symbol` binding — `typeof Symbol.observable` types as `unique
//    symbol` to `tsc` even when `SymbolConstructor.observable` is declared
//    plain `symbol`. A class keyed through it therefore does NOT structurally
//    satisfy an interface keyed off an unrelated wide `symbol` binding: `tsc`
//    reports the missing index signature (`Argument of type 'Emitter' is not
//    assignable to parameter of type 'Sink'. Index signature for type
//    'symbol' is missing in type 'Emitter'.`).
#[test]
fn type_query_alias_of_a_wide_augmented_member_keeps_named_identity() {
    let result = codes(
        r#"
interface SymbolConstructor { readonly observable: symbol }
declare const observableKey: typeof Symbol.observable;
declare const unrelatedKey: symbol;
class Emitter { [observableKey](): number { return 1; } }
interface Sink { [unrelatedKey]: () => number }
declare const emitter: Emitter;
declare function want(value: Sink): void;
want(emitter);
"#,
    );
    assert!(
        result.contains(&2345) || result.contains(&2741),
        "a typeof Symbol.<member> alias for a plain-`symbol`-augmented member \
         must keep its well-known named identity, not fold into an unrelated \
         wide-`symbol` index signature, got: {result:?}"
    );
}
