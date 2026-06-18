//! Witness matrix for the linear-pass-through chase's ASSIGNMENT *relevance*
//! classification (`chase_linear_passthrough` in
//! `crates/tsz-checker/src/flow/control_flow/core/flow_traversal.rs`), now
//! memoized per walk by flow-node id (issue #13311 `fast` slice, completing the
//! chase-classification memoization started in #13683).
//!
//! Structural rule:
//!
//! > When a backward flow walk crosses a straight-line run of ASSIGNMENT flow
//! > nodes that neither *target* nor *affect* the queried reference, each node's
//! > flow type equals its antecedent's, so the chase splices the run and the
//! > narrowing established before it is preserved unchanged. A node that DOES
//! > target or affect the reference (a killing definition or a base/property
//! > reassignment) stops the chase and re-establishes the assignment-narrowed
//! > type. Memoizing the relevance decision by flow-node id is a per-walk-pure
//! > transform: `reference`/`symbol_id` are fixed within a walk, so the cached
//! > verdict is byte-identical to recomputing it on every chase re-scan.
//!
//! Each case below uses distinct binder / parameter / property names so the
//! behavior follows the structural shape, not any identifier spelling
//! (CLAUDE.md anti-hardcoding gate). Both the plain-identifier reference path
//! (`symbol_id` present) and the member-access path (`symbol_id` absent) are
//! covered, in both the narrowing-preserved and narrowing-cleared directions,
//! and with a branch merge that re-schedules interior run nodes so the chase
//! re-scans the run (the case the memo is meant to collapse).

use tsz_checker::test_utils::check_source_codes;

const TS2322_ASSIGNMENT: u32 = 2322;

/// The guard read after the pass-through run is still narrowed, so the trailing
/// return is assignable to the `string` result and emits no TS2322.
fn assert_narrowing_preserved(source: &str) {
    let diags = check_source_codes(source);
    assert!(
        !diags.contains(&TS2322_ASSIGNMENT),
        "narrowing must survive the pass-through run; got: {diags:?}",
    );
}

/// A relevant assignment inside the run re-establishes the assignment-narrowed
/// type, so the trailing return is no longer assignable and TS2322 fires. If the
/// relevance memo wrongly served "irrelevant" the chase would splice past the
/// assignment and the error would vanish.
fn assert_narrowing_cleared(source: &str) {
    let diags = check_source_codes(source);
    assert!(
        diags.contains(&TS2322_ASSIGNMENT),
        "a relevant assignment in the run must clear narrowing (expected TS2322); got: {diags:?}",
    );
}

// =========================================================================
// Narrowing is PRESERVED across an irrelevant pass-through assignment run.
// =========================================================================

/// A long run of independent `const` declarations sits between a `typeof`
/// guard and a later read of the narrowed identifier. None of the `const`s
/// target or affect `value`, so the chase splices them and `value` stays
/// narrowed to `string` — `return value` is assignable to the `string`
/// result and emits no TS2322.
#[test]
fn identifier_narrowing_survives_long_const_run() {
    assert_narrowing_preserved(
        r#"
function widenFirst(value: string | number): string {
    if (typeof value !== "string") return "";
    const p0 = 0; const p1 = 1; const p2 = 2; const p3 = 3; const p4 = 4;
    const p5 = 5; const p6 = 6; const p7 = 7; const p8 = 8; const p9 = 9;
    return value;
}
"#,
    );
}

/// Same shape but with an `if (flag) { ... }` block in the middle of the run.
/// Its `BRANCH_LABEL` merge re-schedules the interior run nodes, so the chase
/// re-scans the run on more than one worklist pop — the exact repetition the
/// relevance memo collapses. Narrowing must still be byte-identical (no
/// TS2322).
#[test]
fn identifier_narrowing_survives_run_with_branch_merge() {
    assert_narrowing_preserved(
        r#"
function widenSecond(token: string | number, flag: boolean): string {
    if (typeof token !== "string") return "";
    const b0 = 0; const b1 = 1; const b2 = 2; const b3 = 3; const b4 = 4;
    if (flag) {
        const c0 = 0; const c1 = 1;
    }
    const d0 = 0; const d1 = 1; const d2 = 2;
    return token;
}
"#,
    );
}

/// Member-access reference (`holder.payload`) carries no `symbol_id`, so the
/// relevance classifier takes its `else` arm. The `const`s do not affect the
/// `holder.payload` path, so it stays narrowed to `string`.
#[test]
fn member_narrowing_survives_long_const_run() {
    assert_narrowing_preserved(
        r#"
function readMember(holder: { payload: string | number }): string {
    if (typeof holder.payload !== "string") return "";
    const e0 = 0; const e1 = 1; const e2 = 2; const e3 = 3; const e4 = 4;
    return holder.payload;
}
"#,
    );
}

// =========================================================================
// A RELEVANT assignment in the run still stops the chase and clears narrowing.
// =========================================================================

/// A killing definition (`subject = 42`) in the middle of the run targets the
/// reference, so the chase must NOT splice past it: `subject` is re-narrowed to
/// `number` and the trailing `return subject` is no longer assignable to the
/// `string` result, emitting TS2322. If the relevance memo wrongly classified
/// the targeting assignment as irrelevant, the chase would skip it and the
/// error would vanish.
#[test]
fn identifier_targeting_assignment_in_run_clears_narrowing() {
    assert_narrowing_cleared(
        r#"
function reassignFirst(subject: string | number): string {
    if (typeof subject === "string") {
        const g0 = 0; const g1 = 1; const g2 = 2; const g3 = 3; const g4 = 4;
        subject = 42;
        const h0 = 0; const h1 = 1;
        return subject;
    }
    return "";
}
"#,
    );
}

/// A property reassignment (`record.field = count`) affects the member
/// reference, so the chase stops at it and `record.field` is re-narrowed to
/// `number`, producing TS2322 on the trailing return. Exercises the
/// member-reference `affects` arm of the relevance classifier.
#[test]
fn member_affecting_assignment_in_run_clears_narrowing() {
    assert_narrowing_cleared(
        r#"
function reassignMember(record: { field: string | number }, count: number): string {
    if (typeof record.field === "string") {
        const j0 = 0; const j1 = 1; const j2 = 2;
        record.field = count;
        return record.field;
    }
    return "";
}
"#,
    );
}
