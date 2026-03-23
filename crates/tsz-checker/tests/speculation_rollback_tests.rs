//! Tests that verify speculative typing does not leak committed checker state.
//!
//! These tests exercise the speculation/transaction API by checking that:
//! - Overload resolution does not duplicate diagnostics
//! - Failed speculative paths do not leave stale dedup entries
//! - Selective-keep behavior preserves intended diagnostics

use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn check(source: &str) -> Vec<Diagnostic> {
    check_with(source, "test.ts", CheckerOptions::default())
}

fn check_with(source: &str, file_name: &str, options: CheckerOptions) -> Vec<Diagnostic> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let source_file = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), source_file);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );
    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(source_file);
    checker.ctx.diagnostics.clone()
}

/// Overload resolution should not duplicate diagnostics when the first overload
/// fails and the second succeeds. Speculative diagnostics must roll back.
#[test]
fn overload_resolution_no_duplicate_diagnostics() {
    let diags = check(
        r#"
        declare function f(x: string): string;
        declare function f(x: number): number;
        let result = f(42);
    "#,
    );
    assert!(diags.is_empty(), "Expected no errors, got: {diags:?}");
}

/// When all overloads fail, TS2769 should be emitted exactly once.
#[test]
fn overload_resolution_single_ts2769() {
    let diags = check(
        r#"
        declare function f(x: string): string;
        declare function f(x: number): number;
        f(true);
    "#,
    );
    let ts2769_count = diags.iter().filter(|d| d.code == 2769).count();
    assert_eq!(
        ts2769_count, 1,
        "Expected exactly one TS2769, got {ts2769_count}: {diags:?}"
    );
}

/// Speculative return-type inference should not pollute the diagnostic dedup set.
#[test]
fn return_type_inference_no_dedup_pollution() {
    let diags = check(
        r#"
        function f() {
            return { a: 1 };
        }
        let x: { a: string } = f();
    "#,
    );
    let has_ts2322 = diags.iter().any(|d| d.code == 2322);
    assert!(
        has_ts2322,
        "Expected TS2322 for type mismatch, got: {diags:?}"
    );
}

/// Dead branches in conditional expressions should not emit diagnostics.
#[test]
fn conditional_dead_branch_no_diagnostics() {
    let diags = check(
        r#"
        let x: string = false ? (1 as any as never) : "hello";
    "#,
    );
    assert!(
        diags.is_empty(),
        "Expected no errors for dead branch, got: {diags:?}"
    );
}

/// Speculative evaluation for elaboration should not leave diagnostics behind.
#[test]
fn elaboration_probe_no_leak() {
    let diags = check(
        r#"
        declare function f(x: { a: number }): void;
        f({ a: "hello" });
    "#,
    );
    // Should get assignability error(s) but not duplicated by elaboration
    let assignability_count = diags
        .iter()
        .filter(|d| d.code == 2345 || d.code == 2322)
        .count();
    assert!(
        assignability_count >= 1,
        "Expected at least one assignability error, got: {diags:?}"
    );
    assert!(
        assignability_count <= 2,
        "Too many assignability errors — possible dedup leak: {diags:?}"
    );
}

