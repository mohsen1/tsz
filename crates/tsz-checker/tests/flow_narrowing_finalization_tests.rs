//! Regression tests for the unified flow-narrowing finalization in
//! `get_type_of_node_with_request` and for speculation rollback of the
//! narrowing marker caches (issue #13079).
//!
//! The finalization rules (freshness stripping, literal-widening undo,
//! stable-flow-cache update) previously existed as three drifting copies —
//! one per return path (flow-cache fast path, cached-hit path, computed
//! path) — and the return-type speculation rollback restored `node_types` /
//! `flow_analysis_cache` without the narrowing markers
//! (`flow_narrowed_nodes`, `daa_error_nodes`, `symbol_flow_confirmed`)
//! written by the same pipeline. These tests pin the agreeing behavior
//! across all paths, with positive and negative cases per family.

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;

fn check(source: &str) -> Vec<Diagnostic> {
    tsz_checker::test_utils::check_source(source, "test.ts", CheckerOptions::default())
}

fn check_strict_null(source: &str) -> Vec<Diagnostic> {
    tsz_checker::test_utils::check_source(
        source,
        "test.ts",
        CheckerOptions {
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    )
}

// ---------------------------------------------------------------------------
// Freshness stripping must hold on every return path (zombie freshness).
// ---------------------------------------------------------------------------

/// Repeated reads of a variable bound to an object literal route through the
/// computed path first, then the cached-hit and flow-cache fast paths. None
/// of them may resurrect the initializer's fresh literal type — a fresh type
/// would re-arm excess-property checking on a plain variable reference.
#[test]
fn repeated_variable_reads_do_not_resurrect_freshness() {
    let diags = check(
        r#"
        declare function take(arg: { a: number }): void;
        const source = { a: 1, extra: 2 };
        source;
        take(source);
        take(source);
    "#,
    );
    assert!(
        diags.is_empty(),
        "variable references must not carry freshness: {diags:?}"
    );
}

/// Same shape with renamed binders and an interface annotation, read through
/// an assignment-narrowed union. The narrowed type at the use site comes from
/// the assignment's flow node; finalization must strip freshness there too.
#[test]
fn assignment_narrowed_reference_does_not_resurrect_freshness() {
    let diags = check_strict_null(
        r#"
        interface Wide { width: number; height: number; }
        let target: Wide | undefined;
        target = { width: 10, height: 20 };
        const slim: { width?: number } = target;
        const slimAgain: { width?: number } = target;
    "#,
    );
    assert!(
        diags.is_empty(),
        "assignment-narrowed references must not carry freshness: {diags:?}"
    );
}

/// Negative control: excess-property checking must still fire on a direct
/// object literal — freshness stripping applies to variable references, not
/// to the literal itself.
#[test]
fn direct_object_literal_still_reports_excess_property() {
    let diags = check(
        r#"
        const direct: { a: number } = { a: 1, extra: 2 };
    "#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2353 || d.code == 2322),
        "fresh object literals must keep excess-property errors: {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Literal-widening undo must hold on cached re-reads.
// ---------------------------------------------------------------------------

/// A mutable variable initialized from a declared literal-typed var widens to
/// the primitive. Re-reads through the cached paths must agree with the first
/// (computed) read instead of flip-flopping between literal and primitive.
#[test]
fn literal_widening_is_stable_across_repeated_reads() {
    let diags = check(
        r#"
        declare var tag: "foo";
        let copy = tag;
        copy;
        const first: string = copy;
        const second: string = copy;
    "#,
    );
    assert!(
        diags.is_empty(),
        "widened literal must stay assignable to its primitive on every read: {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Narrowing agreement across repeated reads in one branch.
// ---------------------------------------------------------------------------

/// Repeated uses of a narrowed identifier inside one guard exercise the
/// computed path (first read), the cached-hit path, and the flow-cache fast
/// path (later reads). All must return the narrowed type.
#[test]
fn repeated_reads_in_guard_agree_on_narrowed_type() {
    let diags = check(
        r#"
        function pick(x: string | number) {
            if (typeof x === "string") {
                const a: string = x;
                const b: string = x;
                const c: string = x;
                return a;
            }
            const d: number = x;
            return "fallback";
        }
        const result: string = pick(1);
    "#,
    );
    assert!(
        diags.is_empty(),
        "every read path must observe the same narrowed type: {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Return-type speculation must roll narrowing markers back with the caches.
// ---------------------------------------------------------------------------

/// Return-type inference evaluates the body speculatively without narrowing
/// context, then rolls back and re-checks with proper context. Narrowing
/// markers minted during speculation must not suppress the re-check's
/// narrowing pass.
#[test]
fn inferred_return_type_keeps_guard_narrowing_after_rollback() {
    let diags = check(
        r#"
        function coerce(u: unknown) {
            if (typeof u === "string") {
                return u;
            }
            return "fallback";
        }
        const s: string = coerce(1);
    "#,
    );
    assert!(
        diags.is_empty(),
        "speculative body evaluation must not leak stale narrowing markers: {diags:?}"
    );
}

/// Discriminated-union narrowing inside an inferred-return function: the
/// speculative pass sees no narrowing context, so property accesses on the
/// union would fail there; after rollback, the real pass must narrow cleanly.
#[test]
fn inferred_return_type_keeps_discriminant_narrowing_after_rollback() {
    let diags = check(
        r#"
        type Shape =
            | { kind: "circle"; radius: number }
            | { kind: "square"; side: number };
        function measure(s: Shape) {
            if (s.kind === "circle") {
                return s.radius;
            }
            return s.side;
        }
        const m: number = measure({ kind: "circle", radius: 1 });
    "#,
    );
    assert!(
        diags.is_empty(),
        "discriminant narrowing must survive speculation rollback: {diags:?}"
    );
}

/// TS2454 from a speculative body evaluation rolls back with its
/// definite-assignment markers, then re-emits exactly once in the real pass.
#[test]
fn use_before_assignment_in_inferred_return_emits_single_ts2454() {
    let diags = check_strict_null(
        r#"
        function f() {
            let v: number;
            return v;
        }
    "#,
    );
    let ts2454_count = tsz_checker::test_utils::diagnostic_count(&diags, 2454);
    assert_eq!(
        ts2454_count, 1,
        "expected exactly one TS2454 after speculation rollback: {diags:?}"
    );
}

/// Negative control: assigning before the read clears both the diagnostic and
/// the markers; narrowing of the assigned value must flow into the inferred
/// return type.
#[test]
fn assigned_before_read_has_no_ts2454_and_narrowed_return() {
    let diags = check_strict_null(
        r#"
        function f() {
            let v: number;
            v = 42;
            return v;
        }
        const n: number = f();
    "#,
    );
    assert!(
        diags.is_empty(),
        "assignment before read must produce no diagnostics: {diags:?}"
    );
}
