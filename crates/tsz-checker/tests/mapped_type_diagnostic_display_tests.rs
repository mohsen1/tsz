//! Optional/readonly modifier display in diagnostics for mapped types.
//!
//! Structural rule: when a mapped type is expanded for display in an error message
//! (e.g., the target type in an excess-property or assignability diagnostic), the
//! `optional` (`?`) and `readonly` modifiers on its materialized properties must
//! match the modifiers that tsc would show.
//!
//! The rule covers three distinct modifier sources:
//! 1. Explicit Add modifier (`?`/`readonly`): `{ [K in keyof T]?: T[K] }` — all
//!    properties become optional.
//! 2. Inherited from source (homomorphic, `None` modifier): `{ [K in keyof T]: T[K] }`
//!    maps over a source with optional/readonly properties — those properties stay
//!    optional/readonly in the output.
//! 3. Explicit Remove modifier (`-?`/`-readonly`): all properties lose the modifier
//!    regardless of the source.
//!
//! The checker-level tests focus on the EVALUATED type shown in diagnostics. For
//! named type aliases, tsc and tsz both prefer the alias form (e.g., `Partial<T>`)
//! over the expanded form. These tests verify the correct modifier behavior by
//! checking the expanded type text in cases where the alias is transparent (e.g.,
//! `Identity<T>`) or via the TS2327 elaboration path.

use tsz_checker::CheckerOptions;
use tsz_checker::test_utils::{check_source, check_source_diagnostics};

// =============================================================================
// Excess-property path — identity homomorphic (`T[K]` template)
// The `Identity<T>` alias is transparent (evaluates to same type), so the
// expanded form `{ prop?: ... }` is shown in diagnostics.
// =============================================================================

#[test]
fn identity_mapped_optional_source_shows_question_mark_in_excess_property_error() {
    // `{ [K in keyof T]: T[K] }` with an optional source property.
    // `Identity<{ val?: number }>` evaluates to `{ val?: number | undefined }`.
    let diags = check_source_diagnostics(
        r#"
type Identity<T> = { [K in keyof T]: T[K] };
declare function f(x: Identity<{ val?: number }>): void;
f({ val: 1, extra: true });
"#,
    );
    // The excess-property error (TS2353) should mention `val?:` in the type display,
    // since the identity mapped type preserves the source property's optionality.
    let found = diags
        .iter()
        .any(|d| d.message_text.contains("val?:") || d.message_text.contains("val?: "));
    assert!(
        found,
        "expected 'val?:' in excess-property message for identity-mapped optional source; \
         got {diags:?}"
    );
}

#[test]
fn identity_mapped_optional_source_name_independent() {
    // Same shape with a different property name — proves the fix is structural.
    let diags = check_source_diagnostics(
        r#"
type Identity<T> = { [K in keyof T]: T[K] };
declare function f(x: Identity<{ greeting?: string }>): void;
f({ greeting: "hi", surplus: 1 });
"#,
    );
    let found = diags
        .iter()
        .any(|d| d.message_text.contains("greeting?:") || d.message_text.contains("greeting?: "));
    assert!(
        found,
        "expected 'greeting?:' in excess-property message; got {diags:?}"
    );
}

#[test]
fn identity_mapped_readonly_source_shows_readonly_in_excess_property_error() {
    // `{ [K in keyof T]: T[K] }` preserves `readonly` from the source.
    let diags = check_source_diagnostics(
        r#"
type Identity<T> = { [K in keyof T]: T[K] };
declare function f(x: Identity<{ readonly val: number }>): void;
f({ val: 1, extra: true });
"#,
    );
    let found = diags.iter().any(|d| {
        d.message_text.contains("readonly val:") || d.message_text.contains("readonly val ")
    });
    assert!(
        found,
        "expected 'readonly val' in excess-property message for identity-mapped readonly source; \
         got {diags:?}"
    );
}

