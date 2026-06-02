//! Regression coverage for bug-family #12174.
//!
//! Structural rule: when a mapped type uses an identity `as K` remap clause
//! (`{ [K in keyof T as K]: T[K] }`) with no explicit modifier directives,
//! tsz must carry optional and readonly modifiers from the source type through
//! to the result — matching `tsc` behavior and matching what the no-`as`-clause
//! form (`{ [K in keyof T]: T[K] }`) already does.
//!
//! The absence of this equivalence produces false-positive TS2741 / TS2322
//! diagnostics on project rows where library types use identity remapped mapped
//! types (e.g., utility types in zod, rxjs, nextjs, kysely).

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_source;

fn check(source: &str) -> Vec<Diagnostic> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    )
}

fn codes(diags: &[Diagnostic]) -> Vec<u32> {
    diags.iter().map(|d| d.code).collect()
}

fn has(diags: &[Diagnostic], code: u32) -> bool {
    diags.iter().any(|d| d.code == code)
}

// ──────────────────────────────────────────────────────────────────────────
// Baseline: no-as-clause identity mapped type must preserve optionality.
// ──────────────────────────────────────────────────────────────────────────

/// Structural rule baseline: `{ [K in keyof T]: T[K] }` (no as-clause) must
/// preserve the source's optional modifier so that an object literal missing
/// an optional property is accepted without a TS2741 diagnostic.
#[test]
fn no_as_clause_identity_mapped_optional_property_is_skippable() {
    let diags = check(
        r#"
type Identity<T> = { [K in keyof T]: T[K] };
type Source = { a?: number; b: string };
type Wrapped = Identity<Source> & { c: boolean };

const w: Wrapped = { b: "hello", c: true };
"#,
    );
    assert!(
        !has(&diags, 2741),
        "Identity<Source> (no as-clause): optional 'a' must not require presence; got: {diags:#?}"
    );
    assert!(
        !has(&diags, 2322),
        "Identity<Source> (no as-clause): no TS2322 expected; got: {diags:#?}"
    );
}

// ──────────────────────────────────────────────────────────────────────────
// Primary: identity `as K` mapped type must behave the same as no-as-clause.
// ──────────────────────────────────────────────────────────────────────────

/// Identity `as K` remap must treat the type as homomorphic and carry source
/// optional modifiers forward, the same as the no-`as`-clause form.
///
/// tsc behavior: `{ b: "hello", c: true }` is valid for `Wrapped` because `a`
/// is optional in `Source` and is therefore optional in `Rename<Source>` and
/// in the intersection `Rename<Source> & { c: boolean }`.
#[test]
fn identity_as_clause_mapped_optional_property_is_skippable() {
    let diags = check(
        r#"
type Rename<T> = { [K in keyof T as K]: T[K] };
type Source = { a?: number; b: string };
type Wrapped = Rename<Source> & { c: boolean };

const w: Wrapped = { b: "hello", c: true };
"#,
    );
    assert!(
        !has(&diags, 2741),
        "Rename<Source> (identity as K): optional 'a' must not require presence; got: {diags:#?}"
    );
    assert!(
        !has(&diags, 2322),
        "Rename<Source> (identity as K): no TS2322 expected; got: {diags:#?}"
    );
}

/// Renamed iteration variable (`P` instead of `K`) must also preserve optional.
/// The structural rule is name-agnostic.
#[test]
fn identity_as_clause_with_renamed_iter_var_preserves_optional() {
    let diags = check(
        r#"
type Rename<T> = { [P in keyof T as P]: T[P] };
type Source = { a?: number; b: string };
type Wrapped = Rename<Source> & { c: boolean };

const w: Wrapped = { b: "hello", c: true };
"#,
    );
    assert!(
        !has(&diags, 2741),
        "Rename<Source> (identity as P): optional 'a' must not require presence; got: {diags:#?}"
    );
}

/// Required properties must still require presence through identity `as K`.
/// This validates that the fix does not accidentally make all properties optional.
#[test]
fn identity_as_clause_required_property_is_still_required() {
    let diags = check(
        r#"
type Rename<T> = { [K in keyof T as K]: T[K] };
type Source = { a?: number; b: string };
type Wrapped = Rename<Source> & { c: boolean };

// b is required, c is required — missing b must produce TS2741
const bad: Wrapped = { c: true };
"#,
    );
    assert!(
        has(&diags, 2741) || has(&diags, 2322),
        "Rename<Source> (identity as K): missing required 'b' must produce an error; got: {:?}",
        codes(&diags)
    );
}

/// All three properties provided including the optional one: should accept.
#[test]
fn identity_as_clause_all_properties_provided_accepts() {
    let diags = check(
        r#"
type Rename<T> = { [K in keyof T as K]: T[K] };
type Source = { a?: number; b: string };
type Wrapped = Rename<Source> & { c: boolean };

const w: Wrapped = { a: 42, b: "hello", c: true };
"#,
    );
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2741 || d.code == 2322)
        .collect();
    assert!(
        errors.is_empty(),
        "Rename<Source> (identity as K): all properties provided should have no errors; got: {diags:#?}"
    );
}

// ──────────────────────────────────────────────────────────────────────────
// Readonly modifier preservation.
// ──────────────────────────────────────────────────────────────────────────

/// Identity `as K` must also preserve `readonly` from the source.
///
/// Writing to a readonly property through an identity-remapped mapped type
/// must be rejected with TS2540, the same as through the no-`as`-clause form.
#[test]
fn identity_as_clause_readonly_property_is_preserved() {
    let diags = check(
        r#"
type Rename<T> = { [K in keyof T as K]: T[K] };
type Source = { readonly x: number };
type Wrapped = Rename<Source>;

declare const w: Wrapped;
// Assigning to a readonly property must be rejected.
w.x = 5;
"#,
    );
    assert!(
        has(&diags, 2540),
        "Rename<Source> (identity as K): assignment to readonly 'x' must be TS2540; got: {diags:#?}"
    );
}

/// Baseline: the no-as-clause form must also produce TS2540.
/// This test ensures parity between the two forms.
#[test]
fn no_as_clause_readonly_property_is_preserved() {
    let diags = check(
        r#"
type Identity<T> = { [K in keyof T]: T[K] };
type Source = { readonly x: number };
type Wrapped = Identity<Source>;

declare const w: Wrapped;
w.x = 5;
"#,
    );
    assert!(
        has(&diags, 2540),
        "Identity<Source> (no as-clause): assignment to readonly 'x' must be TS2540; got: {diags:#?}"
    );
}
