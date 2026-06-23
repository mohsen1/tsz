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
