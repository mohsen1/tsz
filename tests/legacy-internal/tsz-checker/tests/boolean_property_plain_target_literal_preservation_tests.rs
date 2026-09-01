//! Boolean-literal property preservation against a bare (non-union)
//! `boolean` target property in the fresh object-literal source render.
//!
//! Structural rule: `tsc` represents `boolean` internally as `true | false`,
//! so `isLiteralOfContextualType` always finds a same-base literal
//! constituent when the contextual target property type is the bare
//! `boolean` intrinsic — a `true`/`false` source literal keeps its literal
//! display even though the target property is not itself a literal or an
//! explicit union. `string`/`number`/`bigint` are genuine opaque primitives
//! with no literal constituents, so a same-shape source against a plain
//! `string`/`number`/`bigint` target property still widens; only `boolean`
//! is special. tsz now mirrors this in
//! `CheckerState::type_contains_literal_of_primitive_base`
//! (`error_reporter/core/type_display.rs`), which previously only recognized
//! this case through the separate "normalized union" heuristic and missed a
//! bare `boolean` target property entirely.
//!
//! All expectations oracle-pinned against pinned typescript@7.0.2
//! (`scripts/conformance/oracle.sh --strict`, 2026-08-20). Binder and
//! property names vary across cases so the behavior is proven structural,
//! not keyed to any one identifier.

use crate::diagnostics::Diagnostic;
use crate::test_utils::check_source_strict;

fn diags_with_code(source: &str, code: u32) -> Vec<Diagnostic> {
    check_source_strict(source)
        .into_iter()
        .filter(|d| d.code == code)
        .collect()
}

fn single_diag(source: &str, code: u32) -> Diagnostic {
    let mut diags = diags_with_code(source, code);
    assert_eq!(
        diags.len(),
        1,
        "expected exactly one TS{code} for `{source}`, got: {diags:?}"
    );
    diags.remove(0)
}

#[test]
fn true_literal_preserved_against_bare_boolean_target_property() {
    // tsc: Property 'z' is missing in type '{ b: true; }' but required in
    //      type '{ z: string; b: boolean; }'.
    let diag = single_diag(
        r#"
const d: { z: string; b: boolean } = { b: true };
"#,
        2741,
    );
    assert!(
        diag.message_text
            .contains("'{ b: true; }' but required in type '{ z: string; b: boolean; }'"),
        "boolean-literal source must keep its literal display against a bare \
         `boolean` target property, got: {}",
        diag.message_text
    );
}

#[test]
fn false_literal_preserved_with_renamed_binders() {
    // tsc: Property 'name' is missing in type '{ flag: false; }' but
    //      required in type '{ name: string; flag: boolean; }'.
    let diag = single_diag(
        r#"
const alpha: { name: string; flag: boolean } = { flag: false };
"#,
        2741,
    );
    assert!(
        diag.message_text
            .contains("'{ flag: false; }' but required in type '{ name: string; flag: boolean; }'"),
        "renamed binders must not change the structural rule, got: {}",
        diag.message_text
    );
}

/// Pinned residual, deliberately out of scope for this fix: a type-alias
/// wrapper around the target property type (`type Toggle = boolean; ...
/// flag: Toggle`) defeats literal preservation, where an unwrapped `boolean`
/// property (tested above) does not. Confirmed NOT specific to `boolean` —
/// `string_literal_union_defeated_by_alias_wrapper_is_a_general_pre_existing_gap`
/// shows the identical failure for a plain literal-union alias with no
/// `boolean` involved at all, so the owner is generic: the target shape's
/// per-property `TypeId` (from `object_shape_for_type`) stays an unresolved
/// `TypeData::Lazy(DefId)` alias reference at this call site, and
/// `type_contains_literal_of_primitive_base`/`widen_literal_to_primitive`
/// only match concrete `TypeData::Literal`/intrinsic shapes — resolving it
/// needs the checker's `&mut self` lazy-stabilization path
/// (`resolve_lazy_type`), not reachable from this `&self` display helper
/// without a wider signature change across its call sites in
/// `object_literal_source_display.rs`. Owner: alias resolution for shape
/// properties in the literal-preservation acceptance test, not the
/// boolean-is-`true`-or-`false` structural rule this PR fixes.
#[test]
#[ignore = "pre-existing: Lazy(DefId) alias target property types are not \
            resolved before the literal-preservation acceptance check; not \
            specific to boolean, see the sibling ignored test below"]
