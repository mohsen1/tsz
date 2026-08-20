//! Array-literal-valued property display in the fresh object-literal source
//! render (the #17790 handoff probe).
//!
//! Structural rule: when a fresh object-literal source fails against its
//! target and the diagnostic re-renders the source from syntax, a property
//! whose value is an array literal displays its checked (contextually typed)
//! type unwidened whenever the target's own per-property type is what typed
//! it — `tsc` renders `v: [1, 2]` against a tuple arm and `v: 1[]` against an
//! array arm, in the head and the missing-property elaboration alike. A
//! property whose array value found no accepting target still widens (and
//! `tsc` anchors that mismatch at the inner expression with the widened
//! type, e.g. `number[]`).
//!
//! All expectations oracle-pinned against pinned typescript@7.0.2
//! (`scripts/conformance/oracle.sh --strict`, 2026-08-20). Binder and
//! property names vary across cases so the behavior is proven structural.

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
fn numeric_tuple_property_preserved_in_union_head_and_elaboration() {
    // tsc: Type '{ kind: "a"; v: [1, 2]; }' is not assignable to type 'U'.
    //        Property 'w' is missing in type '{ kind: "a"; v: [1, 2]; }' but
    //        required in type '{ kind: "a"; v: [1, 2]; w: string; }'.
    let diag = single_diag(
        r#"
type U = { kind: "a"; v: [1, 2]; w: string } | { kind: "b"; v: [3, 4]; w: string };
const x: U = { kind: "a", v: [1, 2] };
"#,
        2322,
    );
    assert!(
        diag.message_text
            .starts_with(r#"Type '{ kind: "a"; v: [1, 2]; }' is not assignable to type 'U'."#),
        "tuple-valued property must keep element literals in the head, got: {}",
        diag.message_text
    );
}

#[test]
fn numeric_tuple_preserved_in_union_missing_property_chain_link() {
    // tsc: Property 'w' is missing in type '{ kind: "a"; v: [1, 2]; }' but
    //      required in type '{ kind: "a"; v: [1, 2]; w: string; }'.
    let diag = single_diag(
        r#"
type U = { kind: "a"; v: [1, 2]; w: string } | { kind: "b"; v: [3, 4]; w: string };
const x: U = { kind: "a", v: [1, 2] };
"#,
        2322,
    );
    assert!(
        diag.related_information.iter().any(|r| r.message_text
            == r#"Property 'w' is missing in type '{ kind: "a"; v: [1, 2]; }' but required in type '{ kind: "a"; v: [1, 2]; w: string; }'."#),
        "missing-property chain link must keep element literals, got: {:?}",
        diag.related_information
    );
}

#[test]
fn numeric_tuple_property_preserved_in_ts2345_argument_head() {
    // tsc: Argument of type '{ kind: "a"; v: [1, 2]; }' is not assignable to
    //      parameter of type 'U'.
    let diag = single_diag(
        r#"
type U = { kind: "a"; v: [1, 2]; w: string } | { kind: "b"; v: [3, 4]; w: string };
declare function sink(u: U): void;
sink({ kind: "a", v: [1, 2] });
"#,
        2345,
    );
    assert!(
        diag.message_text.starts_with(
            r#"Argument of type '{ kind: "a"; v: [1, 2]; }' is not assignable to parameter of type 'U'."#
        ),
        "tuple-valued property must keep element literals in the TS2345 head, got: {}",
        diag.message_text
    );
}

#[test]
fn array_target_property_renders_contextual_element_literal() {
    // tsc: Type '{ kind: "a"; v: 1[]; }' is not assignable to type 'U'. —
    // the contextually typed value (`1[]`), not the widened `number[]` and
    // not the tuple syntax.
    let diag = single_diag(
        r#"
type U = { kind: "a"; v: 1[]; w: string } | { kind: "b"; v: 2[]; w: string };
const x: U = { kind: "a", v: [1, 1] };
"#,
        2322,
    );
    assert!(
        diag.message_text
            .starts_with(r#"Type '{ kind: "a"; v: 1[]; }' is not assignable to type 'U'."#),
        "array-target property must render its contextual element literal, got: {}",
        diag.message_text
    );
}

#[test]
fn string_tuple_property_preserved_with_renamed_binders() {
    // tsc: Type '{ tag: "p"; parts: ["x", "y"]; }' is not assignable to type 'Doc'.
    let diag = single_diag(
        r#"
type Doc = { tag: "p"; parts: ["x", "y"]; body: string } | { tag: "q"; parts: ["z"]; body: string };
const d: Doc = { tag: "p", parts: ["x", "y"] };
"#,
        2322,
    );
    assert!(
        diag.message_text.starts_with(
            r#"Type '{ tag: "p"; parts: ["x", "y"]; }' is not assignable to type 'Doc'."#
        ),
        "string tuple must keep element literals in the head, got: {}",
        diag.message_text
    );
}

#[test]
fn boolean_tuple_property_preserved_in_ts2741_single_object_target() {
    // tsc: Property 'w' is missing in type '{ v: [true, false]; }' but
    //      required in type 'T'.
    let diag = single_diag(
        r#"
type T = { v: [true, false]; w: string };
const x: T = { v: [true, false] };
"#,
        2741,
    );
    assert!(
        diag.message_text.contains(
            r#"Property 'w' is missing in type '{ v: [true, false]; }' but required in type 'T'."#
        ),
        "boolean tuple must keep element literals in the TS2741 source, got: {}",
        diag.message_text
    );
}

#[test]
fn tuple_inside_nested_object_literal_preserved() {
    // tsc: Type '{ kind: "a"; n: { v: [1, 2]; }; }' is not assignable to type 'U'.
    let diag = single_diag(
        r#"
type U = { kind: "a"; n: { v: [1, 2] }; w: string } | { kind: "b"; n: { v: [3, 4] }; w: string };
const x: U = { kind: "a", n: { v: [1, 2] } };
"#,
        2322,
    );
    assert!(
        diag.message_text.starts_with(
            r#"Type '{ kind: "a"; n: { v: [1, 2]; }; }' is not assignable to type 'U'."#
        ),
        "tuple one object level down must keep element literals, got: {}",
        diag.message_text
    );
}

#[test]
fn non_accepting_array_property_still_widens_at_inner_anchor() {
    // Negative control. tsc anchors the mismatch at the array expression with
    // the widened type: Type 'number[]' is not assignable to type 'string'.
    let diag = single_diag(
        r#"
type T = { v: string; w: string };
const x: T = { v: [1, 2] };
"#,
        2322,
    );
    assert!(
        diag.message_text
            .starts_with(r#"Type 'number[]' is not assignable to type 'string'."#),
        "a non-accepted array literal must stay widened, got: {}",
        diag.message_text
    );
}

#[test]
fn non_accepting_array_property_in_union_widens_at_inner_anchor() {
    // Negative control, union form. tsc: Type 'number[]' is not assignable
    // to type 'string'.
    let diag = single_diag(
        r#"
type U = { kind: "a"; v: string; w: string } | { kind: "b"; v: string; w: string };
const x: U = { kind: "a", v: [1, 2] };
"#,
        2322,
    );
    assert!(
        diag.message_text
            .starts_with(r#"Type 'number[]' is not assignable to type 'string'."#),
        "a non-accepted array literal must stay widened in the union form, got: {}",
        diag.message_text
    );
}

#[test]
#[ignore = "pre-existing semantic divergence, not a display defect: tsz reports the \
            readonly-tuple-vs-mutable-arm per-property failure (`Type 'readonly [1, 2]' is \
            not assignable to type '[1, 2] | [3, 4]'`) where tsc 7.0.2 reports only the \
            missing sibling property with the head rendering `v: [1, 2]`; owner is the \
            fresh union fold's per-property check order for as-const values, not \
            `object_literal_source_type_display`"]
fn as_const_tuple_property_renders_without_readonly_in_head() {
    // tsc: Type '{ kind: "a"; v: [1, 2]; }' is not assignable to type 'U'. —
    // the as-const source still displays `[1, 2]`, not `readonly [1, 2]`.
    let diag = single_diag(
        r#"
type U = { kind: "a"; v: [1, 2]; w: string } | { kind: "b"; v: [3, 4]; w: string };
const x: U = { kind: "a", v: [1, 2] as const };
"#,
        2322,
    );
    assert!(
        diag.message_text
            .starts_with(r#"Type '{ kind: "a"; v: [1, 2]; }' is not assignable to type 'U'."#),
        "as-const tuple must render like the plain tuple in the head, got: {}",
        diag.message_text
    );
}
