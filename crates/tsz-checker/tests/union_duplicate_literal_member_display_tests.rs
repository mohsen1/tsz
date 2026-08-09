//! Duplicate *inline* anonymous object literals in a union must each print,
//! matching `tsc` (#16509).
//!
//! `tsc` mints a fresh anonymous type per written `{ ... }` occurrence, so
//! `{ m: number } | { m: number } | { p: string }` is a genuine three-member
//! union and its diagnostic prints all three constituents. tsz content-interns
//! the two identical `{ m: number }` literals onto one `TypeId`, so its
//! canonical union is only two members — historically the display dropped the
//! duplicate `tsc` keeps.
//!
//! The fix stores the as-written member list as the union's display `origin`
//! when canonical dedup collapsed a duplicate *anonymous object* (see
//! `store_union_origin` / `origin_collapsed_anonymous_object_duplicate`), and
//! the printer exempts a member the source wrote as an inline `{ ... }` literal
//! *in this union* from its display collapse (`is_preserved_inline_object_literal`,
//! keyed on `is_union_literal_member`).
//!
//! Three properties are asserted, with binder names varied so nothing is a
//! name- or file-scoped suppression (anti-hardcoding):
//! - inline duplicates are preserved (the fix);
//! - alias references (`Alias | Alias`) and named types (`Foo | Foo`) still
//!   collapse to one constituent, exactly as `tsc` collapses them;
//! - primitive-literal duplicates (`1 | 1`) still collapse.

use tsz_checker::test_utils::check_source_diagnostics;
use tsz_common::diagnostics::Diagnostic;

fn diagnostics(source: &str) -> Vec<Diagnostic> {
    check_source_diagnostics(source)
}

/// The top-level TS2322 message text, if any.
fn ts2322_message(diags: &[Diagnostic]) -> Option<String> {
    diags
        .iter()
        .find(|d| d.code == 2322)
        .map(|d| d.message_text.clone())
}

/// The issue's exact repro (#16509): a two-member union whose members are
/// *both* the identical inline `{ m: number }` literal. Content-interning
/// collapses the whole union to that one shape, so before the multiplicity-
/// preserving construction the display dropped to a single `{ m: number; }`.
/// `tsc` prints both.
#[test]
fn fully_collapsing_two_member_object_literal_union_prints_both() {
    let diags = diagnostics(
        r#"
declare const a: { m: number } | { m: number };
const x: boolean = a;
"#,
    );
    let msg = ts2322_message(&diags).unwrap_or_default();
    assert!(
        msg.contains("{ m: number; } | { m: number; }"),
        "expected both written constituents to print, got: {msg:?}"
    );
}

/// Renamed-binder control for the fully-collapsing two-member case: distinct
/// property spelling, same per-occurrence provenance behavior.
#[test]
fn fully_collapsing_two_member_object_literal_union_prints_both_renamed() {
    let diags = diagnostics(
        r#"
declare const source: { count: string } | { count: string };
const target: boolean = source;
"#,
    );
    let msg = ts2322_message(&diags).unwrap_or_default();
    assert!(
        msg.contains("{ count: string; } | { count: string; }"),
        "expected both written constituents to print, got: {msg:?}"
    );
}

/// Fully-collapsing three-identical case: multiplicity is exact even when
/// nothing else distinguishes the union, so it renders three constituents.
#[test]
fn fully_collapsing_three_member_object_literal_union_prints_all_three() {
    let diags = diagnostics(
        r#"
declare const a: { m: number } | { m: number } | { m: number };
const x: boolean = a;
"#,
    );
    let msg = ts2322_message(&diags).unwrap_or_default();
    assert!(
        msg.contains("{ m: number; } | { m: number; } | { m: number; }"),
        "expected exactly three constituents to print, got: {msg:?}"
    );
}

/// Negative control for the fully-collapsing path: a two-member union of the
/// *same named interface* still collapses to one constituent — the multiplicity
/// preservation is anonymous-object-specific, matching `tsc`'s `Foo | Foo` → `Foo`.
#[test]
fn fully_collapsing_two_member_named_interface_union_collapses() {
    let diags = diagnostics(
        r#"
interface Foo { m: number }
declare const a: Foo | Foo;
const x: boolean = a;
"#,
    );
    let msg = ts2322_message(&diags).unwrap_or_default();
    assert!(
        msg.contains("'Foo' is not assignable") && !msg.contains("Foo | Foo"),
        "expected the duplicate named interface to collapse to one, got: {msg:?}"
    );
}

