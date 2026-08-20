//! tsc-parity for the diagnostic display of an **anonymous-object
//! intersection** (`{ a } & { b }`) in assignability messages.
//!
//! tsz internally merges an object-only intersection into a single object
//! (`{ a; b }`) so member lookup is O(1), but `tsc` never collapses
//! intersection members into one object literal for display — it renders
//! `{ a; } & { b; }` (or the alias name when the intersection is referenced
//! through a non-generic type alias). These guards pin every assignability
//! position (alias target, inline target, call argument, source) and assert
//! the collapsed single-object form never leaks. Binder names vary across
//! cases (anti-hardcoding): the behavior is structural, not name-driven.

use tsz_checker::test_utils::check_source_code_messages as get_diagnostics;

fn message(source: &str, code: u32) -> String {
    let diags = get_diagnostics(source);
    diags
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, m)| m.clone())
        .unwrap_or_else(|| panic!("expected TS{code}; got: {diags:?}"))
}

/// A non-generic alias whose body is an anonymous-object intersection keeps its
/// declared name in the target position — `tsc` stamps the intersection with
/// the alias symbol, so the message reads `… type 'Combined'`, never the
/// expanded `{ first: 1; second: 2; }`.
#[test]
fn alias_of_anonymous_object_intersection_target_shows_alias_name() {
    let msg = message(
        "type Combined = { first: 1 } & { second: 2 };\nconst bad: Combined = { first: 1 };\n",
        2322,
    );
    assert!(
        msg.contains("type 'Combined'"),
        "expected the alias name 'Combined' in the target, got: {msg}"
    );
    assert!(
        !msg.contains("first: 1; second: 2"),
        "the merged single-object form must not leak, got: {msg}"
    );
}

/// An inline anonymous-object intersection in a **call-argument parameter**
/// position renders as the `&`-joined member form, not the merged object.
#[test]
fn inline_intersection_parameter_keeps_ampersand_form() {
    let msg = message(
        "declare function accept(arg: { alpha: 1 } & { beta: 2 }): void;\naccept({ alpha: 1 });\n",
        2345,
    );
    assert!(
        msg.contains("{ alpha: 1; } & { beta: 2; }"),
        "expected the intersection `&` form for the parameter, got: {msg}"
    );
    assert!(
        !msg.contains("alpha: 1; beta: 2"),
        "the merged single-object form must not leak, got: {msg}"
    );
}

/// The same inline intersection in a **variable-annotation target** position.
#[test]
fn inline_intersection_variable_target_keeps_ampersand_form() {
    let msg = message(
        "const wrong: { left: 1 } & { right: 2 } = { left: 1 };\n",
        2322,
    );
    assert!(
        msg.contains("{ left: 1; } & { right: 2; }"),
        "expected the intersection `&` form for the target, got: {msg}"
    );
}

/// A three-member anonymous-object intersection keeps every member joined by
/// `&` rather than collapsing into one object.
#[test]
fn three_member_anonymous_intersection_keeps_all_members() {
    let msg = message(
        "declare function take(p: { one: 1 } & { two: 2 } & { three: 3 }): void;\ntake({ one: 1 });\n",
        2345,
    );
    assert!(
        msg.contains("{ one: 1; } & { two: 2; } & { three: 3; }"),
        "expected all three intersection members joined by `&`, got: {msg}"
    );
}

/// An anonymous-object intersection in the **source** position is also rendered
/// with the `&` form (here through the `TS2741` missing-property message).
#[test]
fn anonymous_intersection_source_keeps_ampersand_form() {
    let msg = message(
        "declare const blended: { p: 1 } & { q: 2 };\nconst target: { r: 3 } = blended;\n",
        2741,
    );
    assert!(
        msg.contains("{ p: 1; } & { q: 2; }"),
        "expected the intersection `&` form for the source, got: {msg}"
    );
    assert!(
        !msg.contains("p: 1; q: 2; }"),
        "the merged single-object form must not leak in the source, got: {msg}"
    );
}

/// Regression guard: an intersection with a **named** member still renders the
/// `{ … } & Named` form (the named member was never collapsed, and removing the
/// anonymous-only collapse must not change it).
#[test]
fn intersection_with_named_member_unchanged() {
    let msg = message(
        "interface Tail { tailProp: string }\ndeclare function mix<T>(o: T): T & Tail;\nconst out: { headProp: string } = mix({ headProp: 1 });\n",
        2322,
    );
    assert!(
        msg.contains("& Tail"),
        "expected the named member 'Tail' to remain in the `&` form, got: {msg}"
    );
}

// =================================================================
// #14827: inline / anonymous object & union annotations that
// coincidentally match a non-generic type-alias body must render
// structurally, while genuine alias references keep the alias name.
// =================================================================

