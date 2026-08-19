//! Union-source `boolean` witness splitting and the multi-real-member
//! union-target member line (#17700 residual, t16/t17 family).
//!
//! Structural rule: `tsc`'s `booleanType` is a primitive union of the two
//! boolean literals, so its union-source walk (`eachTypeRelatedToType`)
//! relates `false` and `true` separately — the failing witness of a
//! `boolean` member is its first failing literal half, and the display-side
//! literal generalization (`reportRelationError`) widens it back to
//! `boolean` only when the leaf target holds no top-level singleton types.
//! The same walk reports the first failing constituent against the *whole*
//! union target when no best-matching member exists, rather than stopping at
//! the bare union head line. Owners:
//!
//! * Solver (`explain_union_target.rs`): refines a failing `boolean` member
//!   of a genuine multi-member union source to its failing literal half, and
//!   emits the first-failing-member line for multi-real-member union targets
//!   with no best-matching member.
//! * Checker renderer (`fingerprint_policy.rs`): the TS2345 hand-rolled
//!   chain's member leaf applies the same literal-source generalization the
//!   TS2322 renderer already runs.
//!
//! Every expectation below is oracle-pinned against `tsc` 6.0.2
//! (`--strict --target es2020`), byte-for-byte including indentation depth.
//! Property and binder names vary across cases so a fix keyed to a
//! particular spelling cannot satisfy the suite.

use tsz_checker::test_utils::{check_with_options, strict_checker_options};
use tsz_common::diagnostics::Diagnostic;