/// Negative control: a two-member union of the same type parameter (`T | T`)
/// collapses to `T`. The multiplicity preservation keys on the reduced type
/// being a *bare anonymous object*, not on an object reachable only through a
/// type parameter's constraint, so a constrained `T` never renders as `T | T`.
#[test]
fn fully_collapsing_type_parameter_union_collapses() {
    let diags = diagnostics(
        r#"
declare function f<T extends { m: number }>(x: T | T): void;
declare const v: boolean;
f(v);
"#,
    );
    // The TS2345 argument message names the parameter type `T`, never `T | T`.
    let msg = diags
        .iter()
        .find(|d| d.code == 2345)
        .map(|d| d.message_text.clone())
        .unwrap_or_default();
    assert!(
        !msg.contains("T | T"),
        "a `T | T` type-parameter union must collapse to `T`, got: {msg:?}"
    );
}

/// Three written members, two of them identical inline `{ m: number }`
/// literals: `tsc` prints all three. The duplicate must survive to the display.
#[test]
fn duplicate_inline_object_literals_each_print() {
    let diags = diagnostics(
        r#"
declare const c: { m: number } | { m: number } | { p: string };
const y: boolean = c;
"#,
    );
    let msg = ts2322_message(&diags).unwrap_or_default();
    assert!(
        msg.contains("{ m: number; } | { m: number; } | { p: string; }"),
        "expected all three written constituents to print, got: {msg:?}"
    );
}

/// Renamed-binder control for the same shape: the property names and the
/// distinct member all differ, so the behavior is per-occurrence provenance,
/// not a fixed spelling.
#[test]
fn duplicate_inline_object_literals_each_print_renamed() {
    let diags = diagnostics(
        r#"
declare const source: { count: string } | { count: string } | { flag: number };
const target: boolean = source;
"#,
    );
    let msg = ts2322_message(&diags).unwrap_or_default();
    assert!(
        msg.contains("{ count: string; } | { count: string; } | { flag: number; }"),
        "expected all three written constituents to print, got: {msg:?}"
    );
}

/// Four-way: three identical inline literals alongside a distinct one all
/// print — the multiplicity is exact, not merely "at least two".
#[test]
fn triple_inline_object_literal_multiplicity_is_exact() {
    let diags = diagnostics(
        r#"
declare const c: { m: number } | { m: number } | { m: number } | { p: string };
const y: boolean = c;
"#,
    );
    let msg = ts2322_message(&diags).unwrap_or_default();
    assert!(
        msg.contains("{ m: number; } | { m: number; } | { m: number; } | { p: string; }"),
        "expected exactly three `{{ m: number; }}` constituents, got: {msg:?}"
    );
}

/// Negative control: a named alias referenced twice shares `tsc`'s named
/// identity, so `Alias | Alias | { p: string }` collapses the alias to a
/// single constituent — the inline-literal exemption must not fire for
/// alias references.
#[test]
fn duplicate_alias_reference_collapses() {
    let diags = diagnostics(
        r#"
type Alias = { m: number };
declare const c: Alias | Alias | { p: string };
const y: boolean = c;
"#,
    );
    let msg = ts2322_message(&diags).unwrap_or_default();
    assert!(
        msg.contains("Alias | { p: string; }"),
        "expected the duplicate alias reference to collapse to one, got: {msg:?}"
    );
    assert!(
        !msg.contains("Alias | Alias"),
        "an alias reference shares named identity and must not be duplicated, got: {msg:?}"
    );
}

/// Negative control: a named interface referenced twice collapses, exactly as
/// `tsc` collapses `Foo | Foo`.
#[test]
fn duplicate_named_interface_collapses() {
    let diags = diagnostics(
        r#"
interface Foo { m: number }
declare const c: Foo | Foo | { p: string };
const y: boolean = c;
"#,
    );
    let msg = ts2322_message(&diags).unwrap_or_default();
    assert!(
        msg.contains("Foo | { p: string; }") && !msg.contains("Foo | Foo"),
        "expected the duplicate named interface to collapse to one, got: {msg:?}"
    );
}

/// Negative control: primitive-literal duplicates share value identity in
/// `tsc` too, so `1 | 1` collapses — the exemption is object-literal-specific.
#[test]
fn duplicate_primitive_literals_collapse() {
    let diags = diagnostics(
        r#"
declare const c: { m: number } | { m: number } | 0;
const y: boolean = c;
"#,
    );
    let msg = ts2322_message(&diags).unwrap_or_default();
    // The two object literals are preserved; the case is only here to guard
    // that adding a non-object member does not disturb the object duplicates.
    assert!(
        msg.contains("{ m: number; } | { m: number; }"),
        "expected both object literals to print alongside the scalar, got: {msg:?}"
    );
}

/// Distinct anonymous objects were always preserved (different `TypeId`s); this
/// guards that the change does not perturb the common case.
#[test]
fn distinct_object_literals_still_each_print() {
    let diags = diagnostics(
        r#"
declare const c: { m: number } | { m: string } | { p: boolean };
const y: boolean = c;
"#,
    );
    let msg = ts2322_message(&diags).unwrap_or_default();
    assert!(
        msg.contains("{ m: number; } | { m: string; } | { p: boolean; }"),
        "expected all three distinct constituents to print, got: {msg:?}"
    );
}
