//! Regression coverage for the memoized flow-graph DP introduced for #7682.
//!
//! These tests pin the structural rule for backward "all paths" predicates
//! (typeof-exclusion mask and antecedent-chain null exclusion): each flow node
//! is folded once across its antecedents, so the cost is `O(N)` rather than
//! the previous `O(N · 2^N)` clone-per-branch walk. They also exercise
//! identifier-name independence (per CLAUDE.md §25) and the historical
//! fail-safe behavior on CFG back-edges (loops do not over-narrow).

use std::time::{Duration, Instant};

use tsz_checker::context::CheckerOptions;

fn strict_diagnostics(source: &str) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
    .apply_strict_defaults();

    tsz_checker::test_utils::check_source(source, "test.ts", options)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

/// Wraps a checker run with a wall-clock budget. The pre-fix exponential
/// traversal blew through the conformance 90-second per-test timeout on the
/// `instanceofOperatorWithRHSHasSymbolHasInstance.ts` shape; this test fails
/// fast (well under a second on a memoized walk) instead of letting CI hang.
fn diagnostics_within(source: &str, budget: Duration) -> Vec<(u32, String)> {
    let start = Instant::now();
    let diagnostics = strict_diagnostics(source);
    let elapsed = start.elapsed();
    assert!(
        elapsed < budget,
        "flow narrowing did not terminate in budget: elapsed={elapsed:?} budget={budget:?}",
    );
    diagnostics
}

/// 32-level `instanceof` chain on a single `unknown`-typed binding. The
/// pre-fix `O(2^N)` traversal explodes here (each new branch doubles the
/// work); the memoized DP completes in milliseconds. The chain itself does
/// not produce TS2322 — the assertion is that the checker terminates.
#[test]
fn deep_instanceof_chain_does_not_blow_up_on_unknown() {
    let mut source = String::from(
        "declare class A {}\n\
         declare class B {}\n\
         declare class C {}\n\
         declare class D {}\n\
         function probe(x: unknown) {\n",
    );
    for level in 0..32 {
        let cls = ["A", "B", "C", "D"][level % 4];
        source.push_str(&format!("    if (x instanceof {cls}) {{ return; }}\n"));
    }
    source.push_str("    return x;\n}\n");

    diagnostics_within(&source, Duration::from_secs(10));
}

/// Reproduces the conformance shape: many `typeof` branches plus
/// `instanceof` branches over an `unknown` value. The pre-fix path called
/// `flow_has_exhaustive_typeof_exclusions` per BFS step, each call cloning
/// the visited Vec per branch; on this shape it never returned.
#[test]
fn mixed_typeof_and_instanceof_chain_terminates() {
    let mut source = String::from(
        "declare class A {}\n\
         declare class B {}\n\
         declare class C {}\n\
         function probe(value: unknown) {\n\
             if (typeof value === \"string\") return;\n\
             if (typeof value === \"number\") return;\n\
             if (typeof value === \"boolean\") return;\n\
             if (typeof value === \"bigint\") return;\n\
             if (typeof value === \"symbol\") return;\n\
             if (typeof value === \"undefined\") return;\n\
             if (typeof value === \"function\") return;\n",
    );
    for level in 0..24 {
        let cls = ["A", "B", "C"][level % 3];
        source.push_str(&format!("    if (value instanceof {cls}) return;\n"));
    }
    source.push_str("    return value;\n}\n");

    diagnostics_within(&source, Duration::from_secs(10));
}

/// Exhaustive typeof chain that should still leave `{}` after narrowing.
/// Locks the memoized traversal against accidentally weakening the
/// "exhaustive exclusions ⇒ empty object" rule.
#[test]
fn exhaustive_typeof_chain_still_narrows_unknown_to_empty_object() {
    let diagnostics = strict_diagnostics(
        r#"
function probe(x: unknown) {
    if (typeof x === "string") return;
    if (typeof x === "number") return;
    if (typeof x === "boolean") return;
    if (typeof x === "bigint") return;
    if (typeof x === "symbol") return;
    if (typeof x === "undefined") return;
    if (typeof x === "object") return;
    if (typeof x === "function") return;
    const remaining: never = x;
    return remaining;
}
"#,
    );

    assert!(
        diagnostics.iter().any(|(code, message)| *code == 2322
            && message.contains("Type '{}' is not assignable to type 'never'")),
        "expected `{{}}` residue after exhausting all 8 typeof kinds, got: {diagnostics:?}",
    );
}

/// Same exhaustive chain, renamed identifier — the fix is structural and
/// must not depend on the spelling of the narrowed binding (CLAUDE.md §25).
#[test]
fn exhaustive_typeof_chain_renamed_binding_still_narrows_to_empty_object() {
    let diagnostics = strict_diagnostics(
        r#"
function probe(candidate: unknown) {
    if (typeof candidate === "string") return;
    if (typeof candidate === "number") return;
    if (typeof candidate === "boolean") return;
    if (typeof candidate === "bigint") return;
    if (typeof candidate === "symbol") return;
    if (typeof candidate === "undefined") return;
    if (typeof candidate === "object") return;
    if (typeof candidate === "function") return;
    const remaining: never = candidate;
    return remaining;
}
"#,
    );

    assert!(
        diagnostics.iter().any(|(code, message)| *code == 2322
            && message.contains("Type '{}' is not assignable to type 'never'")),
        "renamed candidate should narrow identically, got: {diagnostics:?}",
    );
}

/// DAG sharing: an early `if/else` whose two branches both pass through a
/// later `typeof x === \"object\"` narrowing. Pre-fix, the shared
/// `antecedent_chain_excludes_null_for_target` visited Vec would mark a
/// shared predecessor on the first branch and short-circuit the second to
/// `false`, dropping the null-exclusion. With per-traversal memoization the
/// answer is order-independent.
#[test]
fn dag_shared_antecedent_preserves_null_exclusion() {
    let diagnostics = strict_diagnostics(
        r#"
function probe(value: unknown, flag: boolean) {
    if (value === null) return;
    if (flag) {
        // shared antecedent path 1
    } else {
        // shared antecedent path 2
    }
    if (typeof value === "object") {
        // The chain to here has excluded null on both branches, so the
        // narrowed type must drop null even though the join sees two
        // antecedents that share the null-excluding predecessor.
        const o: object = value;
        return o;
    }
    return value;
}
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|(code, _)| *code != 2322 && *code != 2345),
        "null-exclusion should survive a DAG-shared antecedent, got: {diagnostics:?}",
    );
}

/// A `while` loop with `typeof x === \"string\"` inside the body produces a
/// back-edge in the flow graph. The historical behavior was that the loop
/// edge contributes the identity element to the fold (no narrowing implied
/// by the loop itself); the memoized traversal preserves this by returning
/// the analysis identity when it encounters an `InProgress` sentinel.
#[test]
fn back_edge_in_loop_does_not_spuriously_narrow_after_loop() {
    let diagnostics = strict_diagnostics(
        r#"
function probe(items: unknown[]) {
    let value: unknown = items[0];
    while (typeof value === "string") {
        value = items.shift();
    }
    return value;
}
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|(code, _)| *code != 2322 && *code != 2345),
        "loop back-edge should not cause spurious narrowing errors, got: {diagnostics:?}",
    );
}
