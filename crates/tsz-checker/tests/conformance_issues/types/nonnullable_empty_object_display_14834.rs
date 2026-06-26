//! `NonNullable<T>` (and any user `type Helper<T> = T & {}`) applied to a
//! non-nullable primitive must display in diagnostics as the bare primitive
//! (`number` / `string` / `boolean`), matching tsc — not the boxed,
//! capitalized intersection spelling `Number & {}` / `String & {}`.
//!
//! The `& {}` co-member is the identity element on a non-nullable primitive,
//! so tsc collapses the application alias to the bare primitive. The genuine
//! branded case (`number & { __brand: B }`, #5195) keeps the expanded,
//! capitalized form and must remain unchanged. Source-written `number & {}`
//! annotations also keep the lowercase intersection form in both compilers.
//! (#14834)

use super::super::core::*;

/// `NonNullable<number>` assigned to a literal `0` target prints `number`,
/// not `Number & {}`.
#[test]
fn nonnullable_number_displays_bare_primitive() {
    let diagnostics = compile_and_get_diagnostics_with_lib(
        r#"
declare const a: NonNullable<number>;
const x: 0 = a;
"#,
    );
    let message = diagnostic_message(&diagnostics, 2322)
        .expect("TS2322 expected for `NonNullable<number>` assigned to `0`");
    assert!(
        message.contains("Type 'number' is not assignable to type '0'"),
        "`NonNullable<number>` source type must render as bare `number`, got: {message}"
    );
    assert!(
        !message.contains("Number") && !message.contains('&'),
        "must not render the boxed/expanded `Number & {{}}` spelling, got: {message}"
    );
}

/// `NonNullable<string>` prints `string`.
#[test]
fn nonnullable_string_displays_bare_primitive() {
    let diagnostics = compile_and_get_diagnostics_with_lib(
        r#"
declare const a: NonNullable<string>;
const x: 0 = a;
"#,
    );
    let message = diagnostic_message(&diagnostics, 2322)
        .expect("TS2322 expected for `NonNullable<string>` assigned to `0`");
    assert!(
        message.contains("Type 'string' is not assignable to type '0'"),
        "`NonNullable<string>` source type must render as bare `string`, got: {message}"
    );
}

/// `NonNullable<boolean>` prints `boolean`.
#[test]
fn nonnullable_boolean_displays_bare_primitive() {
    let diagnostics = compile_and_get_diagnostics_with_lib(
        r#"
declare const a: NonNullable<boolean>;
const x: 0 = a;
"#,
    );
    let message = diagnostic_message(&diagnostics, 2322)
        .expect("TS2322 expected for `NonNullable<boolean>` assigned to `0`");
    assert!(
        message.contains("Type 'boolean' is not assignable to type '0'"),
        "`NonNullable<boolean>` source type must render as bare `boolean`, got: {message}"
    );
}

/// Null/undefined stripped: `NonNullable<string | null>` still collapses the
/// surviving `string & {}` to bare `string`.
#[test]
fn nonnullable_strips_nullish_then_displays_bare_primitive() {
    let diagnostics = compile_and_get_diagnostics_with_lib(
        r#"
declare const a: NonNullable<string | null>;
const x: 0 = a;
declare const b: NonNullable<number | undefined>;
const y: 0 = b;
"#,
    );
    let messages: Vec<&str> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2322)
        .map(|(_, m)| m.as_str())
        .collect();
    assert!(
        messages
            .iter()
            .any(|m| m.contains("Type 'string' is not assignable to type '0'")),
        "`NonNullable<string | null>` must render as bare `string`, got: {messages:?}"
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("Type 'number' is not assignable to type '0'")),
        "`NonNullable<number | undefined>` must render as bare `number`, got: {messages:?}"
    );
}

/// Anti-hardcoding: a user-defined `T & {}` helper with a non-lib name must
/// behave identically — the fix is structural (empty-object identity), not
/// keyed on the `NonNullable` name.
#[test]
fn user_t_and_empty_object_helper_displays_bare_primitive() {
    let diagnostics = compile_and_get_diagnostics_with_lib(
        r#"
type MyNN<T> = T & {};
declare const a: MyNN<string>;
const x: 0 = a;
"#,
    );
    let message = diagnostic_message(&diagnostics, 2322)
        .expect("TS2322 expected for `MyNN<string>` assigned to `0`");
    assert!(
        message.contains("Type 'string' is not assignable to type '0'"),
        "user `T & {{}}` helper must render as bare `string`, got: {message}"
    );
}

/// The collapse is display-only: the value really is the primitive, so
/// assigning `NonNullable<string>` to `string` produces no error.
#[test]
fn nonnullable_primitive_remains_assignable_to_bare_primitive() {
    let diagnostics = compile_and_get_diagnostics_with_lib(
        r#"
type MyNN<T> = T & {};
declare const a: MyNN<string>;
const s: string = a;
declare const b: NonNullable<number>;
const n: number = b;
"#,
    );
    assert!(
        !has_error(&diagnostics, 2322),
        "`T & {{}}` of a primitive is assignable to the bare primitive; no TS2322 expected. \
         Actual: {diagnostics:#?}"
    );
}

/// Control 1: a source-written `number & {}` annotation keeps the lowercase
/// intersection form (no application alias drives the boxed spelling).
#[test]
fn source_written_intersection_keeps_lowercase_form() {
    let diagnostics = compile_and_get_diagnostics_with_lib(
        r#"
declare const b: number & {};
const y: 0 = b;
"#,
    );
    let message = diagnostic_message(&diagnostics, 2322)
        .expect("TS2322 expected for `number & {}` assigned to `0`");
    assert!(
        message.contains("Type 'number & {}' is not assignable to type '0'"),
        "source-written `number & {{}}` must keep the lowercase intersection form, got: {message}"
    );
}

/// Control 2: the genuine branded primitive (#5195) keeps the expanded,
/// capitalized intersection form, including the real property bag.
#[test]
fn genuine_branded_primitive_keeps_expanded_capitalized_form() {
    let diagnostics = compile_and_get_diagnostics_with_lib(
        r#"
type Brand<B extends string> = number & { __brand: B };
declare const u: Brand<"usd">;
const w: { x: number } = u;
"#,
    );
    let message = diagnostic_message(&diagnostics, 2741)
        .expect("TS2741 expected — branded `number` is missing property `x`");
    assert!(
        message.contains("Number & { __brand: \"usd\"; }"),
        "genuine branded primitive must keep the expanded `Number & {{ __brand: ... }}` form, \
         got: {message}"
    );
}
