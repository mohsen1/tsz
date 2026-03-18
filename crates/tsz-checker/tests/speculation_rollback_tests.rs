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