#[test]
fn identity_mapped_readonly_source_name_independent() {
    let diags = check_source_diagnostics(
        r#"
type Identity<T> = { [K in keyof T]: T[K] };
declare function f(x: Identity<{ readonly score: number }>): void;
f({ score: 99, extra: true });
"#,
    );
    let found = diags.iter().any(|d| {
        d.message_text.contains("readonly score:") || d.message_text.contains("readonly score ")
    });
    assert!(
        found,
        "expected 'readonly score' in excess-property message; got {diags:?}"
    );
}

// =============================================================================
// TS2327 elaboration path — optional-from-mapped requires the `?` to be visible
// in the type string shown in the "is optional in type '_'" message.
// =============================================================================

#[test]
fn identity_mapped_optional_ts2327_source_type_shows_question_mark() {
    // When the source of a TS2322 is an identity-mapped type with an optional property,
    // the TS2327 elaboration says "Property 'x' is optional in type '...' but required
    // in type '...'". The source type string should show `x?:`.
    //
    // The TS2327 "is optional ... but required" elaboration is only tsc's output
    // under `exactOptionalPropertyTypes`; without it the optional source property
    // contributes `T | undefined` and tsc reports the plain type-incompatibility
    // chain instead. (Verified against tsc 6.0.)
    let diags = check_source(
        r#"
type Identity<T> = { [K in keyof T]: T[K] };
declare let a: Identity<{ x?: number }>;
declare let b: { x: number };
b = a;
"#,
        "test.ts",
        CheckerOptions {
            exact_optional_property_types: true,
            ..CheckerOptions::default()
        },
    );
    // At minimum: a TS2322 must exist with a TS2327 elaboration.
    let has_ts2327 = diags.iter().any(|d| {
        d.code == 2322
            && d.related_information.iter().any(|info| {
                info.message_text.contains("is optional in type")
                    && info.message_text.contains("x?:")
            })
    });
    assert!(
        has_ts2327,
        "expected TS2322 + TS2327 elaboration with 'x?:' in source type; got {diags:?}"
    );
}

// =============================================================================
// Negative: non-homomorphic mapped type should NOT inherit source optionality
// =============================================================================

#[test]
fn non_homomorphic_mapped_type_does_not_inherit_optionality() {
    // `{ [K in "val"]: number }` is NOT homomorphic (constraint is a literal, not keyof T).
    // Its properties should not carry `?` even if a source with optional properties exists.
    let diags = check_source_diagnostics(
        r#"
declare function f(x: { [K in "val"]: number }): void;
f({ val: 1, extra: true });
"#,
    );
    // The type in the error should NOT show `val?:`.
    let has_question_mark = diags
        .iter()
        .any(|d| d.message_text.contains("val?:") || d.message_text.contains("val?: "));
    assert!(
        !has_question_mark,
        "non-homomorphic mapped type must not gain spurious '?'; got {diags:?}"
    );
}

// =============================================================================
// Remove-optional (`-?`) clears optionality even for optional source properties
// =============================================================================

#[test]
fn required_mapped_type_ts2327_does_not_emit_optional_elaboration() {
    // `{ [K in keyof T]-?: T[K] }` (Required<T>-style) — the source property
    // is required in the resulting type, so there must be no TS2327 elaboration.
    // The source has an optional property but the Required-style mapped type removes it.
    let diags = check_source_diagnostics(
        r#"
type Requiredish<T> = { [K in keyof T]-?: T[K] };
declare let a: Requiredish<{ val?: number }>;
declare let b: { val: number; extra: string };
b = a;
"#,
    );
    // TS2322 for missing `extra` is expected. But there must be NO TS2327 saying `val` is optional.
    let has_optional_elaboration = diags.iter().any(|d| {
        d.related_information.iter().any(|info| {
            info.message_text.contains("val") && info.message_text.contains("is optional in type")
        })
    });
    assert!(
        !has_optional_elaboration,
        "remove-optional mapped type must not emit TS2327 for val; got {diags:?}"
    );
}
