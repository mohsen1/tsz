//! Union-target structural (non-missing-property) elaboration
//! (TS2322/TS2345 + nested reason).
//!
//! Structural rule: when a non-fresh value fails to assign to a *union*
//! target, tsc selects the best-matching constituent (`getBestMatchingType`:
//! a written unit discriminant first, then `findMostOverlappyType` — most
//! shared property-name keys, ties to the *last* such member) and re-runs the
//! failed relation against it with errors enabled, so the chain continues
//! past the union head for every failure kind:
//!
//! ```text
//! Type 'S' is not assignable to type '<union>'.
//!   Type 'S' is not assignable to type '<member>'.
//!     Types of property 'm' are incompatible.
//!       Type 'boolean' is not assignable to type 'string'.
//! ```
//!
//! tsz previously carried the elaboration only when the best member failed
//! through a missing required property; a property-*type* mismatch collapsed
//! to the bare union head line. tsz now elaborates through
//! `SubtypeFailureReason::UnionTargetMismatch` (solver `explain.rs`) and the
//! checker's member-frame renderer. A missing required property keeps the
//! folded form (no member frame); a fresh object literal keeps the checker's
//! per-property expression elaboration (`try_elaborate_assignment_source_error`)
//! and never reaches this reason.
//!
//! Every expectation below is oracle-pinned against `tsc` (typescript 7.0.2,
//! `--strict --target es2020`). Tests vary property, binder, and member names
//! so a fix keyed to a particular spelling would not satisfy them.

use tsz_checker::test_utils::check_source_diagnostics;
use tsz_common::diagnostics::Diagnostic;

fn diagnostics(source: &str) -> Vec<Diagnostic> {
    check_source_diagnostics(source)
}

/// The related-information chain of the first diagnostic with `code`, as
/// `(depth, message_text)` pairs in emission order.
fn chain_of(diags: &[Diagnostic], code: u32) -> Vec<(u8, String)> {
    diags
        .iter()
        .find(|d| d.code == code)
        .map(|d| {
            d.related_information
                .iter()
                .map(|info| (info.depth, info.message_text.clone()))
                .collect()
        })
        .unwrap_or_default()
}

/// Assert the chain contains, in order, entries whose depth and substring
/// match `expected` (other entries may be interleaved before/after but the
/// relative order must hold).
fn assert_chain_contains(diags: &[Diagnostic], code: u32, expected: &[(u8, &str)]) {
    let chain = chain_of(diags, code);
    let mut pos = 0usize;
    for (depth, needle) in expected {
        let found = chain
            .iter()
            .skip(pos)
            .position(|(d, text)| d == depth && text.contains(needle));
        match found {
            Some(offset) => pos += offset + 1,
            None => panic!(
                "expected chain entry (depth {depth}, contains {needle:?}) \
                 in order; full chain for TS{code}: {chain:?}\nall: {diags:?}"
            ),
        }
    }
}

/// A declared (non-fresh) source failing a two-arm object union through a
/// property-type mismatch elaborates member frame -> property header -> leaf.
#[test]
fn variable_source_property_type_mismatch_elaborates_member_frame() {
    let diags = diagnostics(
        r#"
declare const src: { m: boolean; v: string };
const z: { m: string; v: string } | { m: number; w: number } = src;
"#,
    );
    assert_chain_contains(
        &diags,
        2322,
        &[
            (0, "is not assignable to type '{ m: string; v: string; }'"),
            (1, "Types of property 'm' are incompatible"),
            (2, "Type 'boolean' is not assignable to type 'string'"),
        ],
    );
}

/// Enum-typed property flavor of the same rule: the widened enum source fails
/// the member-typed arm and the leaf names the enum relation.
#[test]
fn enum_property_mismatch_elaborates_through_best_arm() {
    let diags = diagnostics(
        r#"
enum Mode { A, B }
declare const src: { m: Mode; v: string };
const z: { m: Mode.A; v: string } | { m: Mode.B; w: number } = src;
"#,
    );
    assert_chain_contains(
        &diags,
        2322,
        &[
            (0, "is not assignable to type '{ m: Mode.A; v: string; }'"),
            (1, "Types of property 'm' are incompatible"),
            (2, "Type 'Mode' is not assignable to type 'Mode.A'"),
        ],
    );
}

