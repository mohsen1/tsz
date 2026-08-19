//! Property-drill relation-pair frame elaboration (#17687).
//!
//! Structural rule: `tsc`'s chain under `Types of property 'p' are
//! incompatible.` always begins with the *declared property pair's* relation
//! line before drilling the inner failure. Two owners cooperate:
//!
//! * Solver (`explain_union_target.rs`): a union-vs-union relation whose
//!   target is a nullable union with a sole real member wraps the promoted
//!   member failure in `UnionSourceMismatch`, so the pair frame survives —
//!   the walk also skips source members the *whole* union target absorbs
//!   (`undefined` is never the witness against `T | undefined`), explains
//!   type-parameter members against the full union (no best-matching member,
//!   mirroring tsc `getBestMatchingType`), and drills tuple members like
//!   object members.
//! * Checker renderer: header-led nested reasons (tuple element/arity,
//!   index-signature) get the explicit property-pair frame supplied before
//!   the specialized drill, both under a single property header and beneath
//!   the dotted path fold.
//!
//! Every expectation below is oracle-pinned against `tsc` (typescript 6.0/7.0
//! dev, `--strict --target es2020`), byte-for-byte including indentation
//! depth. Property and binder names vary across cases so a fix keyed to a
//! particular spelling cannot satisfy the suite.

use tsz_checker::test_utils::check_source_diagnostics;
use tsz_common::diagnostics::Diagnostic;

fn diagnostics(source: &str) -> Vec<Diagnostic> {
    check_source_diagnostics(source)
}