/// TS7006 should not be emitted when contextual type exists from union overloads.
#[test]
fn implicit_any_no_false_positive_with_contextual_type() {
    let diags = check_with(
        r#"
        declare function f(cb: (x: string) => void): void;
        declare function f(cb: (x: number) => void): void;
        f((x) => { });
    "#,
        "test.ts",
        CheckerOptions {
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    );
    let ts7006_count = diags.iter().filter(|d| d.code == 7006).count();
    assert_eq!(
        ts7006_count, 0,
        "TS7006 should not be emitted when contextual type exists: {diags:?}"
    );
}

/// Variable declaration with conditional initializer should not duplicate TS2322.
#[test]
fn variable_conditional_init_no_duplicate_ts2322() {
    let diags = check(
        r#"
        declare let cond: boolean;
        let x: string = cond ? 42 : "hello";
    "#,
    );
    let ts2322_count = diags.iter().filter(|d| d.code == 2322).count();
    assert!(
        ts2322_count <= 1,
        "Expected at most one TS2322 for conditional init, got {ts2322_count}: {diags:?}"
    );
}

/// Speculative object literal property inference should not leak diagnostics.
#[test]
fn object_literal_inference_no_diagnostic_leak() {
    let diags = check(
        r#"
        declare function f<T>(obj: { produce: (n: number) => T, consume: (x: T) => void }): void;
        f({ produce: (n) => n + 1, consume: (x) => x.toFixed() });
    "#,
    );
    // This should work — produce returns number, consume expects number
    assert!(
        diags.is_empty(),
        "Expected no errors for valid generic object literal, got: {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Overload probing and successful-candidate rollback tests
// ---------------------------------------------------------------------------
// These tests validate that the speculation infrastructure correctly manages
// diagnostic state across overload resolution phases. They cover:
// - Multi-candidate probing with rollback between candidates
// - Successful candidate committing only its own diagnostics
// - Callback body diagnostics from failed candidates not leaking

/// When the first overload fails on argument type but the second matches,
/// speculative diagnostics from the first candidate must not leak.
#[test]
fn overload_probe_first_fails_second_succeeds_no_leak() {
    let diags = check(
        r#"
        declare function overloaded(x: string, y: string): string;
        declare function overloaded(x: number, y: number): number;
        let r = overloaded(1, 2);
    "#,
    );
    assert!(
        diags.is_empty(),
        "Expected no errors when second overload matches, got: {diags:?}"
    );
}

/// Overload resolution with callback arguments: speculative callback body
/// diagnostics from a failed candidate should not survive into the successful path.
#[test]
fn overload_probe_callback_body_diagnostics_rollback() {
    let diags = check(
        r#"
        declare function process(cb: (x: string) => string): string;
        declare function process(cb: (x: number) => number): number;
        let r = process((x) => x + 1);
    "#,
    );
    // The second overload (number → number) should match.
    // No TS2365 or TS2322 from the first candidate's speculative callback body check.
    let leaked_errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2365 || d.code == 2322)
        .collect();
    assert!(
        leaked_errors.is_empty(),
        "Speculative callback body errors leaked from failed overload candidate: {leaked_errors:?}"
    );
}

/// When all overloads fail, the fallback diagnostic (TS2769) should be clean
/// and not contain duplicated speculative diagnostics from multiple candidates.
#[test]
fn overload_all_fail_no_duplicate_speculative_diagnostics() {
    let diags = check(
        r#"
        declare function multi(x: string): void;
        declare function multi(x: number): void;
        declare function multi(x: boolean): void;
        multi({} as never);
    "#,
    );
    let ts2769_count = diags.iter().filter(|d| d.code == 2769).count();
    assert!(
        ts2769_count <= 1,
        "Expected at most one TS2769 for total overload failure, got {ts2769_count}: {diags:?}"
    );
}

/// Overload resolution with generic candidate and contextual refresh:
/// The successful candidate's argument re-typing should produce clean diagnostics.
#[test]
fn overload_generic_candidate_contextual_refresh_clean() {
    let diags = check(
        r#"
        declare function convert<T>(x: T, cb: (v: T) => string): string;
        declare function convert(x: string, cb: (v: string) => number): number;
        let r = convert(42, (v) => v.toFixed());
    "#,
    );
    // First overload should match: T=number, cb gets (v: number) => string
    assert!(
        diags.is_empty(),
        "Expected no errors for generic overload with contextual refresh, got: {diags:?}"
    );
}

/// Ensure that `TypeParameterConstraintViolation` during overload resolution
/// correctly rolls back and tries the next candidate.
#[test]
fn overload_constraint_violation_tries_next_candidate() {
    let diags = check(
        r#"
        declare function constrained<T extends string>(x: T): T;
        declare function constrained(x: number): number;
        let r = constrained(42);
    "#,
    );
    assert!(
        diags.is_empty(),
        "Expected no errors when constraint-violated overload falls through to next, got: {diags:?}"
    );
}

/// Speculative diagnostics from argument type collection with unresolved
/// contextual types should be properly rolled back, not left as duplicates.
#[test]
fn unresolved_contextual_arg_implicit_any_rollback() {
    let diags = check_with(
        r#"
        declare function withCb<T>(produce: () => T, consume: (x: T) => void): void;
        withCb(() => 42, (x) => x.toFixed());
    "#,
        "test.ts",
        CheckerOptions {
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    );
    // TS7006 should not be emitted for `x` since it gets contextual type number
    let ts7006_in_consume: Vec<_> = diags.iter().filter(|d| d.code == 7006).collect();
    assert!(
        ts7006_in_consume.is_empty(),
        "TS7006 should not appear when contextual type resolves via generic inference: {ts7006_in_consume:?}"
    );
}

// ---------------------------------------------------------------------------
// Nested speculation edge cases
// ---------------------------------------------------------------------------
// These tests exercise scenarios where nested or cross-path speculation
// can cause the diagnostics list to shrink below a snapshot's recorded
// length — the clamping logic must prevent panics.

/// Nested overload resolution inside a conditional expression: inner
/// speculative rollback must not panic even if outer speculation has
/// already modified the diagnostic vector.
#[test]
fn nested_overload_in_conditional_no_panic() {
    let diags = check(
        r#"
        declare function inner(x: string): string;
        declare function inner(x: number): number;
        declare let cond: boolean;
        let result: string = cond ? inner(42) : inner("hello");
    "#,
    );
    // We care that it doesn't panic; the exact diagnostics depend on
    // type narrowing but must be bounded.
    assert!(
        diags.len() <= 3,
        "Expected bounded diagnostics for nested overload in conditional, got {}: {diags:?}",
        diags.len()
    );
}

/// Deeply nested overload inside switch-like narrowing: exercises
/// multiple levels of snapshot/rollback.
#[test]
fn nested_overload_in_narrowing_chain_no_panic() {
    let diags = check(
        r#"
        declare function f(x: string): string;
        declare function f(x: number): number;
        function test(x: string | number) {
            if (typeof x === "string") {
                return f(x);
            } else {
                return f(x);
            }
        }
    "#,
    );
    // Both branches resolve cleanly; no leaked diagnostics expected.
    let leaked = diags
        .iter()
        .filter(|d| d.code == 2769 || d.code == 2345)
        .count();
    assert_eq!(
        leaked, 0,
        "No overload errors should leak from narrowed branches: {diags:?}"
    );
}

/// Overload resolution with nested conditional expression as argument:
/// speculative argument typing interacts with conditional branch rollback.
#[test]
fn overload_with_conditional_argument_no_panic() {
    let diags = check(
        r#"
        declare function g(x: string): void;
        declare function g(x: number): void;
        declare let b: boolean;
        g(b ? "hello" : 42);
    "#,
    );
    // Should succeed without panic; either overload or TS2769 is acceptable.
    assert!(
        diags.len() <= 2,
        "Expected bounded diagnostics, got {}: {diags:?}",
        diags.len()
    );
}

/// Multiple overloaded calls in sequence: ensure rollback from one call
/// doesn't corrupt the snapshot state for the next call.
#[test]
fn sequential_overload_calls_independent_rollback() {
    let diags = check(
        r#"
        declare function h(x: string): string;
        declare function h(x: number): number;
        let a = h(1);
        let b = h("hello");
        let c = h(true);
    "#,
    );
    // First two calls succeed, third fails with TS2769.
    let ts2769_count = diags.iter().filter(|d| d.code == 2769).count();
    assert!(
        ts2769_count <= 1,
        "Expected at most one TS2769 from the failing call, got {ts2769_count}: {diags:?}"
    );
}

/// Nested speculative typing with callback and conditional:
/// exercises the deepest nesting pattern (overload → callback body → conditional).
#[test]
fn deeply_nested_speculation_callback_conditional_no_panic() {
    let diags = check(
        r#"
        declare function apply<T>(cb: (x: T) => T): T;
        declare function apply(cb: (x: string) => string): string;
        let result = apply((x) => typeof x === "string" ? x.toUpperCase() : x);
    "#,
    );
    // Should not panic. Exact diagnostics depend on inference but must be bounded.
    assert!(
        diags.len() <= 4,
        "Expected bounded diagnostics for deeply nested speculation, got {}: {diags:?}",
        diags.len()
    );
}

// ---------------------------------------------------------------------------
// Additional edge cases for nested/cross-path speculation robustness
// ---------------------------------------------------------------------------

/// Three-level nesting: overload resolution with a callback argument that
/// contains an overloaded call inside a conditional. This exercises snapshot
/// clamping at multiple nesting depths.
#[test]
fn three_level_nested_overload_callback_conditional_overload() {
    let diags = check(
        r#"
        declare function outer(cb: (x: string) => string): string;
        declare function outer(cb: (x: number) => number): number;
        declare function inner(x: string): string;
        declare function inner(x: number): number;
        let result = outer((x) => typeof x === "string" ? inner(x) : inner(x));
    "#,
    );
    // Must not panic from nested snapshot/rollback interactions.
    assert!(
        diags.len() <= 5,
        "Expected bounded diagnostics for three-level nesting, got {}: {diags:?}",
        diags.len()
    );
}

/// Overload resolution where the successful candidate's callback body
/// contains another overloaded call that fails — the inner failure
/// diagnostics should not leak after the outer overload succeeds.
#[test]
fn overload_success_with_inner_overload_failure_no_leak() {
    let diags = check(
        r#"
        declare function process(cb: (x: number) => void): void;
        declare function process(cb: (x: string) => void): void;
        declare function format(x: string): string;
        declare function format(x: boolean): boolean;
        process((x) => { format(x); });
    "#,
    );
    // The outer overload should match (number or string).
    // Inner `format(x)` may fail if the contextual type doesn't match format's
    // overloads, but the speculative diagnostics from failed inner overloads
    // must not duplicate.
    let ts2769_count = diags.iter().filter(|d| d.code == 2769).count();
    assert!(
        ts2769_count <= 1,
        "Inner overload failure should not produce duplicate TS2769: {diags:?}"
    );
}

/// Multiple overloaded calls on the same line with different resolution
/// outcomes: ensures per-call snapshot isolation.
#[test]
fn multiple_overloaded_calls_same_statement_isolation() {
    let diags = check(
        r#"
        declare function f(x: string): string;
        declare function f(x: number): number;
        let a = f(1) + f("hello") + f(true);
    "#,
    );
    // f(1) and f("hello") should succeed, f(true) should produce TS2769.
    // No diagnostic cross-contamination between calls.
    let ts2769_count = diags.iter().filter(|d| d.code == 2769).count();
    assert!(
        ts2769_count <= 1,
        "Expected at most one TS2769 from f(true), got {ts2769_count}: {diags:?}"
    );
}

/// Overload resolution inside a switch/if-else narrowing chain where
/// each branch tests a different overload. Exercises snapshot/rollback
/// with flow-narrowed types at each branch.
#[test]
fn overload_in_switch_narrowing_branches_no_panic() {
    let diags = check(
        r#"
        declare function handler(x: string): string;
        declare function handler(x: number): number;
        declare function handler(x: boolean): boolean;
        function dispatch(x: string | number | boolean) {
            if (typeof x === "string") {
                return handler(x);
            } else if (typeof x === "number") {
                return handler(x);
            } else {
                return handler(x);
            }
        }
    "#,
    );
    // All branches should resolve cleanly via narrowing.
    let error_count = diags
        .iter()
        .filter(|d| d.code == 2769 || d.code == 2345)
        .count();
    assert_eq!(
        error_count, 0,
        "Narrowed overload calls should resolve cleanly: {diags:?}"
    );
}

/// Overloaded generic function with multiple callback arguments:
/// the second callback's contextual type depends on the first callback's
/// return type, creating a dependency chain during speculation.
#[test]
fn overload_generic_chained_callback_inference() {
    let diags = check(
        r#"
        declare function chain<T, U>(
            first: (x: number) => T,
            second: (y: T) => U
        ): U;
        declare function chain(
            first: (x: number) => string,
            second: (y: string) => number
        ): number;
        let result = chain((x) => x + 1, (y) => y.toFixed());
    "#,
    );
    // Generic overload should match: T=number, U=string
    // No leaked diagnostics from inference probing.
    assert!(
        diags.is_empty(),
        "Expected no errors for chained generic callback inference, got: {diags:?}"
    );
}

/// Nested speculation where inner call uses `as` assertion inside overload:
/// ensures that type assertion evaluation during speculation doesn't corrupt
/// the outer rollback state.
#[test]
fn nested_speculation_with_type_assertion_no_corruption() {
    let diags = check(
        r#"
        declare function convert(x: string): number;
        declare function convert(x: number): string;
        declare let input: string | number;
        let result = convert(input as string);
    "#,
    );
    // Should not panic and should have bounded diagnostics.
    assert!(
        diags.len() <= 2,
        "Expected bounded diagnostics with type assertion, got {}: {diags:?}",
        diags.len()
    );
}

/// Variable declaration with overloaded call as initializer and explicit
/// type annotation: exercises the `variable_checking` snapshot interacting
/// with overload resolution speculation.
#[test]
fn variable_decl_overloaded_init_with_annotation() {
    let diags = check(
        r#"
        declare function parse(x: string): number;
        declare function parse(x: number): string;
        let x: number = parse("hello");
        let y: string = parse(42);
    "#,
    );
    // Both should type-check successfully.
    assert!(
        diags.is_empty(),
        "Expected no errors for correctly typed overloaded initializers, got: {diags:?}"
    );
}

/// Deeply nested conditional with overloads at leaf positions:
/// exercises maximum nesting depth for snapshot clamping.
#[test]
fn deeply_nested_conditional_overloads_at_leaves() {
    let diags = check(
        r#"
        declare function f(x: string): string;
        declare function f(x: number): number;
        declare let a: boolean;
        declare let b: boolean;
        declare let c: boolean;
        let result: string | number = a ? (b ? f("a") : f(1)) : (c ? f("b") : f(2));
    "#,
    );
    // All four leaf calls should resolve; no panic from deep nesting.
    assert!(
        diags.len() <= 2,
        "Expected bounded diagnostics for deeply nested conditionals, got {}: {diags:?}",
        diags.len()
    );
}

/// Overload with spread argument: the spread unpacking interacts with
/// speculation rollback for argument type collection.
#[test]
fn overload_with_spread_argument_no_panic() {
    let diags = check(
        r#"
        declare function f(a: string, b: number): void;
        declare function f(a: number, b: string): void;
        let args: [string, number] = ["hello", 42];
        f(...args);
    "#,
    );
    // Should resolve to first overload without panic.
    assert!(
        diags.len() <= 2,
        "Expected bounded diagnostics for spread argument, got {}: {diags:?}",
        diags.len()
    );
}

/// Regression test: ensure that after a filtered rollback that keeps some
/// diagnostics, a subsequent full rollback to the same outer snapshot
/// clamps correctly and doesn't panic.
#[test]
fn filtered_rollback_then_full_rollback_no_panic() {
    let diags = check(
        r#"
        declare function g(x: string): string;
        declare function g(x: number): number;
        declare function h(x: boolean): boolean;
        // g(true) will fail overload resolution (inner), then the result
        // is used in h() which may produce additional diagnostics (outer).
        let result = h(g(true) as any as boolean);
    "#,
    );
    // The key invariant: no panic from nested rollback interactions.
    // g(true) fails, but the `as any as boolean` cast makes h() succeed.
    assert!(
        diags.len() <= 3,
        "Expected bounded diagnostics, got {}: {diags:?}",
        diags.len()
    );
}

// ---------------------------------------------------------------------------
// Deferred TS2454 and rollback consistency tests
// ---------------------------------------------------------------------------
// These tests exercise scenarios where deferred_ts2454_errors must be
// truncated consistently across all rollback paths (rollback_diagnostics,
// rollback_diagnostics_filtered, take_speculative_diagnostics).

/// TS2454 (variable used before assigned) inside an overloaded callback:
/// the deferred TS2454 entry must not survive a filtered rollback of the
/// failed overload candidate.
#[test]
fn ts2454_in_overload_callback_filtered_rollback() {
    let diags = check(
        r#"
        declare function f(cb: (x: string) => string): string;
        declare function f(cb: (x: number) => number): number;
        f((x) => {
            let v: number;
            return x;
        });
    "#,
    );
    // The second overload should match. TS2454 for `v` should not appear
    // because `v` is declared but never used — but if it were emitted
    // speculatively by the first candidate, it must be rolled back cleanly.
    let ts2454_count = diags.iter().filter(|d| d.code == 2454).count();
    assert!(
        ts2454_count <= 1,
        "TS2454 should not be duplicated across overload candidates: {diags:?}"
    );
}

/// Nested overloads where the inner call's speculation interacts with the
/// outer call's filtered rollback: deferred TS2454 state must be consistent.
#[test]
fn nested_overload_deferred_ts2454_consistency() {
    let diags = check(
        r#"
        declare function outer(cb: (x: string) => void): void;
        declare function outer(cb: (x: number) => void): void;
        declare function inner(x: string): string;
        declare function inner(x: number): number;
        outer((x) => {
            let v: number;
            inner(x);
        });
    "#,
    );
    // Must not panic, and TS2454 (if emitted for `v`) should appear at most once.
    let ts2454_count = diags.iter().filter(|d| d.code == 2454).count();
    assert!(
        ts2454_count <= 1,
        "Nested overload should not duplicate TS2454: {diags:?}"
    );
}

/// Overloaded call as a variable initializer with type annotation:
/// exercises the `variable_checking` snapshot interacting with overload
/// `take_speculative_diagnostics` + re-insertion pattern.
#[test]
fn overload_take_and_reinsert_diagnostics_no_duplicate() {
    let diags = check(
        r#"
        declare function parse(x: string): number;
        declare function parse(x: number): string;
        declare function parse(x: boolean): boolean;
        // All three overloads fail for `{}`
        let x: number = parse({} as any);
    "#,
    );
    // The `as any` cast makes it succeed for the first overload.
    // Key invariant: diagnostics from take + re-insertion are not duplicated.
    let total_errors = diags.len();
    assert!(
        total_errors <= 2,
        "Expected bounded diagnostics for take+reinsert, got {total_errors}: {diags:?}"
    );
}

/// Overload resolution where a successful candidate's diagnostics are
/// taken and then the outer context rolls back to an even earlier snapshot.
/// This exercises `take_speculative_diagnostics` followed by outer rollback.
#[test]
fn take_diagnostics_then_outer_rollback_no_panic() {
    let diags = check(
        r#"
        declare function f(x: string): string;
        declare function f(x: number): number;
        declare function g(x: boolean): boolean;
        // Inner: f(42) succeeds on second overload (take its diags)
        // Outer: g(f(42)) — g expects boolean, gets number → TS2345
        g(f(42));
    "#,
    );
    // f(42) resolves to number, g expects boolean → assignability error.
    // The key invariant: no panic from take + outer rollback interaction.
    let error_count = diags
        .iter()
        .filter(|d| d.code == 2345 || d.code == 2769)
        .count();
    assert!(
        error_count >= 1,
        "Expected at least one error for type mismatch: {diags:?}"
    );
    assert!(
        error_count <= 2,
        "Expected bounded error count, got {error_count}: {diags:?}"
    );
}

/// Four sequential overloaded calls: ensures that rollback state from
/// earlier calls doesn't accumulate and corrupt later calls.
#[test]
fn four_sequential_overloaded_calls_no_accumulation() {
    let diags = check(
        r#"
        declare function f(x: string): string;
        declare function f(x: number): number;
        let a = f(1);      // succeeds (second overload)
        let b = f("hi");   // succeeds (first overload)
        let c = f(true);   // fails (TS2769)
        let d = f(1);      // succeeds (second overload)
    "#,
    );
    let ts2769_count = diags.iter().filter(|d| d.code == 2769).count();
    assert_eq!(
        ts2769_count, 1,
        "Exactly one TS2769 expected from f(true), got {ts2769_count}: {diags:?}"
    );
}

/// Overload inside a ternary where both branches call the same overloaded
/// function with different argument types. Tests that speculation rollback
/// in one branch doesn't affect the other.
#[test]
fn overload_in_both_ternary_branches_isolation() {
    let diags = check(
        r#"
        declare function f(x: string): string;
        declare function f(x: number): number;
        declare let cond: boolean;
        let result = cond ? f(42) : f("hello");
    "#,
    );
    // Both branches should resolve cleanly.
    assert!(
        diags.is_empty(),
        "Both ternary branches should resolve overloads cleanly: {diags:?}"
    );
}

/// Regression: overload where the successful candidate has a callback
/// that itself contains a conditional with a dead branch. This exercises
/// three-level nesting: overload → callback → conditional dead branch.
#[test]
fn overload_callback_with_dead_branch_no_leak() {
    let diags = check(
        r#"
        declare function process(cb: (x: string) => string): string;
        declare function process(cb: (x: number) => number): number;
        let result = process((x) => {
            if (false) {
                return "dead" as any;
            }
            return x;
        });
    "#,
    );
    // Should resolve to one of the overloads without leaking dead-branch diagnostics.
    assert!(
        diags.len() <= 2,
        "Expected bounded diagnostics for overload+callback+dead branch, got {}: {diags:?}",
        diags.len()
    );
}
