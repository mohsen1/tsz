//! Regression coverage for #13609: a type parameter's optional `default`
//! annotation (and its `const` modifier) must not fragment relation/identity.
//!
//! `tsc` never distinguishes type parameters by their `default` or by `const`
//! in relation or identity (`compareTypeParametersIdentical` compares only the
//! constraint). tsz previously interned `TypeData::TypeParameter` over the whole
//! `TypeParamInfo` — including `default` and `is_const` — so two structurally
//! identical parameters that differed only in `default` (or only in `const`)
//! received distinct `TypeId`s. A wrapping `Application`/`Conditional` then
//! failed the relation's reflexive/identity short-circuit and fell into a
//! resolver-state-dependent structural walk, surfacing as a false `TS2322`
//! (#13609; the original ofetch / `$Fetch["raw"]`-vs-`onError` witness). The
//! fix normalizes type-parameter identity in the solver canonicalizer:
//! `canonical_type_param` canonicalizes the constraint and drops both `default`
//! and `is_const`, while the interned parameter keeps its `default` so bare
//! instantiation still applies it.
//!
//! These witnesses pin the user-facing parity end-to-end through the checker.
//! Binder names are varied (never the canonical `R`/`T`/`U` spellings the
//! original report used) so the rule follows the type-parameter *shape*, not a
//! literal name — per the anti-hardcoding test discipline.

use crate::CheckerOptions;
use crate::test_utils::{check_source_with_libs, diagnostic_codes, load_default_lib_files};

fn codes(source: &str) -> Vec<u32> {
    diagnostic_codes(&check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions::default(),
        &load_default_lib_files(),
    ))
}

fn assert_clean(source: &str) {
    let found = codes(source);
    assert!(
        found.is_empty(),
        "expected no diagnostics, got {found:?} for source:\n{source}"
    );
}

fn assert_has_code(source: &str, code: u32) {
    let found = codes(source);
    assert!(
        found.contains(&code),
        "expected TS{code}, got {found:?} for source:\n{source}"
    );
}

/// Two generic function types that differ *only* in whether their type
/// parameter declares a `default` relate as identical in both directions.
#[test]
fn default_present_vs_absent_function_types_relate_both_ways() {
    assert_clean(
        r#"
type Defaulted   = <Pick_ extends string = "json">(seed: Pick_) => Pick_;
type Undefaulted = <Sift  extends string>(seed: Sift) => Sift;

declare const withDefault: Defaulted;
declare const without: Undefaulted;

const toUndefaulted: Undefaulted = withDefault;
const toDefaulted: Defaulted = without;
"#,
    );
}

/// A `const` type parameter and an otherwise-identical non-`const` parameter
/// relate as identical: `tsc` ignores `const` in identity just like `default`.
#[test]
fn const_vs_non_const_type_params_relate_both_ways() {
    assert_clean(
        r#"
type Constish    = <const Holder>(box: Holder) => Holder;
type Plainish     = <Crate>(box: Crate) => Crate;

declare const constish: Constish;
declare const plainish: Plainish;

const toPlain: Plainish = constish;
const toConst: Constish = plainish;
"#,
    );
}

/// The `default` is still applied when the alias is referenced with no type
/// argument: dropping `default` from identity must not drop it from defaulting.
#[test]
fn default_is_still_applied_on_bare_instantiation() {
    assert_clean(
        r#"
type Channel<Mode extends string = "json"> = Mode;
const stays: Channel = "json";
"#,
    );
}

/// ...and bare instantiation rejects a value outside the applied default,
/// proving the default genuinely resolves to `"json"` (not erased to `string`).
#[test]
fn default_applied_on_bare_instantiation_rejects_off_default_value() {
    assert_has_code(
        r#"
type Channel<Mode extends string = "json"> = Mode;
const bad: Channel = "csv";
"#,
        2322,
    );
}

/// A still-generic conditional whose check type is a type parameter relates to
/// the same conditional reached through a parameter that differs only in its
/// `default` — the shape closest to the original ofetch witness.
#[test]
fn deferred_conditional_over_param_differing_only_in_default_relates() {
    assert_clean(
        r#"
type WhenDefaulted<Mode extends string = "json"> = Mode extends "json" ? 1 : 2;
type WhenPlain<Kind extends string>               = Kind extends "json" ? 1 : 2;

declare const defaulted: <Mode extends string = "json">() => WhenDefaulted<Mode>;
const asPlain: <Kind extends string>() => WhenPlain<Kind> = defaulted;
"#,
    );
}

/// The default may itself reference an earlier type parameter; two such generic
/// signatures that differ only in the presence of that defaulting clause still
/// relate as identical.
#[test]
fn param_relative_default_does_not_fragment_identity() {
    assert_clean(
        r#"
type WithRelDefault = <Seed, Echo extends Seed = Seed>(seed: Seed, echo: Echo) => Echo;
type NoRelDefault   = <Root, Leaf extends Root>(seed: Root, echo: Leaf) => Leaf;

declare const withRel: WithRelDefault;
const asNoRel: NoRelDefault = withRel;
"#,
    );
}