fn true_literal_preserved_through_a_type_alias_wrapper() {
    // tsc: Property 'name' is missing in type '{ flag: true; }' but
    //      required in type '{ name: string; flag: boolean; }'.
    let diag = single_diag(
        r#"
type Toggle = boolean;
const beta: { name: string; flag: Toggle } = { flag: true };
"#,
        2741,
    );
    assert!(
        diag.message_text
            .contains("'{ flag: true; }' but required in type '{ name: string; flag: boolean; }'"),
        "a type-alias wrapper around `boolean` must not defeat literal \
         preservation, got: {}",
        diag.message_text
    );
}

#[test]
#[ignore = "pre-existing: same Lazy(DefId) alias-resolution gap as above, \
            proven independent of boolean"]
fn string_literal_union_defeated_by_alias_wrapper_is_a_general_pre_existing_gap() {
    let diag = single_diag(
        r#"
type Choice = "a" | "b";
const beta: { name: string; pick: Choice } = { pick: "a" };
"#,
        2741,
    );
    assert!(
        diag.message_text.contains(
            "'{ pick: \"a\"; }' but required in type '{ name: string; pick: \"a\" | \"b\"; }'"
        ),
        "got: {}",
        diag.message_text
    );
}

#[test]
fn true_literal_preserved_against_bare_boolean_member_in_union_target() {
    // tsc: Type '{ kind: "a"; flag: true; }' is not assignable to type 'U'.
    //        Property 'extra' is missing in type '{ kind: "a"; flag: true; }'
    //        but required in type
    //        '{ kind: "a"; flag: boolean; extra: string; }'.
    let diag = single_diag(
        r#"
type U =
  | { kind: "a"; flag: boolean; extra: string }
  | { kind: "b"; flag: boolean; extra: string };
const gamma: U = { kind: "a", flag: true };
"#,
        2322,
    );
    assert!(
        diag.message_text
            .starts_with(r#"Type '{ kind: "a"; flag: true; }' is not assignable to type 'U'."#),
        "the union-member path must also keep the literal for a bare \
         `boolean` member property, got: {}",
        diag.message_text
    );
}

#[test]
fn string_literal_still_widens_against_bare_string_target_property() {
    // tsc: Property 'z' is missing in type '{ s: string; }' but required in
    //      type '{ z: string; s: string; }'. `string` (unlike `boolean`) is
    //      a genuine opaque primitive, not a union of all string literals.
    let diag = single_diag(
        r#"
const e: { z: string; s: string } = { s: "hello" };
"#,
        2741,
    );
    assert!(
        diag.message_text
            .contains("'{ s: string; }' but required in type '{ z: string; s: string; }'"),
        "a string literal source must still widen against a bare `string` \
         target property, got: {}",
        diag.message_text
    );
}

#[test]
fn number_literal_still_widens_against_bare_number_target_property() {
    // tsc: Property 'z' is missing in type '{ n: number; }' but required in
    //      type '{ z: string; n: number; }'.
    let diag = single_diag(
        r#"
const f: { z: string; n: number } = { n: 5 };
"#,
        2741,
    );
    assert!(
        diag.message_text
            .contains("'{ n: number; }' but required in type '{ z: string; n: number; }'"),
        "a numeric literal source must still widen against a bare `number` \
         target property, got: {}",
        diag.message_text
    );
}

#[test]
fn bigint_literal_still_widens_against_bare_bigint_target_property() {
    // tsc: Property 'z' is missing in type '{ g: bigint; }' but required in
    //      type '{ z: string; g: bigint; }'.
    let diag = single_diag(
        r#"
const g: { z: string; g: bigint } = { g: 5n };
"#,
        2741,
    );
    assert!(
        diag.message_text
            .contains("'{ g: bigint; }' but required in type '{ z: string; g: bigint; }'"),
        "a bigint literal source must still widen against a bare `bigint` \
         target property, got: {}",
        diag.message_text
    );
}