/// An inline object-literal annotation whose shape coincides with a non-generic
/// alias body renders structurally (`{ a: number; }`), not the alias name `A` —
/// the annotation carries no `aliasSymbol`. The alias `A` is declared so the
/// reverse type-to-def lookup *would* repaint it without the fix.
#[test]
fn inline_object_target_coinciding_with_alias_renders_structurally() {
    let msg = message(
        "type A = { a: number };\nconst x: { a: number } = \"wrong\";\n",
        2322,
    );
    assert!(
        msg.contains("type '{ a: number; }'"),
        "inline object target must render structurally, got: {msg}"
    );
    assert!(
        !msg.contains("type 'A'"),
        "the coincidental alias name must not leak, got: {msg}"
    );
}

/// A genuine reference to the alias keeps the alias name (the annotation carried
/// the `aliasSymbol`). This is the anti-over-suppression guard for the object
/// case: the fix must distinguish a reference from an inline annotation.
#[test]
fn object_alias_reference_target_keeps_alias_name() {
    let msg = message("type A = { a: number };\nconst x: A = \"wrong\";\n", 2322);
    assert!(
        msg.contains("type 'A'"),
        "an alias reference must keep its name, got: {msg}"
    );
}

/// An inline union of object literals coinciding with alias bodies renders each
/// member structurally, never repainting members with their alias names.
#[test]
fn inline_object_union_target_renders_structurally() {
    let msg = message(
        "type A = { a: number };\ntype B = { b: string };\nconst u: { a: number } | { b: string } = 5;\n",
        2322,
    );
    assert!(
        msg.contains("{ a: number; } | { b: string; }"),
        "inline union members must render structurally, got: {msg}"
    );
}

/// A genuine union-of-aliases reference keeps both alias names (`A | B`).
#[test]
fn union_alias_reference_target_keeps_alias_names() {
    let msg = message(
        "type A = { a: number };\ntype B = { b: string };\nconst u: A | B = 5;\n",
        2322,
    );
    assert!(
        msg.contains("type 'A | B'"),
        "a union of alias references must keep the names, got: {msg}"
    );
}

/// An inline object annotation on a **source** identifier renders structurally
/// rather than picking up the coincidental alias name.
#[test]
fn inline_object_source_renders_structurally() {
    let msg = message(
        "type A = { a: number };\ndeclare const s: { a: number };\nconst t: string = s;\n",
        2322,
    );
    assert!(
        msg.contains("Type '{ a: number; }'"),
        "inline object source must render structurally, got: {msg}"
    );
    assert!(
        !msg.contains("Type 'A'"),
        "the coincidental alias name must not leak in the source, got: {msg}"
    );
}

// ---------------------------------------------------------------------------
// Whitespace normalization: a written annotation echoed from source text (a
// named-reference intersection/union, which does not reach the structural
// `{ … } & { … }` formatter) must be re-spaced canonically like `tsc`'s
// printer, never leaked verbatim. Binder names and spacing vary per case.
// ---------------------------------------------------------------------------

/// A named-interface intersection source written with padded `&` spacing renders
/// `Left & Right`, not the verbatim `Left   &   Right`.
#[test]
fn named_intersection_source_normalizes_extra_whitespace() {
    let msg = message(
        "interface Left { p: number }\ninterface Right { q: number }\n\
         const src: Left   &   Right = { p: 1, q: 2 };\nconst sink: { r: number } = src;\n",
        2741,
    );
    assert!(
        msg.contains("in type 'Left & Right'"),
        "padded intersection spacing must normalize to 'Left & Right', got: {msg}"
    );
    assert!(
        !msg.contains("Left   &"),
        "verbatim source whitespace must not leak, got: {msg}"
    );
}

/// A named-interface intersection in a call-argument source position normalizes
/// its spacing too (TS2345).
#[test]
fn named_intersection_call_argument_source_normalizes_whitespace() {
    let msg = message(
        "interface One { p: number }\ninterface Two { q: number }\n\
         declare function consume(x: number): void;\n\
         const combined: One   &   Two = { p: 1, q: 2 };\nconsume(combined);\n",
        2345,
    );
    assert!(
        msg.contains("Argument of type 'One & Two'"),
        "call-argument intersection source spacing must normalize, got: {msg}"
    );
}

/// A named-interface union in a target-annotation position normalizes padded
/// `|` spacing to `First | Second` (TS2322).
#[test]
fn named_union_target_normalizes_extra_whitespace() {
    let msg = message(
        "interface First { a: number }\ninterface Second { b: number }\n\
         const wrong: First   |   Second = 5;\n",
        2322,
    );
    assert!(
        msg.contains("to type 'First | Second'"),
        "padded union spacing must normalize to 'First | Second', got: {msg}"
    );
    assert!(
        !msg.contains("First   |"),
        "verbatim source whitespace must not leak, got: {msg}"
    );
}
