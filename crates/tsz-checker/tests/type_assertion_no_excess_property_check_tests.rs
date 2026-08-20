//! A type assertion (`{ … } as T`, `<T>{ … }`) does NOT run an
//! excess-property check — this locks in tsc parity against a plausible-but-
//! wrong "assertions should report TS2353 for excess properties" change.
//!
//! It is tempting to route an asserted fresh object literal through the same
//! excess-property machinery as an assignment (the concise-arrow-body doc even
//! says "an asserted body performs its own check"). But tsc does NOT report
//! TS2353 for an excess property in a type assertion — an asserted object
//! literal that is otherwise assignable is accepted, and one that does not
//! overlap yields TS2352 ("conversion may be a mistake"), never TS2353.
//!
//! Oracle evidence (pinned typescript 7.0.2, the shapes these fences mirror):
//! - `contextualTyping35.ts`: `<{ id: number; }>{ id: 4, name: "as" }` → no error.
//! - `overloadResolutionOverNonCTObjectLit.ts`: `<IToken>{ …excess… }` → no error.
//! - `noImplicitAnyInCastExpression.ts`: `<IFoo>{ c: null }` → only TS2352.
//!
//! Adding an assertion-time excess check regresses exactly those rows, so these
//! fences guard the boundary. (The `arrayCast.ts` conformance row, where the
//! pinned tsgo oracle reports TS2353 for a fresh array-literal cast, is a
//! tsgo-vs-classic-tsc array-element divergence — classic tsc reports TS2352
//! with the excess as a nested chain line, which is what tsz matches — and is
//! tracked separately, not addressed by widening the object path here.)

use tsz_checker::context::CheckerOptions;

fn diags(source: &str, strict: bool) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        strict,
        strict_null_checks: strict,
        ..Default::default()
    };
    tsz_checker::test_utils::check_source(source, "test.ts", options)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

fn codes(diags: &[(u32, String)]) -> Vec<u32> {
    diags.iter().map(|(c, _)| *c).collect()
}

#[test]
fn angle_bracket_assignable_source_with_excess_is_accepted() {
    // `{ id, name }` is assignable to `{ id }` (has `id`); the excess `name`
    // is NOT reported for an assertion. Mirrors `contextualTyping35.ts`.
    assert!(
        diags(
            r#"const foo = <{ id: number }>{ id: 4, name: "as" };"#,
            false
        )
        .is_empty(),
        "an assertion must not excess-check an assignable object-literal source"
    );
}

#[test]
fn angle_bracket_assignable_source_with_excess_is_accepted_strict() {
    // The absence of an excess check is not a strict-mode artifact.
    assert!(
        diags(
            r#"const foo = <{ id: number }>{ id: 4, name: "as" };"#,
            true
        )
        .is_empty(),
        "strict mode must not introduce an assertion excess check"
    );
}

#[test]
fn as_expression_assignable_source_with_excess_is_accepted() {
    // The `expr as T` spelling behaves identically to `<T>expr`.
    assert!(
        diags(
            r#"const g = ({ id: 1, extra: 2 }) as { id: number };"#,
            true
        )
        .is_empty(),
        "`as` assertions must not excess-check either"
    );
}

#[test]
fn interface_target_assertion_with_excess_is_accepted() {
    // A nominal (interface) target is likewise not excess-checked. Mirrors
    // `overloadResolutionOverNonCTObjectLit.ts`.
    assert!(
        diags(
            r#"interface IToken { a: number; b: string } const x = <IToken>{ a: 1, b: "s", extra: 3 };"#,
            true
        )
        .is_empty(),
        "an interface-targeted assertion must not report an excess property"
    );
}

#[test]
fn non_overlapping_object_assertion_is_ts2352_not_ts2353() {
    // A fresh object literal with no property in common with the target is a
    // conversion mistake (TS2352) — NOT an excess-property error (TS2353).
    // Mirrors the `<IFoo>{ c: null }` line of `noImplicitAnyInCastExpression.ts`.
    let d = diags(r#"const h = <{ id: number }>{ foo: "s" };"#, true);
    assert_eq!(
        codes(&d),
        vec![2352],
        "no-overlap object cast must be exactly TS2352, got: {d:?}"
    );
    assert!(
        !d.iter().any(|(c, _)| *c == 2353),
        "must never report TS2353 for an assertion: {d:?}"
    );
}

#[test]
fn renamed_binder_assertion_with_excess_is_accepted() {
    // §Anti-hardcoding: the accepted-ness is structural, not name-dependent.
    assert!(
        diags(
            r#"const slot = <{ tag: number }>{ tag: 1, marker: "x" };"#,
            true
        )
        .is_empty(),
        "renamed shape must also be accepted without an excess error"
    );
}