/// The full chain of the single diagnostic with `code`: the primary message
/// at depth 0 prepended to its related-information `(depth + 1, text)` pairs,
/// asserted exactly.
fn assert_exact_chain(source: &str, code: u32, expected: &[(u8, &str)]) {
    let diags = diagnostics(source);
    let matching: Vec<&Diagnostic> = diags.iter().filter(|d| d.code == code).collect();
    assert_eq!(
        matching.len(),
        1,
        "expected exactly one TS{code}, got {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
    let mut chain = vec![(0u8, matching[0].message_text.clone())];
    chain.extend(
        matching[0]
            .related_information
            .iter()
            .map(|info| (info.depth + 1, info.message_text.clone())),
    );
    let rendered: Vec<(u8, &str)> = chain.iter().map(|(d, m)| (*d, m.as_str())).collect();
    assert_eq!(rendered, expected, "chain mismatch for:\n{source}");
}

// --- Optional-property pair frame (issue #17687 shape 1) -------------------

#[test]
fn optional_property_pair_frame_precedes_reduced_leaf() {
    assert_exact_chain(
        r#"
declare const src: { m?: boolean; v: string };
const z: { m?: string; v: string } = src;
"#,
        2322,
        &[
            (
                0,
                "Type '{ m?: boolean | undefined; v: string; }' is not assignable to type '{ m?: string | undefined; v: string; }'.",
            ),
            (1, "Types of property 'm' are incompatible."),
            (
                2,
                "Type 'boolean | undefined' is not assignable to type 'string | undefined'.",
            ),
            (3, "Type 'boolean' is not assignable to type 'string'."),
        ],
    );
}

#[test]
fn optional_property_pair_frame_renamed_binders() {
    assert_exact_chain(
        r#"
declare const q7src: { wobble?: boolean; keep: string };
const q7dst: { wobble?: string; keep: string } = q7src;
"#,
        2322,
        &[
            (
                0,
                "Type '{ wobble?: boolean | undefined; keep: string; }' is not assignable to type '{ wobble?: string | undefined; keep: string; }'.",
            ),
            (1, "Types of property 'wobble' are incompatible."),
            (
                2,
                "Type 'boolean | undefined' is not assignable to type 'string | undefined'.",
            ),
            (3, "Type 'boolean' is not assignable to type 'string'."),
        ],
    );
}

#[test]
fn nullable_union_property_pair_frame_with_null() {
    assert_exact_chain(
        r#"
declare const src: { m: boolean | null };
const z: { m: string | null } = src;
"#,
        2322,
        &[
            (
                0,
                "Type '{ m: boolean | null; }' is not assignable to type '{ m: string | null; }'.",
            ),
            (1, "Types of property 'm' are incompatible."),
            (
                2,
                "Type 'boolean | null' is not assignable to type 'string | null'.",
            ),
            (3, "Type 'boolean' is not assignable to type 'string'."),
        ],
    );
}

#[test]
fn multi_member_source_union_picks_first_failing_real_member() {
    assert_exact_chain(
        r#"
declare const src: { m?: boolean | number };
const z: { m?: string } = src;
"#,
        2322,
        &[
            (
                0,
                "Type '{ m?: number | boolean | undefined; }' is not assignable to type '{ m?: string | undefined; }'.",
            ),
            (1, "Types of property 'm' are incompatible."),
            (
                2,
                "Type 'number | boolean | undefined' is not assignable to type 'string | undefined'.",
            ),
            (3, "Type 'number' is not assignable to type 'string'."),
        ],
    );
}

// --- The union member witness skips members the full target absorbs --------

#[test]
fn object_member_is_witness_not_undefined() {
    assert_exact_chain(
        r#"
declare const src: { m?: { a: boolean } };
const z: { m?: { a: string } } = src;
"#,
        2322,
        &[
            (
                0,
                "Type '{ m?: { a: boolean; } | undefined; }' is not assignable to type '{ m?: { a: string; } | undefined; }'.",
            ),
            (1, "Types of property 'm' are incompatible."),
            (
                2,
                "Type '{ a: boolean; } | undefined' is not assignable to type '{ a: string; } | undefined'.",
            ),
            (
                3,
                "Type '{ a: boolean; }' is not assignable to type '{ a: string; }'.",
            ),
            (4, "Types of property 'a' are incompatible."),
            (5, "Type 'boolean' is not assignable to type 'string'."),
        ],
    );
}

#[test]
fn missing_property_member_folds_beneath_pair_frame() {
    assert_exact_chain(
        r#"
declare const src: { m?: { a: boolean; b: number } };
const z: { m?: { a: string; c: symbol } } = src;
"#,
        2322,
        &[
            (
                0,
                "Type '{ m?: { a: boolean; b: number; } | undefined; }' is not assignable to type '{ m?: { a: string; c: symbol; } | undefined; }'.",
            ),
            (1, "Types of property 'm' are incompatible."),
            (
                2,
                "Type '{ a: boolean; b: number; } | undefined' is not assignable to type '{ a: string; c: symbol; } | undefined'.",
            ),
            (
                3,
                "Property 'c' is missing in type '{ a: boolean; b: number; }' but required in type '{ a: string; c: symbol; }'.",
            ),
        ],
    );
}

#[test]
fn tuple_member_drills_beneath_pair_frame() {
    assert_exact_chain(
        r#"
declare const src: { m?: [boolean] };
const z: { m?: [string] } = src;
"#,
        2322,
        &[
            (
                0,
                "Type '{ m?: [boolean] | undefined; }' is not assignable to type '{ m?: [string] | undefined; }'.",
            ),
            (1, "Types of property 'm' are incompatible."),
            (
                2,
                "Type '[boolean] | undefined' is not assignable to type '[string] | undefined'.",
            ),
            (3, "Type '[boolean]' is not assignable to type '[string]'."),
            (4, "Type 'boolean' is not assignable to type 'string'."),
        ],
    );
}

// --- Type-parameter members explain against the full union target ----------

#[test]
fn type_parameter_member_keeps_full_union_target() {
    assert_exact_chain(
        r#"
function g<Q>(x: { m?: Q }): void {
  const z: { m?: string } = x;
}
"#,
        2322,
        &[
            (
                0,
                "Type '{ m?: Q | undefined; }' is not assignable to type '{ m?: string | undefined; }'.",
            ),
            (1, "Types of property 'm' are incompatible."),
            (
                2,
                "Type 'Q | undefined' is not assignable to type 'string | undefined'.",
            ),
            (
                3,
                "Type 'Q' is not assignable to type 'string | undefined'.",
            ),
        ],
    );
}

#[test]
fn constrained_type_parameter_member_drills_constraint() {
    assert_exact_chain(
        r#"
function g<Q extends boolean>(x: { m?: Q }): void {
  const z: { m?: string | undefined } = x;
}
"#,
        2322,
        &[
            (
                0,
                "Type '{ m?: Q | undefined; }' is not assignable to type '{ m?: string | undefined; }'.",
            ),
            (1, "Types of property 'm' are incompatible."),
            (
                2,
                "Type 'Q | undefined' is not assignable to type 'string | undefined'.",
            ),
            (
                3,
                "Type 'Q' is not assignable to type 'string | undefined'.",
            ),
            (4, "Type 'boolean' is not assignable to type 'string'."),
        ],
    );
}

// --- Header-led nested reasons get the pair frame (issue #17687 shape 2) ---

#[test]
fn named_property_vs_index_signature_pair_frame() {
    assert_exact_chain(
        r#"
declare const src: { m: { extra: boolean } };
const z: { m: { [k: string]: string } } = src;
"#,
        2322,
        &[
            (
                0,
                "Type '{ m: { extra: boolean; }; }' is not assignable to type '{ m: { [k: string]: string; }; }'.",
            ),
            (1, "Types of property 'm' are incompatible."),
            (
                2,
                "Type '{ extra: boolean; }' is not assignable to type '{ [k: string]: string; }'.",
            ),
            (3, "Property 'extra' is incompatible with index signature."),
            (4, "Type 'boolean' is not assignable to type 'string'."),
        ],
    );
}

#[test]
fn index_vs_index_signature_pair_frame() {
    assert_exact_chain(
        r#"
declare const src: { m: { [k: string]: number } };
const z: { m: { [k: string]: string } } = src;
"#,
        2322,
        &[
            (
                0,
                "Type '{ m: { [k: string]: number; }; }' is not assignable to type '{ m: { [k: string]: string; }; }'.",
            ),
            (1, "Types of property 'm' are incompatible."),
            (
                2,
                "Type '{ [k: string]: number; }' is not assignable to type '{ [k: string]: string; }'.",
            ),
            (3, "'string' index signatures are incompatible."),
            (4, "Type 'number' is not assignable to type 'string'."),
        ],
    );
}

#[test]
fn tuple_under_property_pair_frame() {
    assert_exact_chain(
        r#"
declare const src: { m: [boolean] };
const z: { m: [string] } = src;
"#,
        2322,
        &[
            (
                0,
                "Type '{ m: [boolean]; }' is not assignable to type '{ m: [string]; }'.",
            ),
            (1, "Types of property 'm' are incompatible."),
            (2, "Type '[boolean]' is not assignable to type '[string]'."),
            (3, "Type 'boolean' is not assignable to type 'string'."),
        ],
    );
}

// --- Dotted-path fold keeps the deepest pair frame --------------------------

#[test]
fn folded_path_keeps_optional_pair_frame() {
    assert_exact_chain(
        r#"
declare const src: { outer: { m?: boolean } };
const z: { outer: { m?: string } } = src;
"#,
        2322,
        &[
            (
                0,
                "Type '{ outer: { m?: boolean | undefined; }; }' is not assignable to type '{ outer: { m?: string | undefined; }; }'.",
            ),
            (
                1,
                "The types of 'outer.m' are incompatible between these types.",
            ),
            (
                2,
                "Type 'boolean | undefined' is not assignable to type 'string | undefined'.",
            ),
            (3, "Type 'boolean' is not assignable to type 'string'."),
        ],
    );
}

#[test]
fn folded_path_collapses_index_signature_leaf() {
    assert_exact_chain(
        r#"
declare const src: { outer: { m: { extra: boolean } } };
const z: { outer: { m: { [k: string]: string } } } = src;
"#,
        2322,
        &[
            (
                0,
                "Type '{ outer: { m: { extra: boolean; }; }; }' is not assignable to type '{ outer: { m: { [k: string]: string; }; }; }'.",
            ),
            (
                1,
                "The types of 'outer.m' are incompatible between these types.",
            ),
            (
                2,
                "Type '{ extra: boolean; }' is not assignable to type '{ [k: string]: string; }'.",
            ),
            (3, "Property 'extra' is incompatible with index signature."),
            (4, "Type 'boolean' is not assignable to type 'string'."),
        ],
    );
}

#[test]
fn folded_path_collapses_tuple_leaf() {
    assert_exact_chain(
        r#"
declare const src: { outer: { m: [boolean] } };
const z: { outer: { m: [string] } } = src;
"#,
        2322,
        &[
            (
                0,
                "Type '{ outer: { m: [boolean]; }; }' is not assignable to type '{ outer: { m: [string]; }; }'.",
            ),
            (
                1,
                "The types of 'outer.m' are incompatible between these types.",
            ),
            (2, "Type '[boolean]' is not assignable to type '[string]'."),
            (3, "Type 'boolean' is not assignable to type 'string'."),
        ],
    );
}

// --- Top-level and argument-position surfaces -------------------------------

#[test]
fn top_level_union_pair_elaborates_reduced_member_leaf() {
    assert_exact_chain(
        r#"
declare const s: boolean | undefined;
const t: string | undefined = s;
"#,
        2322,
        &[
            (
                0,
                "Type 'boolean | undefined' is not assignable to type 'string | undefined'.",
            ),
            (1, "Type 'boolean' is not assignable to type 'string'."),
        ],
    );
}

#[test]
fn argument_position_property_pair_frame() {
    assert_exact_chain(
        r#"
declare function f(arg: { m?: string; v: string }): void;
declare const src: { m?: boolean; v: string };
f(src);
"#,
        2345,
        &[
            (
                0,
                "Argument of type '{ m?: boolean | undefined; v: string; }' is not assignable to parameter of type '{ m?: string | undefined; v: string; }'.",
            ),
            (1, "Types of property 'm' are incompatible."),
            (
                2,
                "Type 'boolean | undefined' is not assignable to type 'string | undefined'.",
            ),
            (3, "Type 'boolean' is not assignable to type 'string'."),
        ],
    );
}

#[test]
fn argument_position_top_level_union_leaf_is_reduced() {
    assert_exact_chain(
        r#"
declare const src: boolean | undefined;
declare function h(p: string | undefined): void;
h(src);
"#,
        2345,
        &[
            (
                0,
                "Argument of type 'boolean | undefined' is not assignable to parameter of type 'string | undefined'.",
            ),
            (1, "Type 'boolean' is not assignable to type 'string'."),
        ],
    );
}

// --- Negative controls: shapes that must NOT gain a frame -------------------

#[test]
fn plain_scalar_property_keeps_single_leaf() {
    assert_exact_chain(
        r#"
declare const src: { m: boolean };
const z: { m: string } = src;
"#,
        2322,
        &[
            (
                0,
                "Type '{ m: boolean; }' is not assignable to type '{ m: string; }'.",
            ),
            (1, "Types of property 'm' are incompatible."),
            (2, "Type 'boolean' is not assignable to type 'string'."),
        ],
    );
}

#[test]
fn optional_source_against_required_target_keeps_existing_chain() {
    assert_exact_chain(
        r#"
declare const src: { m?: number };
const z: { m: number } = src;
"#,
        2322,
        &[
            (
                0,
                "Type '{ m?: number | undefined; }' is not assignable to type '{ m: number; }'.",
            ),
            (1, "Types of property 'm' are incompatible."),
            (
                2,
                "Type 'number | undefined' is not assignable to type 'number'.",
            ),
            (3, "Type 'undefined' is not assignable to type 'number'."),
        ],
    );
}

#[test]
fn scalar_source_against_nullable_target_keeps_reduced_leaf() {
    assert_exact_chain(
        r#"
declare const src: { m: boolean };
const z: { m: string | undefined } = src;
"#,
        2322,
        &[
            (
                0,
                "Type '{ m: boolean; }' is not assignable to type '{ m: string | undefined; }'.",
            ),
            (1, "Types of property 'm' are incompatible."),
            (2, "Type 'boolean' is not assignable to type 'string'."),
        ],
    );
}

#[test]
fn multi_real_member_union_target_keeps_full_pair() {
    assert_exact_chain(
        r#"
declare const src: { m: boolean };
const z: { m: string | number } = src;
"#,
        2322,
        &[
            (
                0,
                "Type '{ m: boolean; }' is not assignable to type '{ m: string | number; }'.",
            ),
            (1, "Types of property 'm' are incompatible."),
            (
                2,
                "Type 'boolean' is not assignable to type 'string | number'.",
            ),
        ],
    );
}

#[test]
fn single_object_member_source_keeps_folded_path() {
    assert_exact_chain(
        r#"
declare const src: { m: { x: boolean } };
const z: { m: { x: string } | undefined } = src;
"#,
        2322,
        &[
            (
                0,
                "Type '{ m: { x: boolean; }; }' is not assignable to type '{ m: { x: string; } | undefined; }'.",
            ),
            (
                1,
                "The types of 'm.x' are incompatible between these types.",
            ),
            (2, "Type 'boolean' is not assignable to type 'string'."),
        ],
    );
}

#[test]
fn top_level_scalar_against_nullable_union_stays_single_line() {
    assert_exact_chain(
        r#"
declare const b: boolean;
const t: string | undefined = b;
"#,
        2322,
        &[(0, "Type 'boolean' is not assignable to type 'string'.")],
    );
}
