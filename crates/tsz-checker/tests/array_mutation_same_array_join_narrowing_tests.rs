//! Witness matrix for the same-array `ARRAY_MUTATION`-then-join narrowing fix in
//! the backward flow walk (`condition_antecedent_requires_defer` /
//! `array_mutation_chain_requires_defer` in
//! `crates/tsz-checker/src/flow/control_flow/core/flow_traversal.rs`).
//!
//! Structural rule:
//!
//! > An `ARRAY_MUTATION` flow node (`.push` / `.unshift` / `.pop`) on array `A`
//! > that itself carries `A`'s assignment-narrowing (`A = A || []`, `A ??= []`,
//! > `if (A === undefined) A = []`) must keep that narrowing alive across a
//! > following control-flow join. A CONDITION / branch-label antecedent that is
//! > such a mutation — directly, or behind a run of interleaved sibling-array
//! > mutations whose chain still reaches `A`'s narrowing assignment — must defer
//! > to it rather than re-deriving the declared `T | undefined`. Previously the
//! > CONDITION classifier did not recognize an `ARRAY_MUTATION` antecedent, so a
//! > post-join read of `A` re-widened to `T | undefined` and emitted a false
//! > `TS18048`.
//!
//! This is the same-array counterpart of
//! `array_mutation_unrelated_narrowing_tests` (the unrelated-array case); together
//! they clear the mobx `eq.ts` witness (`aStack`/`bStack` `.pop()` after the
//! traversal loop). The deferral is confined to the branch/join classifier — not
//! the linear-passthrough chase — so straight-line loop entries are untouched.
//!
//! `tsc` keeps `A` narrowed across the mutation+join; every case here is
//! `tsc`-clean except the explicitly-cleared reassignment case. Each case uses
//! distinct binder / parameter names so the behavior follows the structural
//! shape, not any identifier spelling (CLAUDE.md anti-hardcoding gate).

use tsz_checker::test_utils::check_source_strict_codes;

const TS18048_POSSIBLY_UNDEFINED: u32 = 18048;

fn assert_no_possibly_undefined(source: &str) {
    let diags = check_source_strict_codes(source);
    assert!(
        !diags.contains(&TS18048_POSSIBLY_UNDEFINED),
        "same-array mutation narrowing must survive the control-flow join \
         (unexpected TS18048); got: {diags:?}",
    );
}

fn assert_possibly_undefined(source: &str) {
    let diags = check_source_strict_codes(source);
    assert!(
        diags.contains(&TS18048_POSSIBLY_UNDEFINED),
        "a genuinely possibly-undefined read must still report TS18048; got: {diags:?}",
    );
}

// =========================================================================
// Narrowing of A is PRESERVED across an ARRAY_MUTATION followed by a join.
// =========================================================================

/// `|| []` narrowing, `.push`, then an empty-if join, then `.pop()`. The minimal
/// repro of the bug; `tsc` is clean.
#[test]
fn or_default_push_survives_empty_if_join() {
    assert_no_possibly_undefined(
        r#"
function deepEq(value: unknown, trail: unknown[] | undefined) {
    trail = trail || [];
    trail.push(value);
    if (value) {} else {}
    trail.pop();
}
"#,
    );
}

/// `??=` narrowing form across the same join.
#[test]
fn nullish_assign_push_survives_if_join() {
    assert_no_possibly_undefined(
        r#"
function walk(node: unknown, seen?: unknown[]) {
    seen ??= [];
    seen.push(node);
    if (node) {}
    seen.pop();
}
"#,
    );
}

/// `if (x === undefined) x = []` narrowing form across the same join.
#[test]
fn if_undefined_push_survives_if_else_join() {
    assert_no_possibly_undefined(
        r#"
function visit(item: unknown, queue?: unknown[]) {
    if (queue === undefined) queue = [];
    queue.push(item);
    if (item) {} else {}
    queue.pop();
}
"#,
    );
}

/// `switch` statement join after the mutation.
#[test]
fn push_survives_switch_join() {
    assert_no_possibly_undefined(
        r#"
function classify(tag: number, bucket?: number[]) {
    bucket = bucket || [];
    bucket.push(tag);
    switch (tag) { case 1: break; default: break; }
    bucket.pop();
}
"#,
    );
}

/// `.unshift` mutator across the join.
#[test]
fn unshift_survives_if_join() {
    assert_no_possibly_undefined(
        r#"
function prepend(head: unknown, list?: unknown[]) {
    list = list || [];
    list.unshift(head);
    if (head) {}
    list.pop();
}
"#,
    );
}

/// A local (non-parameter) `let` with the same narrow + mutate + join shape.
#[test]
fn local_let_push_survives_if_join() {
    assert_no_possibly_undefined(
        r#"
function run(flag: boolean) {
    let frames: number[] | undefined = flag ? [1] : undefined;
    frames = frames || [];
    frames.push(1);
    if (flag) {} else {}
    frames.pop();
}
"#,
    );
}

