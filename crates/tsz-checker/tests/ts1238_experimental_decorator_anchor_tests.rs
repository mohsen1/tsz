//! TS1238/TS1270 anchor position under `--experimentalDecorators` for class
//! decorators. tsc anchors a call-resolution failure at the decorator's
//! EXPRESSION (one column past `@`) for every failure kind EXCEPT "too few
//! arguments" (every declared signature needs more than the 1 argument the
//! runtime supplies), which anchors at the whole DECORATOR (`@`) instead —
//! regardless of whether the decorator expression is itself a call
//! expression. Oracle-verified against pinned `typescript@7.0.2`.

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_source;

fn check(source: &str) -> Vec<Diagnostic> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            experimental_decorators: true,
            ..CheckerOptions::default()
        },
    )
}

fn has_diagnostic_at(diagnostics: &[Diagnostic], code: u32, start: usize) -> bool {
    diagnostics
        .iter()
        .any(|diag| diag.code == code && diag.start == start as u32)
}

#[test]
fn ts1238_too_few_args_bare_decorator_anchors_at_whole_decorator() {
    let source = "function d(a: string, b: string, c: string) {} @d class C {}";
    let diagnostics = check(source);
    let at_sign = source.find('@').expect("@ present");
    assert!(
        has_diagnostic_at(&diagnostics, 1238, at_sign),
        "expected TS1238 anchored at `@` ({at_sign}), got: {:?}",
        diagnostics
            .iter()
            .filter(|d| d.code == 1238)
            .collect::<Vec<_>>()
    );
}

#[test]
fn ts1238_too_few_args_factory_call_decorator_anchors_at_whole_decorator() {
    // The decorator expression is itself a call (`@d()`), which returns a
    // function requiring more arguments than the runtime supplies (1, the
    // class constructor). Still anchors at `@`, not at the inner call.
    let source = "function d() { return (a: string, b: string, c: string) => {}; } @d() class C {}";
    let diagnostics = check(source);
    let at_sign = source.find('@').expect("@ present");
    assert!(
        has_diagnostic_at(&diagnostics, 1238, at_sign),
        "expected TS1238 anchored at `@` ({at_sign}), got: {:?}",
        diagnostics
            .iter()
            .filter(|d| d.code == 1238)
            .collect::<Vec<_>>()
    );
}

#[test]
fn ts1238_not_callable_bare_decorator_anchors_at_expression() {
    let source = "const d = 1; @d class C {}";
    let diagnostics = check(source);
    let expr_start = source.rfind('d').expect("decorator identifier present");
    assert!(
        has_diagnostic_at(&diagnostics, 1238, expr_start),
        "expected TS1238 anchored at the expression ({expr_start}), got: {:?}",
        diagnostics
            .iter()
            .filter(|d| d.code == 1238)
            .collect::<Vec<_>>()
    );
}

#[test]
fn ts1238_not_callable_factory_call_decorator_anchors_at_expression_not_at_sign() {
    // Regression: previously a call-expression decorator (`@d()`) always
    // anchored at the whole decorator (`@`), which is wrong for every
    // failure kind except "too few arguments".
    let source = "function d() { return 1; } @d() class C {}";
    let diagnostics = check(source);
    let at_sign = source.find('@').expect("@ present");
    let expr_start = at_sign + 1;
    assert!(
        has_diagnostic_at(&diagnostics, 1238, expr_start),
        "expected TS1238 anchored at the call expression ({expr_start}), got: {:?}",
        diagnostics
            .iter()
            .filter(|d| d.code == 1238)
            .collect::<Vec<_>>()
    );
    assert!(
        !has_diagnostic_at(&diagnostics, 1238, at_sign),
        "TS1238 should not anchor at `@` ({at_sign}) for a non-arity failure"
    );
}

#[test]
fn ts1238_class_used_as_decorator_anchors_at_expression() {
    // No call signatures at all (construct signatures only) — still anchors
    // at the expression, not the whole decorator.
    let source = "class Decorate {} @Decorate class C {}";
    let diagnostics = check(source);
    let expr_start = source.find("Decorate class").expect("second Decorate use");
    assert!(
        has_diagnostic_at(&diagnostics, 1238, expr_start),
        "expected TS1238 anchored at the expression ({expr_start}), got: {:?}",
        diagnostics
            .iter()
            .filter(|d| d.code == 1238)
            .collect::<Vec<_>>()
    );
}

#[test]
fn ts1270_return_type_mismatch_anchors_at_expression() {
    let source = "function d(a: any) { return 5; } @d class C {}";
    let diagnostics = check(source);
    let expr_start = source.rfind('d').expect("decorator identifier present");
    assert!(
        has_diagnostic_at(&diagnostics, 1270, expr_start),
        "expected TS1270 anchored at the expression ({expr_start}), got: {:?}",
        diagnostics
            .iter()
            .filter(|d| d.code == 1270)
            .collect::<Vec<_>>()
    );
}
