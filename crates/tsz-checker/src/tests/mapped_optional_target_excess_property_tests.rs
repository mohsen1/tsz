//! Excess-property (TS2353) checking against a *concrete* homomorphic mapped
//! target that adds the optional modifier (`?` / `+?`).
//!
//! Structural rule: when a homomorphic mapped type over a non-generic source —
//! `{ [K in keyof A]?: A[K] }`, with or without an identity/renaming `as`
//! clause — is used as a contextual type, tsc evaluates it to a concrete object
//! and applies normal excess-property checking against the expanded shape. The
//! optional modifier turns the result into a "weak" object, but a fresh object
//! literal that specifies a key outside the source key set is still TS2353.
//!
//! tsz before fix: such a target was kept as a deferred `Mapped` in the declared
//! type, so the solver's object-literal excess path never ran against the
//! expanded shape and the checker's generic-mapped excess loop deferred it (the
//! evaluated object carries no free type parameters). The excess key was
//! silently accepted. A *generic* `Partial<T>`-style target must stay deferred,
//! since its key set is unknown until `T` is instantiated.
//!
//! Binder names are varied across cases so the fix is structural, not spelling
//! specific.

use crate::test_utils::check_source_strict_messages_without_missing_libs;

fn codes(source: &str) -> Vec<u32> {
    check_source_strict_messages_without_missing_libs(source)
        .into_iter()
        .map(|(code, _)| code)
        .collect()
}

/// Plain `?` optional homomorphic mapped target flags an excess key.
#[test]
fn plain_optional_mapped_target_flags_excess_property() {
    let diags = check_source_strict_messages_without_missing_libs(
        "type Src = { a: string; b: number };\n\
         const v: { [K in keyof Src]?: Src[K] } = { a: \"\", b: 1, zzz: 9 };",
    );
    let cs: Vec<u32> = diags.iter().map(|(c, _)| *c).collect();
    assert!(
        cs.contains(&2353),
        "excess key on optional mapped target must be TS2353; got {diags:?}"
    );
    // The display must show the expanded object, not the deferred mapped form.
    let msg = diags
        .iter()
        .find(|(c, _)| *c == 2353)
        .map(|(_, m)| m.clone())
        .unwrap_or_default();
    assert!(
        msg.contains("a?:") && msg.contains("b?:") && !msg.contains("in keyof"),
        "TS2353 must render the expanded object shape, got: {msg}"
    );
}

/// Explicit `+?` optional modifier behaves the same as `?`.
#[test]
fn plus_optional_mapped_target_flags_excess_property() {
    let cs = codes(
        "type Box = { x: string; y: number };\n\
         const v: { [P in keyof Box]+?: Box[P] } = { x: \"\", y: 1, extra: true };",
    );
    assert!(
        cs.contains(&2353),
        "excess key on `+?` mapped target must be TS2353; got {cs:?}"
    );
}

/// An identity-renaming `as K` clause is still homomorphic, so excess checking
/// applies against the (unchanged) key set.
#[test]
fn identity_rename_optional_mapped_target_flags_excess_property() {
    let cs = codes(
        "type Rec = { one: string; two: number };\n\
         const v: { [K in keyof Rec as K]?: Rec[K] } = { one: \"\", two: 1, three: 3 };",
    );
    assert!(
        cs.contains(&2353),
        "excess key on `as K` optional mapped target must be TS2353; got {cs:?}"
    );
}

/// A fresh object literal that stays within the source key set is clean — the
/// fix must not introduce false positives.
#[test]
fn optional_mapped_target_without_excess_is_clean() {
    let cs = codes(
        "type Src = { a: string; b: number };\n\
         const v: { [K in keyof Src]?: Src[K] } = { a: \"\", b: 1 };",
    );
    assert!(
        cs.is_empty(),
        "in-bounds object literal against optional mapped target must be clean; got {cs:?}"
    );
}

/// The empty object literal satisfies an all-optional mapped target.
#[test]
fn empty_literal_satisfies_optional_mapped_target() {
    let cs = codes(
        "type Src = { a: string; b: number };\n\
         const v: { [K in keyof Src]?: Src[K] } = {};",
    );
    assert!(
        cs.is_empty(),
        "empty literal against all-optional mapped target must be clean; got {cs:?}"
    );
}

/// A generic `Partial<T>`-style optional mapped target keeps its key set
/// deferred; an object literal with an unknown-at-definition key must NOT be
/// reported as excess (tsc accepts it until `T` is instantiated).
#[test]
fn generic_optional_mapped_target_does_not_flag_excess() {
    let cs = codes(
        "type Part<T> = { [K in keyof T]?: T[K] };\n\
         function f<T>(): void {\n\
           const v: Part<T> = { anything: 1 } as Part<T>;\n\
           void v;\n\
         }",
    );
    assert!(
        !cs.contains(&2353),
        "generic deferred mapped target must not flag excess; got {cs:?}"
    );
}

/// `-?` (required) optional-removal mapped targets are not weak; excess on a
/// required-shaped target is still flagged (regression guard for the shared
/// path).
#[test]
fn required_mapped_target_still_flags_excess_property() {
    let cs = codes(
        "type Src = { a?: string; b?: number };\n\
         const v: { [K in keyof Src]-?: Src[K] } = { a: \"\", b: 1, gone: 0 };",
    );
    assert!(
        cs.contains(&2353),
        "excess key on `-?` mapped target must be TS2353; got {cs:?}"
    );
}