// =========================================================================
// The interleaved-sibling-array shape (the mobx `eq.ts` witness): two arrays
// each narrowed and pushed, then a loop, then both popped. The pass-through
// mutation of the *other* array must not hide each array's own narrowing across
// the join.
// =========================================================================

/// Two arrays narrowed + pushed (interleaved), a traversal loop, then both
/// popped. Both `.pop()` reads must stay narrowed. (`eq.ts` shape; `tsc` clean.)
#[test]
fn interleaved_pushes_then_loop_keep_both_narrowed() {
    assert_no_possibly_undefined(
        r#"
declare function consume(x: unknown): void;
function structuralEq(value: unknown, left?: unknown[], right?: unknown[]) {
    left = left || [];
    right = right || [];
    left.push(value);
    right.push(value);
    let depth = 3;
    while (depth--) {
        consume(left);
        consume(right);
    }
    left.pop();
    right.pop();
    return true;
}
"#,
    );
}

/// The full `eq.ts`-shaped function: a pre-push search loop, interleaved pushes,
/// a two-armed `areArrays` branch whose arms each loop and recurse passing both
/// arrays, then both pops. Exercises the branch+loop join the point fix targets.
#[test]
fn eq_shaped_branch_and_loops_keep_both_narrowed() {
    assert_no_possibly_undefined(
        r#"
declare function hasKey(obj: unknown, key: unknown): boolean;
function structuralCompare(
    a: any,
    b: any,
    depth: number,
    firstStack?: unknown[],
    secondStack?: unknown[],
): boolean {
    const bothArrays = Array.isArray(a) && Array.isArray(b);
    firstStack = firstStack || [];
    secondStack = secondStack || [];
    let length = firstStack.length;
    while (length--) {
        if (firstStack[length] === a) {
            return secondStack[length] === b;
        }
    }
    firstStack.push(a);
    secondStack.push(b);
    if (bothArrays) {
        length = a.length;
        if (length !== b.length) {
            return false;
        }
        while (length--) {
            if (!structuralCompare(a[length], b[length], depth - 1, firstStack, secondStack)) {
                return false;
            }
        }
    } else {
        const keys = Object.keys(a);
        const total = keys.length;
        for (let i = 0; i < total; i++) {
            const key = keys[i];
            if (!(hasKey(b, key) && structuralCompare(a[key], b[key], depth - 1, firstStack, secondStack))) {
                return false;
            }
        }
    }
    firstStack.pop();
    secondStack.pop();
    return true;
}
"#,
    );
}

// =========================================================================
// Regression guards: straight-line loop entries must stay narrowed, and the
// deferral must not be confused for an evolving-array shape.
// =========================================================================

/// A single narrowed array pushed, then a bare loop (no branch), then popped —
/// this never regressed on `tsc` and must stay clean (the loop-entry resolution
/// the branch-confined deferral deliberately leaves alone).
#[test]
fn push_then_bare_loop_stays_narrowed() {
    assert_no_possibly_undefined(
        r#"
declare function touch(x: unknown): void;
function loopOnly(value: unknown, store?: unknown[]) {
    store = store || [];
    store.push(value);
    let n = 3;
    while (n--) {
        touch(store);
    }
    store.pop();
}
"#,
    );
}

/// An evolving array (`let a = []`) pushed across a join must keep evolving and
/// must not gain a spurious `undefined`: a plain read after the join is clean.
#[test]
fn evolving_array_push_across_join_stays_clean() {
    assert_no_possibly_undefined(
        r#"
function evolve(flag: boolean) {
    let acc = [];
    acc.push(1);
    if (flag) {} else {}
    acc.push(2);
    const head: number = acc[0];
    return head;
}
"#,
    );
}

// =========================================================================
// NEGATIVE: a genuine reassignment after the guard re-introduces `undefined`,
// so the post-join read must still report TS18048 (parity with `tsc`).
// =========================================================================

/// After narrowing and pushing, the array is reassigned to a possibly-undefined
/// value; the join read is then genuinely possibly-undefined. (Element access is
/// used for the final read rather than `.pop()` so the read surfaces TS18048
/// independently of the minimal test-harness lib's `Array.prototype` member
/// resolution; the CLI reports TS18048 on the `.pop()` form too.)
#[test]
fn reassignment_after_guard_still_reports_possibly_undefined() {
    assert_possibly_undefined(
        r#"
function clobber(value: number, primary?: number[], fallback?: number[]) {
    primary = primary || [];
    primary.push(value);
    primary = fallback;
    if (value) {}
    const last = primary[0];
    return last;
}
"#,
    );
}