/// Renamed binders/properties: no dependence on any particular spelling.
#[test]
fn renamed_binders_still_elaborate() {
    let diags = diagnostics(
        r#"
declare const other: { alpha: boolean; beta: string };
const q: { alpha: string; beta: string } | { alpha: number; gamma: number } = other;
"#,
    );
    assert_chain_contains(
        &diags,
        2322,
        &[
            (
                0,
                "is not assignable to type '{ alpha: string; beta: string; }'",
            ),
            (1, "Types of property 'alpha' are incompatible"),
            (2, "Type 'boolean' is not assignable to type 'string'"),
        ],
    );
}

/// Three arms: the most-overlappy member (two shared keys beats one) is the
/// frame target.
#[test]
fn most_overlappy_member_selected_across_three_arms() {
    let diags = diagnostics(
        r#"
declare const src: { tag: boolean; a: string; b: string };
const z: { tag: string; a: string; b: string } | { tag: number; a: string } | { tag: symbol } = src;
"#,
    );
    assert_chain_contains(
        &diags,
        2322,
        &[
            (
                0,
                "is not assignable to type '{ tag: string; a: string; b: string; }'",
            ),
            (1, "Types of property 'tag' are incompatible"),
        ],
    );
}

/// Equal-overlap tie breaks to the *last* member (tsc compares with `>=`).
#[test]
fn equal_overlap_tie_selects_last_member() {
    let diags = diagnostics(
        r#"
declare const src: { m: boolean };
const z: { m: string } | { m: number } = src;
"#,
    );
    assert_chain_contains(
        &diags,
        2322,
        &[
            (0, "is not assignable to type '{ m: number; }'"),
            (1, "Types of property 'm' are incompatible"),
            (2, "Type 'boolean' is not assignable to type 'number'"),
        ],
    );
}

/// A written unit discriminant overrides raw key overlap: `kind: "b"` selects
/// the second arm even though the first shares more keys.
#[test]
fn discriminant_overrides_key_overlap() {
    let diags = diagnostics(
        r#"
declare const src: { kind: "b"; x: boolean; y: boolean; z: boolean };
const z: { kind: "a"; x: string; y: string; z: string } | { kind: "b"; q: number } = src;
"#,
    );
    // The discriminant arm fails through a missing `q`, so the fold names it —
    // proving the discriminant match beat the three-shared-key first arm
    // (which would have produced a `Types of property 'x'` chain instead).
    assert_chain_contains(
        &diags,
        2322,
        &[(
            0,
            "Property 'q' is missing in type '{ kind: \"b\"; x: boolean; y: boolean; z: boolean; }' \
             but required in type '{ kind: \"b\"; q: number; }'",
        )],
    );
}

/// Discriminant selection with a property-*type* failure exercises the frame
/// path against the discriminant arm (not the three-shared-key arm).
#[test]
fn discriminant_arm_property_mismatch_frames_discriminant_arm() {
    let diags = diagnostics(
        r#"
declare const src: { kind: "b"; x: boolean; y: boolean; z: boolean };
const z: { kind: "a"; x: string; y: string; z: string } | { kind: "b"; x: number } = src;
"#,
    );
    assert_chain_contains(
        &diags,
        2322,
        &[
            (0, "is not assignable to type '{ kind: \"b\"; x: number; }'"),
            (1, "Types of property 'x' are incompatible"),
            (2, "Type 'boolean' is not assignable to type 'number'"),
        ],
    );
}

/// A nested object property keeps tsc's path-compressed drill form.
#[test]
fn nested_property_chain_path_compresses() {
    let diags = diagnostics(
        r#"
declare const src: { m: { inner: boolean }; v: string };
const z: { m: { inner: string }; v: string } | { m: number; w: number } = src;
"#,
    );
    assert_chain_contains(
        &diags,
        2322,
        &[
            (
                0,
                "is not assignable to type '{ m: { inner: string; }; v: string; }'",
            ),
            (
                1,
                "The types of 'm.inner' are incompatible between these types",
            ),
            (2, "Type 'boolean' is not assignable to type 'string'"),
        ],
    );
}

/// Union member types reached through aliases keep their alias display in the
/// member frame.
#[test]
fn alias_members_keep_alias_display_in_frame() {
    let diags = diagnostics(
        r#"
type Left = { m: string; v: string };
type Right = { m: number; w: number };
type Both = Left | Right;
declare const src: { m: boolean; v: string };
const z: Both = src;
"#,
    );
    assert_chain_contains(
        &diags,
        2322,
        &[
            (0, "is not assignable to type 'Left'"),
            (1, "Types of property 'm' are incompatible"),
            (2, "Type 'boolean' is not assignable to type 'string'"),
        ],
    );
}

