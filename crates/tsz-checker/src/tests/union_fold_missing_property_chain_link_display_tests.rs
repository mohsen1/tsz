//! Fresh-source display in the union-fold missing-property chain link
//! (the #17796 handoff residual).
//!
//! Structural rule: `tsc` renders every relation line's source with
//! `typeToString` of the same checked type, so when a fresh object-literal
//! source fails against a union target and the fold elaborates the
//! best/discriminant-matched member's missing required property, the chained
//! `Property 'x' is missing in type '_' …` (and plural `Type '_' is missing
//! the following properties …`) link shows the identical fresh render the
//! head used. Per-property widening is decided by the contextual target
//! property (`id: 1` widens against `id: number`, `v: 1` stays literal
//! against `v: 1`), never by a wholesale widening of the fresh literal. tsz
//! owns this at the checker diagnostic display boundary:
//! `nested_fresh_object_literal_chain_source_display` reuses the head's
//! syntax-driven renderer for a depth > 0 frame that re-describes the
//! anchored literal, and every other nested frame keeps the widened
//! rendering.
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

fn assert_chain_link(diag: &Diagnostic, expected: &str) {
    assert!(
        diag.related_information
            .iter()
            .any(|r| r.message_text == expected),
        "expected chain link `{expected}`, got: {:?}",
        diag.related_information
    );
}

#[test]
fn literal_discriminant_source_stays_fresh_in_chain_link() {
    // tsc: Type '{ kind: "a"; v: 1; }' is not assignable to type 'U1'.
    //        Property 'w' is missing in type '{ kind: "a"; v: 1; }' but
    //        required in type '{ kind: "a"; v: 1; w: string; }'.
    let diag = single_diag(
        r#"
type U1 = { kind: "a"; v: 1; w: string } | { kind: "b"; v: 2; w: string };
const a1: U1 = { kind: "a", v: 1 };
"#,
        2322,
    );
    assert!(
        diag.message_text
            .starts_with(r#"Type '{ kind: "a"; v: 1; }' is not assignable to type 'U1'."#),
        "head must keep the fresh render, got: {}",
        diag.message_text
    );
    assert_chain_link(
        &diag,
        r#"Property 'w' is missing in type '{ kind: "a"; v: 1; }' but required in type '{ kind: "a"; v: 1; w: string; }'."#,
    );
}

#[test]
fn renamed_binders_source_stays_fresh_in_chain_link() {
    // tsc: Property 'label' is missing in type '{ tag: "circle"; r: 3; }' but
    //      required in type '{ tag: "circle"; r: 3; label: string; }'.
    let diag = single_diag(
        r#"
type Shape = { tag: "circle"; r: 3; label: string } | { tag: "rect"; r: 4; label: string };
const s2: Shape = { tag: "circle", r: 3 };
"#,
        2322,
    );
    assert_chain_link(
        &diag,
        r#"Property 'label' is missing in type '{ tag: "circle"; r: 3; }' but required in type '{ tag: "circle"; r: 3; label: string; }'."#,
    );
}

#[test]
fn argument_path_source_stays_fresh_in_chain_link() {
    // tsc: Argument of type '{ kind: "a"; v: 1; }' is not assignable to
    //      parameter of type 'U5'.
    //        Property 'w' is missing in type '{ kind: "a"; v: 1; }' but
    //        required in type '{ kind: "a"; v: 1; w: string; }'.
    let diag = single_diag(
        r#"
type U5 = { kind: "a"; v: 1; w: string } | { kind: "b"; v: 2; w: string };
declare function sink(u: U5): void;
sink({ kind: "a", v: 1 });
"#,
        2345,
    );
    assert_chain_link(
        &diag,
        r#"Property 'w' is missing in type '{ kind: "a"; v: 1; }' but required in type '{ kind: "a"; v: 1; w: string; }'."#,
    );
}

#[test]
fn plural_missing_properties_source_stays_fresh_in_chain_link() {
    // tsc: Type '{ kind: "a"; v: 1; }' is missing the following properties
    //      from type '{ kind: "a"; v: 1; w: string; z: number; }': w, z
    let diag = single_diag(
        r#"
type U6 = { kind: "a"; v: 1; w: string; z: number } | { kind: "b"; v: 2; w: string; z: number };
const c6: U6 = { kind: "a", v: 1 };
"#,
        2322,
    );
    assert_chain_link(
        &diag,
        r#"Type '{ kind: "a"; v: 1; }' is missing the following properties from type '{ kind: "a"; v: 1; w: string; z: number; }': w, z"#,
    );
}

#[test]
fn nested_object_value_stays_contextual_in_chain_link() {
    // tsc: Property 'req' is missing in type '{ k: "x"; inner: { p: 1; }; }'
    //      but required in type '{ k: "x"; inner: { p: 1; }; req: string; }'.
    let diag = single_diag(
        r#"
type N = { k: "x"; inner: { p: 1 }; req: string } | { k: "y"; inner: { p: 2 }; req: string };
const n3: N = { k: "x", inner: { p: 1 } };
"#,
        2322,
    );
    assert_chain_link(
        &diag,
        r#"Property 'req' is missing in type '{ k: "x"; inner: { p: 1; }; }' but required in type '{ k: "x"; inner: { p: 1; }; req: string; }'."#,
    );
}

#[test]
fn non_literal_contextual_property_still_widens_in_chain_link() {
    // The per-property rule, not a blanket freshness pin: `id: 1` against the
    // arm's `id: number` widens in head and chain alike.
    // tsc: Property 'w' is missing in type '{ kind: "a"; id: number; }' but
    //      required in type '{ kind: "a"; id: number; w: string; }'.
    let diag = single_diag(
        r#"
type U7 = { kind: "a"; id: number; w: string } | { kind: "b"; id: string; w: string };
const d7: U7 = { kind: "a", id: 1 };
"#,
        2322,
    );
    assert_chain_link(
        &diag,
        r#"Property 'w' is missing in type '{ kind: "a"; id: number; }' but required in type '{ kind: "a"; id: number; w: string; }'."#,
    );
}

#[test]
fn non_fresh_source_rendering_unchanged() {
    // Negative control: a declared (non-fresh) source never widened, and must
    // not be repainted by the fresh-render route either.
    // tsc: Property 'w' is missing in type '{ kind: "a"; v: 1; }' but
    //      required in type '{ kind: "a"; v: 1; w: string; }'.
    let diag = single_diag(
        r#"
type U4 = { kind: "a"; v: 1; w: string } | { kind: "b"; v: 2; w: string };
declare const pre: { kind: "a"; v: 1 };
const b4: U4 = pre;
"#,
        2322,
    );
    assert_chain_link(
        &diag,
        r#"Property 'w' is missing in type '{ kind: "a"; v: 1; }' but required in type '{ kind: "a"; v: 1; w: string; }'."#,
    );
}

#[test]
fn genuinely_nested_frame_keeps_inner_source() {
    // Negative control: a property value's own missing-property frame
    // describes the INNER literal — the anchored-literal reuse must not
    // repaint it with the outer object.
    // tsc: Types of property 'm' are incompatible.
    //        Property 'c' is missing in type '{ b: string; }' but required in
    //        type '{ b: string; c: number; }'.
    let diag = single_diag(
        r#"
type V8 = { k: "a"; m: { b: string; c: number } } | { k: "b"; m: { b: string } };
const v8: V8 = { k: "a", m: { b: "s" } };
"#,
        2322,
    );
    assert_chain_link(
        &diag,
        r#"Property 'c' is missing in type '{ b: string; }' but required in type '{ b: string; c: number; }'."#,
    );
}
