//! Witness matrix for the `ARRAY_MUTATION` pass-through narrowing fix in the
//! backward flow walk (`check_flow`'s `ARRAY_MUTATION` arm in
//! `crates/tsz-checker/src/flow/control_flow/core/flow_traversal.rs`).
//!
//! Structural rule:
//!
//! > An `ARRAY_MUTATION` flow node for a mutating method call (`.push` / `.pop`
//! > / `.unshift`) on array `B` must only affect the evolving-array narrowing of
//! > `B` itself. For an unrelated reference `A` the node is a pure value
//! > pass-through, so the walk must defer to the antecedent that carries `A`'s
//! > narrowing instead of re-deriving (and re-widening) it. Previously the arm's
//! > `!affects_ref` branch used a defer predicate that did not recognize a prior
//! > `ARRAY_MUTATION` node (the mutation/evolution of `A` itself) nor a
//! > non-targeting ASSIGNMENT chain still carrying `A`'s assignment-narrowing, so
//! > the unrelated mutation re-widened `A` back to `T | undefined` and emitted a
//! > false `TS18048` on a later read of `A`.
//!
//! `tsc` keeps `A` narrowed across an unrelated array mutation; all cases here
//! are `tsc`-clean except the explicitly-cleared one. Each case uses distinct
//! binder / parameter names so the behavior follows the structural shape, not
//! any identifier spelling (CLAUDE.md anti-hardcoding gate).

use tsz_checker::test_utils::check_source_codes;

const TS18048_POSSIBLY_UNDEFINED: u32 = 18048;

fn assert_no_possibly_undefined(source: &str) {
    let diags = check_source_codes(source);
    assert!(
        !diags.contains(&TS18048_POSSIBLY_UNDEFINED),
        "unrelated array mutation must not re-widen the narrowed reference \
         (unexpected TS18048); got: {diags:?}",
    );
}

fn assert_possibly_undefined(source: &str) {
    let diags = check_source_codes(source);
    assert!(
        diags.contains(&TS18048_POSSIBLY_UNDEFINED),
        "a genuinely possibly-undefined read must still report TS18048; got: {diags:?}",
    );
}

// =========================================================================
// Narrowing of A is PRESERVED across an unrelated array mutation of B.
// =========================================================================

/// `first` is narrowed by `= first || []`, referenced, then a DIFFERENT array
/// `second` is mutated with `.push`. The later `first.pop()` must read the
/// narrowed `number[]`, not the declared `number[] | undefined`. (The minimal
/// repro of the bug; `tsc` is clean.)
#[test]
fn or_default_narrowing_survives_unrelated_push() {
    assert_no_possibly_undefined(
        r#"
function withOrDefault(first: number[] | undefined, second: number[]) {
    first = first || [];
    first.push(1);
    second.push(1);
    first.pop();
}
"#,
    );
}

/// Same shape narrowed via an `if (x === undefined) x = []` guard instead of
/// `|| []`. Both produce a targeting ASSIGNMENT that the unrelated mutation
/// must defer through.
#[test]
fn if_undefined_narrowing_survives_unrelated_push() {
    assert_no_possibly_undefined(
        r#"
function withIfGuard(left: string[] | undefined, right: string[]) {
    if (left === undefined) left = [];
    left.push("a");
    right.push("b");
    left.pop();
}
"#,
    );
}

/// The unrelated mutation uses `.unshift` rather than `.push`; still a pure
/// pass-through for the narrowed array.
#[test]
fn narrowing_survives_unrelated_unshift() {
    assert_no_possibly_undefined(
        r#"
function withUnshift(primary: number[] | undefined, secondary: number[]) {
    primary = primary || [];
    primary.push(1);
    secondary.unshift(1);
    primary.pop();
}
"#,
    );
}

/// The unrelated mutation uses `.pop`; the narrowed array must stay narrowed.
#[test]
fn narrowing_survives_unrelated_pop() {
    assert_no_possibly_undefined(
        r#"
function withPop(alpha: number[] | undefined, beta: number[]) {
    alpha = alpha || [];
    alpha.push(1);
    beta.pop();
    alpha.pop();
}
"#,
    );
}

/// An element write (`other[0] = 1`) interleaved between the narrowed array's
/// own mutation and a later read must not drop the narrowing either. This
/// exercises the non-targeting ASSIGNMENT-after-ARRAY_MUTATION chain.
#[test]
fn narrowing_survives_unrelated_element_write() {
    assert_no_possibly_undefined(
        r#"
function withElementWrite(head: number[] | undefined, other: number[]) {
    head = head || [];
    head.push(1);
    other[0] = 1;
    head.pop();
}
"#,
    );
}

/// Two independently-narrowed arrays whose mutations interleave: each read must
/// see its own narrowed type, unaffected by the other's mutation.
#[test]
fn two_narrowed_arrays_interleaved_mutations() {
    assert_no_possibly_undefined(
        r#"
function withTwo(one: number[] | undefined, two: string[] | undefined) {
    one = one || [];
    two = two || [];
    one.push(1);
    two.push("x");
    one.pop();
    two.pop();
}
"#,
    );
}

// =========================================================================
// A genuinely-undefined read still reports TS18048 (no over-suppression).
// =========================================================================

/// No narrowing of `maybe` ever happens; the unrelated mutation must not
/// fabricate a non-null narrowing. The `maybe[0]` read is still a true
/// possibly-undefined access and must report TS18048. (Element access is used
/// rather than a `.pop()` call so the read surfaces TS18048 independently of
/// the minimal test-harness lib's `Array.prototype` member resolution; the CLI
/// reports TS18048 on the `.pop()` form too.)
#[test]
fn unnarrowed_reference_still_reports_possibly_undefined() {
    assert_possibly_undefined(
        r#"
function withoutGuard(maybe: number[] | undefined, present: number[]) {
    present.push(1);
    const value = maybe[0];
    return value;
}
"#,
    );
}