/// Argument position: the same chain re-anchors onto the TS2345 surface.
#[test]
fn argument_position_reanchors_chain_on_ts2345() {
    let diags = diagnostics(
        r#"
declare const src: { m: boolean; v: string };
declare function f(x: { m: string; v: string } | { m: number; w: number }): void;
f(src);
"#,
    );
    assert_chain_contains(
        &diags,
        2345,
        &[
            (0, "is not assignable to type '{ m: string; v: string; }'"),
            (1, "Types of property 'm' are incompatible"),
            (2, "Type 'boolean' is not assignable to type 'string'"),
        ],
    );
}

/// A union *source* against a union target elaborates the first failing
/// source member against the whole union, then that member's own
/// union-target chain (member header -> member frame -> drill).
#[test]
fn union_source_member_elaborates_against_union_target() {
    let diags = diagnostics(
        r#"
declare const src: { m: boolean; v: string } | { m: symbol; v: string };
const z: { m: string; v: string } | { m: number; w: number } = src;
"#,
    );
    assert_chain_contains(
        &diags,
        2322,
        &[
            (
                0,
                "Type '{ m: boolean; v: string; }' is not assignable to type \
                 '{ m: string; v: string; } | { m: number; w: number; }'",
            ),
            (
                1,
                "Type '{ m: boolean; v: string; }' is not assignable to type \
                 '{ m: string; v: string; }'",
            ),
            (2, "Types of property 'm' are incompatible"),
            (3, "Type 'boolean' is not assignable to type 'string'"),
        ],
    );
}

/// Negative control: no key overlap -> no best member -> the bare union head
/// stays alone, exactly as before.
#[test]
fn no_overlap_keeps_bare_union_line() {
    let diags = diagnostics(
        r#"
declare const src: { p: boolean };
const z: { m: string } | { w: number } = src;
"#,
    );
    let chain = chain_of(&diags, 2322);
    assert!(
        chain.is_empty(),
        "expected the bare union head with no elaboration when no member \
         shares a key with the source; got {chain:?}"
    );
}

/// Negative control: a missing required property keeps the folded form — the
/// missing-property line directly beneath the head, with no member frame.
#[test]
fn missing_property_stays_folded_without_member_frame() {
    let diags = diagnostics(
        r#"
declare const src: { m: string };
const z: { m: string; v: string } | { m: number; w: number } = src;
"#,
    );
    assert_chain_contains(&diags, 2322, &[(0, "Property 'w' is missing in type")]);
    let chain = chain_of(&diags, 2322);
    assert!(
        !chain
            .iter()
            .any(|(_, text)| text.contains("is not assignable to type '{ m: number; w: number; }'")),
        "missing-property fold must not gain a member frame; got {chain:?}"
    );
}

/// Negative control: a fresh object literal keeps the checker's per-property
/// expression elaboration (error at the property initializer against the
/// union of per-arm property types), not the union-head chain.
#[test]
fn fresh_object_literal_keeps_property_site_elaboration() {
    let diags = diagnostics(
        r#"
declare const b: boolean;
const z: { m: string; v: string } | { m: number; w: number } = { m: b, v: "x" };
"#,
    );
    let has_property_site = diags.iter().any(|d| {
        d.code == 2322
            && d.message_text
                .contains("Type 'boolean' is not assignable to type 'string | number'")
    });
    assert!(
        has_property_site,
        "expected the fresh-literal property-site TS2322 against \
         'string | number'; got {diags:?}"
    );
    let chain = chain_of(&diags, 2322);
    assert!(
        !chain
            .iter()
            .any(|(_, text)| text.contains("Types of property 'm' are incompatible")),
        "fresh literal must not gain the union-head member chain; got {diags:?}"
    );
}

/// Clean control: a source matching one arm produces no diagnostic at all.
#[test]
fn assignable_source_stays_clean() {
    let diags = diagnostics(
        r#"
declare const src: { m: "x"; v: string };
const z: { m: string; v: string } | { m: number; w: number } = src;
const ok: { m: number; w: number } | { m: string; v: string } = { m: 3, w: 4 };
"#,
    );
    assert!(
        diags.iter().all(|d| d.code != 2322),
        "expected no TS2322 for assignable sources; got {diags:?}"
    );
}
