//! A utility-type application (`Pick`/`Record`/`Omit`/...) that evaluates to an
//! object shape records a `display_alias` so diagnostics can print the nice
//! `Pick<...>` / `Record<...>` form. Object types are content-interned, so that
//! single alias is keyed on the shared structural `TypeId` and — before this fix
//! — leaked onto every later plain `{ ... }` annotation of the same shape,
//! repainting an unrelated declaration's type with the utility name.
//!
//! `tsc` always renders a hand-written object-type literal annotation
//! structurally and never stamps an alias symbol on it, so the printer must
//! refuse to repaint a literal annotation with a same-shape utility alias. The
//! fix marks object ids produced from a `{ ... }` annotation and renders them
//! structurally even when a utility result recorded an alias on the shared id.
//! Display-only: the value semantics are unchanged. (#14833)

use super::super::core::*;

/// The reported witness: a plain `{ a: number; b: string }` annotation on a line
/// with no utility type must render structurally, not as the `Pick<...>` name
/// borrowed from a structurally identical `Pick` result elsewhere in the file.
#[test]
fn pick_result_does_not_pollute_plain_annotation() {
    let diagnostics = compile_and_get_diagnostics_with_lib(
        r#"
type P = Pick<{ a: number; b: string; c: boolean }, "a" | "b">;
declare const p: P; const z: 0 = p;
declare const plain: { a: number; b: string };
const w: 0 = plain;
"#,
    );
    let messages: Vec<&str> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2322)
        .map(|(_, m)| m.as_str())
        .collect();
    // The plain `{ a: number; b: string }` annotation must produce a TS2322
    // whose source renders exactly as the structural object — never the
    // `Pick<...>` alias borrowed from the structurally identical `Pick` result.
    assert!(
        messages
            .iter()
            .any(|m| m.contains("Type '{ a: number; b: string; }' is not assignable to type '0'")),
        "the plain object annotation must render structurally, got: {messages:?}"
    );
    assert!(
        messages
            .iter()
            .all(|m| m.contains("Pick<") == m.contains("c: boolean")),
        "no `Pick<...>` alias may escape onto a message that is not the genuine \
         `Pick<{{ a; b; c; }}, ...>` source, got: {messages:?}"
    );
}

/// `Record` minimal variant from the issue: no `Pick`/`Record` token on the
/// witness line.
#[test]
fn record_result_does_not_pollute_plain_annotation() {
    let diagnostics = compile_and_get_diagnostics_with_lib(
        r#"
type PR = Record<"a" | "b", number>;
declare const pr: PR; const z: 0 = pr;
declare const pk: { a: number; b: number };
const w: 0 = pk;
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
            .any(|m| m.contains("Type '{ a: number; b: number; }' is not assignable to type '0'")),
        "the plain object annotation must render structurally, got: {messages:?}"
    );
}

/// Order-independent: the plain annotation comes BEFORE the utility declaration.
/// The pollution fired in either order, so the fix must hold in either order.
#[test]
fn pollution_is_order_independent() {
    let diagnostics = compile_and_get_diagnostics_with_lib(
        r#"
declare const plain: { a: number; b: string };
const w: 0 = plain;
type P = Pick<{ a: number; b: string; c: boolean }, "a" | "b">;
declare const p: P; const z: 0 = p;
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
            .any(|m| m.contains("Type '{ a: number; b: string; }' is not assignable to type '0'")),
        "the plain annotation must still render structurally when written first, got: {messages:?}"
    );
    assert!(
        messages
            .iter()
            .all(|m| !(m.contains("Pick<") && m.contains("b: string"))),
        "no `Pick<...>` alias may escape onto the structural annotation, got: {messages:?}"
    );
}

/// Anti-hardcoding: the fix is structural (literal-annotation provenance), not
/// keyed on the lib `Pick`/`Record` names. A user-defined generic alias with a
/// renamed binder that evaluates to the same object shape must also not pollute
/// a plain annotation.
#[test]
fn user_named_helper_does_not_pollute_plain_annotation() {
    let diagnostics = compile_and_get_diagnostics_with_lib(
        r#"
type GrabFirst<T, K extends keyof T> = { [P in K]: T[P] };
type Q = GrabFirst<{ a: number; b: string; c: boolean }, "a" | "b">;
declare const q: Q; const z: 0 = q;
declare const plain: { a: number; b: string };
const w: 0 = plain;
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
            .any(|m| m.contains("Type '{ a: number; b: string; }' is not assignable to type '0'")),
        "the plain object annotation must render structurally, got: {messages:?}"
    );
    // The only message allowed to carry the `GrabFirst<...>` alias is the
    // genuine utility use (whose expanded source arg includes `c: boolean`).
    assert!(
        messages
            .iter()
            .all(|m| m.contains("GrabFirst<") == m.contains("c: boolean")),
        "the user helper alias must not escape onto the structural annotation, got: {messages:?}"
    );
}

/// Control: an inline `Pick<...>` annotation with no structurally-identical
/// plain `{ ... }` annotation keeps its utility name (the alias is only
/// suppressed for hand-written literal annotations, never for the utility use).
#[test]
fn inline_utility_annotation_keeps_its_name() {
    let diagnostics = compile_and_get_diagnostics_with_lib(
        r#"
const x: Pick<{ a: number; b: string }, "a"> = { a: 5 as number, extra: 1 };
"#,
    );
    let message = diagnostic_message(&diagnostics, 2353)
        .expect("TS2353 expected — excess property on a `Pick<...>` target");
    assert!(
        message.contains("Pick<"),
        "the inline utility annotation must keep its `Pick<...>` name, got: {message}"
    );
}

/// The fix is display-only: a `Pick` value really has the picked members, so it
/// stays assignable to a structurally-identical plain object type — no TS2322.
#[test]
fn pick_value_remains_assignable_to_structural_target() {
    let diagnostics = compile_and_get_diagnostics_with_lib(
        r#"
type P = Pick<{ a: number; b: string; c: boolean }, "a" | "b">;
declare const p: P;
const plain: { a: number; b: string } = p;
"#,
    );
    assert!(
        !has_error(&diagnostics, 2322),
        "a `Pick` result is structurally assignable to the same shape; no TS2322 expected. \
         Actual: {diagnostics:#?}"
    );
}
