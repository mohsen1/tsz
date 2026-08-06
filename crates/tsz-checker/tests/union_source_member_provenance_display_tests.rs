//! Union-source elaboration must not repaint an anonymous member with an
//! unrelated same-shaped alias's name (#16513, row 1).
//!
//! tsc's nested `Type '<member>' is not assignable to type '<target>'.` line
//! under a failing union assignment names the member exactly as written: an
//! anonymous literal (`{ m: number }`) stays anonymous, a named alias
//! reference (`A`) keeps its name. tsz's structural interner content-interns
//! an anonymous literal and a coincidentally-shaped alias body onto the same
//! `TypeId`, so the elaboration's reverse-shape lookup (`resolve_object_shape_name`)
//! could not previously tell "this occurrence was written as `{ ... }`" apart
//! from "this occurrence merely has the same shape as some unrelated alias
//! declared elsewhere in the file" — it repainted the former with the
//! latter's name.
//!
//! The fix records, per `(union_type_id, member_type_id)`, whether a member
//! was written as an anonymous literal directly in that union
//! (`TypeInterner::mark_union_literal_member`, populated in
//! `get_type_from_union_type`), and the elaboration renderer consults that
//! narrower record instead of the global identity-keyed
//! `is_literal_object_annotation` flag. These tests vary binder names and mix
//! literal/named members in one union (anti-hardcoding): the behavior is
//! per-occurrence provenance, not a name or file-scoped suppression.

use tsz_checker::test_utils::check_source_diagnostics;
use tsz_common::diagnostics::Diagnostic;

fn diagnostics(source: &str) -> Vec<Diagnostic> {
    check_source_diagnostics(source)
}

/// True when some TS2322 carries a nested elaboration line matching `predicate`.
fn ts2322_has_nested<P: Fn(&str) -> bool>(diags: &[Diagnostic], predicate: P) -> bool {
    diags.iter().any(|d| {
        d.code == 2322
            && d.related_information
                .iter()
                .any(|info| predicate(&info.message_text))
    })
}

/// Row 1 from #16513: an anonymous two-member literal union assigned to an
/// incompatible target must elaborate the failing member structurally
/// (`{ m: number; }`), never as the unrelated later-declared alias `U` whose
/// body happens to reduce to the same shape.
#[test]
fn anonymous_union_member_elaborates_structurally_despite_unrelated_same_shaped_alias() {
    let diags = diagnostics(
        r#"
declare const c1: { m: number } | { m: string };
const y1: boolean = c1;
type U = { m: number } | { m: number };
declare const c2: U;
const y2: boolean = c2;
"#,
    );
    assert!(
        ts2322_has_nested(&diags, |msg| msg.contains("{ m: number; }")
            && msg.contains("is not assignable to type 'boolean'")),
        "expected the anonymous member to elaborate structurally as '{{ m: number; }}', got: {diags:?}"
    );
    assert!(
        !ts2322_has_nested(&diags, |msg| msg.contains("'U'")),
        "the anonymous member must never be repainted with the unrelated alias name 'U', got: {diags:?}"
    );
}

/// Renamed-binder control for the same shape: the unrelated alias's name and
/// property spelling both change, and the anonymous member must still render
/// structurally rather than picking up whichever alias happens to share its
/// shape.
#[test]
fn anonymous_union_member_elaborates_structurally_with_renamed_unrelated_alias() {
    let diags = diagnostics(
        r#"
declare const source: { count: number } | { count: string };
const target: boolean = source;
type Renamed = { count: number } | { count: number };
declare const other: Renamed;
const alsoTarget: boolean = other;
"#,
    );
    assert!(
        ts2322_has_nested(&diags, |msg| msg.contains("{ count: number; }")),
        "expected the anonymous member to elaborate structurally, got: {diags:?}"
    );
    assert!(
        !ts2322_has_nested(&diags, |msg| msg.contains("'Renamed'")),
        "the anonymous member must never be repainted with the unrelated alias name, got: {diags:?}"
    );
}

/// A genuine named-alias reference inside a union keeps its alias name in the
/// elaboration line — this is the negative control the naive
/// `is_literal_object_annotation`-only fix broke (falsified in #16513's
/// 03:59Z comment): `A`'s body also marks the global literal-annotation
/// table, so a fix that consults only that table cannot distinguish this row
/// from the anonymous-member row above.
#[test]
fn named_alias_union_member_keeps_alias_name_in_elaboration() {
    let diags = diagnostics(
        r#"
type A = { a: number };
type B = { b: string };
declare const c: A | B;
const y: boolean = c;
"#,
    );
    assert!(
        ts2322_has_nested(&diags, |msg| msg.contains("'A'")
            && msg.contains("is not assignable to type 'boolean'")),
        "expected the first failing union member to keep its alias name 'A', got: {diags:?}"
    );
    assert!(
        !ts2322_has_nested(&diags, |msg| msg.contains("{ a: number; }")),
        "a named alias reference must not be expanded structurally, got: {diags:?}"
    );
}

/// Mixed union: a named-alias member and an anonymous-literal member in the
/// same union. The alias member keeps its name; the fix must not blanket-
/// suppress alias names for every member once any member in the union is
/// anonymous.
#[test]
fn mixed_alias_and_literal_union_members_render_independently() {
    let diags = diagnostics(
        r#"
type Named = { tag: number };
declare const c: Named | { other: string };
const y: boolean = c;
"#,
    );
    assert!(
        ts2322_has_nested(&diags, |msg| msg.contains("'Named'")),
        "the alias member (first, and failing) must keep its name, got: {diags:?}"
    );
    assert!(
        !ts2322_has_nested(&diags, |msg| msg.contains("{ tag: number; }")),
        "the alias member must not be expanded structurally, got: {diags:?}"
    );
}

/// A union reached through a further alias indirection: `U2`'s own written
/// member list still carries the real per-member provenance (a named alias
/// reference and an anonymous literal), so aliasing the whole union under a
/// second name must not change which member elaborates with a name and which
/// elaborates structurally.
#[test]
fn union_member_provenance_survives_alias_indirection() {
    let diags = diagnostics(
        r#"
type A = { a: number };
type U2 = A | { m: string };
declare const c: U2;
const y: boolean = c;
"#,
    );
    assert!(
        ts2322_has_nested(&diags, |msg| msg.contains("'A'")),
        "the aliased member must keep its name through the U2 indirection, got: {diags:?}"
    );
    assert!(
        !ts2322_has_nested(&diags, |msg| msg.contains("{ a: number; }")),
        "the aliased member must not be expanded structurally, got: {diags:?}"
    );
}