fn diagnostics(source: &str) -> Vec<Diagnostic> {
    check_with_options(source, strict_checker_options())
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

// --- Sole-real-member targets: the failing literal half is the witness ----

#[test]
fn sole_real_true_target_splits_false_witness() {
    assert_exact_chain(
        r#"
declare const alpha: { fl: boolean | undefined };
declare let beta: { fl: true | undefined };
beta = alpha;
"#,
        2322,
        &[
            (
                0,
                "Type '{ fl: boolean | undefined; }' is not assignable to type '{ fl: true | undefined; }'.",
            ),
            (1, "Types of property 'fl' are incompatible."),
            (
                2,
                "Type 'boolean | undefined' is not assignable to type 'true | undefined'.",
            ),
            (3, "Type 'false' is not assignable to type 'true'."),
        ],
    );
}

#[test]
fn alias_of_true_target_splits_false_witness() {
    assert_exact_chain(
        r#"
type OnState = true;
declare const src: { mode: boolean | undefined };
declare let dst: { mode: OnState | undefined };
dst = src;
"#,
        2322,
        &[
            (
                0,
                "Type '{ mode: boolean | undefined; }' is not assignable to type '{ mode: true | undefined; }'.",
            ),
            (1, "Types of property 'mode' are incompatible."),
            (
                2,
                "Type 'boolean | undefined' is not assignable to type 'true | undefined'.",
            ),
            (3, "Type 'false' is not assignable to type 'true'."),
        ],
    );
}

#[test]
fn false_literal_target_splits_true_witness() {
    assert_exact_chain(
        r#"
declare const inp: { off: boolean | undefined };
declare let out: { off: false | undefined };
out = inp;
"#,
        2322,
        &[
            (
                0,
                "Type '{ off: boolean | undefined; }' is not assignable to type '{ off: false | undefined; }'.",
            ),
            (1, "Types of property 'off' are incompatible."),
            (
                2,
                "Type 'boolean | undefined' is not assignable to type 'false | undefined'.",
            ),
            (3, "Type 'true' is not assignable to type 'false'."),
        ],
    );
}

#[test]
fn toplevel_union_source_splits_false_witness() {
    assert_exact_chain(
        r#"
declare const lhs: boolean | undefined;
declare let rhs: true | undefined;
rhs = lhs;
"#,
        2322,
        &[
            (
                0,
                "Type 'boolean | undefined' is not assignable to type 'true | undefined'.",
            ),
            (1, "Type 'false' is not assignable to type 'true'."),
        ],
    );
}

#[test]
fn string_literal_singleton_target_keeps_false_witness() {
    assert_exact_chain(
        r#"
declare const has: { tag: boolean | undefined };
declare let want: { tag: "a" | undefined };
want = has;
"#,
        2322,
        &[
            (
                0,
                "Type '{ tag: boolean | undefined; }' is not assignable to type '{ tag: \"a\" | undefined; }'.",
            ),
            (1, "Types of property 'tag' are incompatible."),
            (
                2,
                "Type 'boolean | undefined' is not assignable to type '\"a\" | undefined'.",
            ),
            (3, "Type 'false' is not assignable to type '\"a\"'."),
        ],
    );
}

#[test]
fn nonsingleton_target_recombines_boolean_witness() {
    // Both literal halves fail and `string` holds no singleton, so the
    // witness generalizes back to `boolean` — tsc's display for the same
    // walk. Regression fence: the split must not leak `false` here.
    assert_exact_chain(
        r#"
declare const from: { txt: boolean | undefined };
declare let into: { txt: string | undefined };
into = from;
"#,
        2322,
        &[
            (
                0,
                "Type '{ txt: boolean | undefined; }' is not assignable to type '{ txt: string | undefined; }'.",
            ),
            (1, "Types of property 'txt' are incompatible."),
            (
                2,
                "Type 'boolean | undefined' is not assignable to type 'string | undefined'.",
            ),
            (3, "Type 'boolean' is not assignable to type 'string'."),
        ],
    );
}

// --- Multi-real-member union targets: the member line is emitted ----------

#[test]
fn multi_real_member_target_emits_boolean_member_line() {
    assert_exact_chain(
        r#"
declare const give: { val: boolean | string };
declare let need: { val: string | number };
need = give;
"#,
        2322,
        &[
            (
                0,
                "Type '{ val: string | boolean; }' is not assignable to type '{ val: string | number; }'.",
            ),
            (1, "Types of property 'val' are incompatible."),
            (
                2,
                "Type 'string | boolean' is not assignable to type 'string | number'.",
            ),
            (
                3,
                "Type 'boolean' is not assignable to type 'string | number'.",
            ),
        ],
    );
}

#[test]
fn multi_real_member_true_target_emits_false_member_line() {
    assert_exact_chain(
        r#"
declare const pick: { opt: boolean | string };
declare let slot: { opt: true | string };
slot = pick;
"#,
        2322,
        &[
            (
                0,
                "Type '{ opt: string | boolean; }' is not assignable to type '{ opt: string | true; }'.",
            ),
            (1, "Types of property 'opt' are incompatible."),
            (
                2,
                "Type 'string | boolean' is not assignable to type 'string | true'.",
            ),
            (3, "Type 'false' is not assignable to type 'string | true'."),
        ],
    );
}

#[test]
fn multi_real_member_first_failing_nonboolean_member_line() {
    assert_exact_chain(
        r#"
declare const raw: { num: string | boolean };
declare let fit: { num: true | number };
fit = raw;
"#,
        2322,
        &[
            (
                0,
                "Type '{ num: string | boolean; }' is not assignable to type '{ num: number | true; }'.",
            ),
            (1, "Types of property 'num' are incompatible."),
            (
                2,
                "Type 'string | boolean' is not assignable to type 'number | true'.",
            ),
            (
                3,
                "Type 'string' is not assignable to type 'number | true'.",
            ),
        ],
    );
}

// --- Argument position (TS2345 hand-rolled chain) -------------------------

#[test]
fn argument_position_true_target_splits_false_witness() {
    assert_exact_chain(
        r#"
declare function accept(x: { on: true | undefined }): void;
declare const cfg: { on: boolean | undefined };
accept(cfg);
"#,
        2345,
        &[
            (
                0,
                "Argument of type '{ on: boolean | undefined; }' is not assignable to parameter of type '{ on: true | undefined; }'.",
            ),
            (1, "Types of property 'on' are incompatible."),
            (
                2,
                "Type 'boolean | undefined' is not assignable to type 'true | undefined'.",
            ),
            (3, "Type 'false' is not assignable to type 'true'."),
        ],
    );
}

#[test]
fn argument_position_literal_leaf_generalizes_to_base() {
    // Pre-existing adjacent defect in the same family: the TS2345 chain's
    // member leaf skipped tsc's literal-source generalization entirely
    // (`"x"` stayed `"x"` against a singleton-free `number` target).
    assert_exact_chain(
        r#"
declare function ingest(x: { key: number | undefined }): void;
declare const row: { key: "x" | undefined };
ingest(row);
"#,
        2345,
        &[
            (
                0,
                "Argument of type '{ key: \"x\" | undefined; }' is not assignable to parameter of type '{ key: number | undefined; }'.",
            ),
            (1, "Types of property 'key' are incompatible."),
            (
                2,
                "Type '\"x\" | undefined' is not assignable to type 'number | undefined'.",
            ),
            (3, "Type 'string' is not assignable to type 'number'."),
        ],
    );
}

// --- Negative / gate controls ---------------------------------------------

#[test]
fn bare_boolean_source_keeps_boolean_witness() {
    // A bare `boolean` source is a primitive union in tsc, whose walk never
    // reports per constituent — the witness stays `boolean` even though only
    // the `false` half fails against `true`.
    assert_exact_chain(
        r#"
declare const one: { b: boolean };
declare let two: { b: true | undefined };
two = one;
"#,
        2322,
        &[
            (
                0,
                "Type '{ b: boolean; }' is not assignable to type '{ b: true | undefined; }'.",
            ),
            (1, "Types of property 'b' are incompatible."),
            (2, "Type 'boolean' is not assignable to type 'true'."),
        ],
    );
}

#[test]
fn absorbed_boolean_member_reports_nothing() {
    let diags = diagnostics(
        r#"
declare const okA: { p: boolean | undefined };
declare let okB: { p: boolean | string | undefined };
okB = okA;
declare const okC: { q: boolean };
declare let okD: { q: true | false };
okD = okC;
"#,
    );
    assert!(
        diags.is_empty(),
        "expected no diagnostics, got {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn enum_in_union_walk_keeps_first_failing_member() {
    // Control: an enum arm inside a written union stays whole and the walk's
    // first failing member is unchanged by the boolean split.
    assert_exact_chain(
        r#"
enum Level { Low, High }
declare const mix: { lv: Level | string };
declare let tgt: { lv: number | undefined };
tgt = mix;
"#,
        2322,
        &[
            (
                0,
                "Type '{ lv: string | Level; }' is not assignable to type '{ lv: number | undefined; }'.",
            ),
            (1, "Types of property 'lv' are incompatible."),
            (
                2,
                "Type 'string | Level' is not assignable to type 'number | undefined'.",
            ),
            (3, "Type 'string' is not assignable to type 'number'."),
        ],
    );
}
